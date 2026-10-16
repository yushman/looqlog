//! HTTP + WebSocket server (`local-server`, `query-api`, `security` specs, ADR-0010).

use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use axum::body::Body;
use axum::extract::ws::{CloseFrame, Message, WebSocket, WebSocketUpgrade};
use axum::extract::{DefaultBodyLimit, Path, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post};
use axum::{extract::Request, Json, Router};
use futures_util::StreamExt;
use looqlog_core::dto::format_from_str;
use looqlog_core::session::{filters_from, QueryInput, TzOverride};
use looqlog_core::View;

use crate::assets::{render_index, APP_CSS, APP_JS};
use crate::protocol::{ClientMessage, ServerMessage};
use crate::source::{self, lock, CloseError, CloseReason, Kind, Registry, Shared, Source};

/// How long a freshly upgraded `/ws` connection has to present the token before it is
/// closed without any log data being sent.
const AUTH_TIMEOUT: Duration = Duration::from_secs(5);

/// Notification coalescing interval (design.md D2): at most one of each per tick.
const NOTIFY_INTERVAL: Duration = Duration::from_millis(100);

pub const TOKEN_HEADER: &str = "x-looqlog-token";
pub const FILENAME_HEADER: &str = "x-looqlog-filename";

pub struct AppState {
    /// Per-process random token, embedded in the page and required by `/ws`, `/upload`
    /// and close. Never placed in a URL.
    pub token: String,
    pub registry: Arc<Mutex<Registry>>,
    /// `--max-lines`, for a stdin source's page.
    pub max_lines: usize,
    /// The address the listener is bound to, for the `Host` check.
    pub bound: SocketAddr,
}

impl AppState {
    pub fn registry(&self) -> MutexGuard<'_, Registry> {
        self.registry.lock().expect("registry lock poisoned")
    }
}

pub fn build_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/", get(landing_handler))
        .route("/l/{id}", get(source_page_handler))
        .route("/sources/{id}/close", post(close_handler))
        .route("/assets/index.js", get(app_js_handler))
        .route("/assets/index.css", get(app_css_handler))
        .route("/ws", get(ws_handler))
        .route(
            "/upload",
            // The body is streamed and capped by `source::CAP_BYTES`, not buffered.
            post(upload_handler).layer(DefaultBodyLimit::disable()),
        )
        .with_state(state)
        .layer(middleware::from_fn(add_security_headers))
}

/// `security` spec, "Content Security Policy on every response": same-origin scripts,
/// styles and the `/ws` connection, nothing else — no WASM allowance, no inline styles.
async fn add_security_headers(request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    response.headers_mut().insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static("default-src 'self'; style-src 'self'"),
    );
    response
}

async fn landing_handler(State(state): State<Arc<AppState>>) -> Html<String> {
    Html(render_index(r#"{"mode":"landing"}"#, &state.token))
}

/// Served for any id, known or not (`local-server` spec): the page itself renders a
/// closed or unknown source from what `/ws` says.
async fn source_page_handler(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Html<String> {
    let stdin = state
        .registry()
        .get(&id)
        .is_some_and(|shared| lock(&shared).kind == Kind::Stdin);
    let mode = if stdin {
        format!(r#"{{"mode":"stdin","maxLines":{}}}"#, state.max_lines)
    } else {
        r#"{"mode":"file"}"#.to_string()
    };
    Html(render_index(&mode, &state.token))
}

async fn app_js_handler() -> impl IntoResponse {
    ([(header::CONTENT_TYPE, "application/javascript")], APP_JS)
}

async fn app_css_handler() -> impl IntoResponse {
    ([(header::CONTENT_TYPE, "text/css")], APP_CSS)
}

/// `Host` must name the bound address — an IP literal (any, when bound to an unspecified
/// address) or `localhost` on a loopback bind — with the bound port. A DNS-rebound name
/// such as `attacker.example` never passes.
fn host_allowed(host: Option<&HeaderValue>, bound: SocketAddr) -> bool {
    let Some(host) = host.and_then(|h| h.to_str().ok()) else {
        return false;
    };
    let (name, port) = match host.rsplit_once(':') {
        Some((name, port)) if !name.ends_with(':') => (name, port),
        _ => return false,
    };
    if port.parse::<u16>().ok() != Some(bound.port()) {
        return false;
    }
    let name = name.trim_start_matches('[').trim_end_matches(']');
    if name.eq_ignore_ascii_case("localhost") {
        return bound.ip().is_loopback();
    }
    match name.parse::<IpAddr>() {
        Ok(ip) => bound.ip().is_unspecified() || ip == bound.ip(),
        Err(_) => false,
    }
}

/// The `Origin` authority must equal the request's own `Host`.
fn origin_matches_host(origin: &HeaderValue, host: Option<&HeaderValue>) -> bool {
    let (Some(host), Ok(origin_str)) = (host, origin.to_str()) else {
        return false;
    };
    let Ok(host_str) = host.to_str() else {
        return false;
    };
    let origin_authority = origin_str.split("://").nth(1).unwrap_or(origin_str);
    origin_authority.eq_ignore_ascii_case(host_str)
}

/// `security` spec, "WebSocket origin check". A browser always sends `Origin` on a
/// WebSocket handshake; a missing one means a non-browser client, which the token
/// handshake still gates.
async fn ws_handler(
    ws: WebSocketUpgrade,
    headers: HeaderMap,
    State(state): State<Arc<AppState>>,
) -> Response {
    let host = headers.get(header::HOST);
    if !host_allowed(host, state.bound) {
        tracing::warn!("rejected /ws upgrade: Host does not name the bound address");
        return (StatusCode::FORBIDDEN, "host not allowed").into_response();
    }
    if let Some(origin) = headers.get(header::ORIGIN) {
        if !origin_matches_host(origin, host) {
            tracing::warn!("rejected /ws upgrade: Origin does not match Host");
            return (StatusCode::FORBIDDEN, "origin mismatch").into_response();
        }
    }
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

fn upload_error(status: StatusCode, message: impl Into<String>) -> Response {
    (status, Json(serde_json::json!({ "error": message.into() }))).into_response()
}

/// Token, `Origin` and `Host` checks shared by `/upload` and close (`security` spec).
fn post_allowed(state: &AppState, headers: &HeaderMap) -> bool {
    let host = headers.get(header::HOST);
    let token_ok = headers
        .get(TOKEN_HEADER)
        .and_then(|t| t.to_str().ok())
        .is_some_and(|t| t == state.token);
    let origin_ok = headers
        .get(header::ORIGIN)
        .is_some_and(|o| origin_matches_host(o, host));
    token_ok && origin_ok && host_allowed(host, state.bound)
}

/// Drops an upload's partial source unless it completed — including when the client
/// goes away and the handler is dropped mid-body (`file-ingest` spec).
struct PartialUpload {
    state: Arc<AppState>,
    id: String,
    reason: Option<CloseReason>,
}

impl Drop for PartialUpload {
    fn drop(&mut self) {
        if let Some(reason) = self.reason {
            self.state.registry().remove(&self.id, reason);
        }
    }
}

/// `POST /upload` (`file-ingest` spec, design.md D4): checked before a byte of the body
/// is read, then streamed into a new source. Any failure drops that source and leaves
/// every other one as it was.
async fn upload_handler(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    body: Body,
) -> Response {
    if !post_allowed(&state, &headers) {
        tracing::warn!("rejected /upload: token, Origin or Host check failed");
        return upload_error(StatusCode::FORBIDDEN, "forbidden");
    }
    let name = headers
        .get(FILENAME_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(percent_decode)
        .unwrap_or_else(|| "uploaded file".to_string());
    let declared = headers
        .get(header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok());
    if let Some(size) = declared.filter(|&s| s > source::CAP_BYTES) {
        return upload_error(
            StatusCode::PAYLOAD_TOO_LARGE,
            source::cap_error(&name, size),
        );
    }

    let mut stream = body.into_data_stream();
    let first = match stream.next().await {
        None => return upload_error(StatusCode::BAD_REQUEST, format!("{name} is empty")),
        Some(Err(err)) => {
            return upload_error(
                StatusCode::BAD_REQUEST,
                format!("reading {name} failed: {err}"),
            )
        }
        Some(Ok(chunk)) => chunk,
    };
    let mut new_source = Source::new(Kind::Upload, name.clone(), None);
    new_source.total_bytes = declared;
    new_source.warning = declared.and_then(source::size_warning);
    let (id, shared) = state.registry().add(new_source);
    let mut partial = PartialUpload {
        state: state.clone(),
        id: id.clone(),
        reason: Some(CloseReason::Closed),
    };
    let mut received = 0u64;
    let mut next = Some(Ok(first));
    while let Some(chunk) = next {
        let chunk = match chunk {
            Ok(chunk) => chunk,
            Err(err) => {
                return upload_error(
                    StatusCode::BAD_REQUEST,
                    format!("reading {name} failed: {err}"),
                )
            }
        };
        received += chunk.len() as u64;
        if received > source::CAP_BYTES {
            return upload_error(
                StatusCode::PAYLOAD_TOO_LARGE,
                source::cap_error(&name, received),
            );
        }
        let retained = {
            let mut src = lock(&shared);
            if src.closed.is_some() {
                return upload_error(StatusCode::CONFLICT, format!("{name} was closed"));
            }
            src.session.ingest(&chunk);
            src.session.retained_bytes()
        };
        if let Err(message) = state.registry().charge(&id, retained) {
            partial.reason = Some(CloseReason::Budget);
            return upload_error(StatusCode::INSUFFICIENT_STORAGE, message);
        }
        next = stream.next().await;
    }
    let retained = {
        let mut src = lock(&shared);
        src.total_bytes = Some(received);
        src.session.finish();
        src.session.retained_bytes()
    };
    let charged = {
        let mut registry = state.registry();
        let charged = registry.charge(&id, retained);
        if charged.is_ok() {
            registry.upload_done(&id, Instant::now());
        }
        charged
    };
    if let Err(message) = charged {
        partial.reason = Some(CloseReason::Budget);
        return upload_error(StatusCode::INSUFFICIENT_STORAGE, message);
    }
    partial.reason = None;
    (StatusCode::CREATED, Json(serde_json::json!({ "id": id }))).into_response()
}

/// `POST /sources/{id}/close`: the same checks as `/upload`; stdin is refused.
async fn close_handler(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Response {
    if !post_allowed(&state, &headers) {
        tracing::warn!("rejected close: token, Origin or Host check failed");
        return upload_error(StatusCode::FORBIDDEN, "forbidden");
    }
    match state.registry().close(&id) {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(err @ CloseError::Stdin) => upload_error(StatusCode::CONFLICT, err.message()),
        Err(err @ CloseError::Unknown) => upload_error(StatusCode::NOT_FOUND, err.message()),
    }
}

/// Decodes `%XX` escapes (the page sends the name through `encodeURIComponent`).
fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = |b: u8| (b as char).to_digit(16);
            if let (Some(hi), Some(lo)) = (hex(bytes[i + 1]), hex(bytes[i + 2])) {
                out.push((hi * 16 + lo) as u8);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// `security` spec, "WebSocket token handshake": the first text frame must be
/// `{"type":"auth","token":...,"source":...}` with the per-process token. Anything else —
/// including a query sent first — fails, and nothing is ever sent on that socket. On
/// success, returns the source the connection names (`None` for the landing).
async fn authenticate(socket: &mut WebSocket, expected_token: &str) -> Option<Option<String>> {
    let result = tokio::time::timeout(AUTH_TIMEOUT, async {
        loop {
            match socket.recv().await {
                Some(Ok(Message::Text(text))) => {
                    return match serde_json::from_str::<ClientMessage>(&text) {
                        Ok(ClientMessage::Auth { token, source }) if token == expected_token => {
                            Some(source)
                        }
                        _ => None,
                    };
                }
                Some(Ok(Message::Close(_))) | None => return None,
                Some(Ok(_)) => continue,
                Some(Err(_)) => return None,
            }
        }
    })
    .await;
    result.ok().flatten()
}

/// What this connection was last told, so each tick sends only what changed.
#[derive(Default)]
struct Sent {
    epoch: Option<u64>,
    revision: Option<u64>,
    counts: Option<(usize, usize, usize)>,
    evicted: u64,
    progress: Option<(u64, u64, bool)>,
    ended: bool,
}

/// One connection's query state: its source (`None` once it is gone) and its view.
struct Conn {
    source: Option<Shared>,
    view: View,
    sent: Sent,
}

/// Keeps the source's tab count for as long as the connection lives.
struct Tab {
    state: Arc<AppState>,
    id: String,
}

impl Drop for Tab {
    fn drop(&mut self) {
        self.state.registry().detach(&self.id, Instant::now());
    }
}

/// `/ws` connection loop (`query-api` spec): authenticate, then serve either the landing
/// (the source list) or one source.
async fn handle_socket(mut socket: WebSocket, state: Arc<AppState>) {
    let Some(source) = authenticate(&mut socket, &state.token).await else {
        let _ = socket
            .send(Message::Close(Some(CloseFrame {
                code: 4001,
                reason: "authentication required".into(),
            })))
            .await;
        return;
    };
    match source {
        Some(id) => serve_source(socket, state, id).await,
        None => serve_landing(socket, state).await,
    }
}

fn no_source_error(id: u64) -> ServerMessage {
    ServerMessage::Error {
        id,
        message: "this connection has no source".into(),
    }
}

/// A landing connection: the source list after auth and whenever it changes, and
/// `close` requests.
async fn serve_landing(mut socket: WebSocket, state: Arc<AppState>) {
    let mut sent = None;
    let mut tick = tokio::time::interval(NOTIFY_INTERVAL);
    loop {
        tokio::select! {
            _ = tick.tick() => {
                let registry = state.registry.clone();
                let Ok(rows) = tokio::task::spawn_blocking(move || {
                    registry.lock().expect("registry lock poisoned").rows()
                })
                .await else {
                    return;
                };
                if sent.as_ref() != Some(&rows) {
                    let msg = ServerMessage::Sources { sources: rows.clone() };
                    if send_json(&mut socket, &msg).await.is_err() {
                        return;
                    }
                    sent = Some(rows);
                }
            }
            incoming = socket.recv() => {
                let Some(request) = next_request(incoming) else {
                    return;
                };
                let Some(request) = request else {
                    continue;
                };
                let Some(id) = request.id() else {
                    continue;
                };
                let reply = match request {
                    ClientMessage::Close { source, .. } => match state.registry().close(&source) {
                        Ok(()) => reply(id, serde_json::json!({})),
                        Err(err) => ServerMessage::Error { id, message: err.message().into() },
                    },
                    _ => no_source_error(id),
                };
                if send_json(&mut socket, &reply).await.is_err() {
                    return;
                }
            }
        }
    }
}

/// `None`: the connection is over. `Some(None)`: a frame to skip.
fn next_request(incoming: Option<Result<Message, axum::Error>>) -> Option<Option<ClientMessage>> {
    let text = match incoming {
        Some(Ok(Message::Text(text))) => text,
        Some(Ok(Message::Close(_))) | None | Some(Err(_)) => return None,
        Some(Ok(_)) => return Some(None),
    };
    match serde_json::from_str::<ClientMessage>(&text) {
        Ok(request) => Some(Some(request)),
        Err(err) => {
            tracing::warn!("unparsable /ws request: {err}");
            Some(None)
        }
    }
}

/// A tab's connection to one source: answer requests and push notifications coalesced to
/// [`NOTIFY_INTERVAL`] with cumulative values. The first notifications right after
/// authentication carry the current totals, so a late or reconnecting client positions
/// itself without a snapshot transfer. An unknown or gone source gets `closed` instead.
async fn serve_source(mut socket: WebSocket, state: Arc<AppState>, source_id: String) {
    let attached = state.registry().attach(&source_id);
    let _tab = attached.is_ok().then(|| Tab {
        state: state.clone(),
        id: source_id.clone(),
    });
    let mut conn = Some(Conn {
        source: attached.as_ref().ok().cloned(),
        view: View::new(),
        sent: Sent::default(),
    });
    if let Err(reason) = attached {
        if send_json(&mut socket, &ServerMessage::Closed { reason })
            .await
            .is_err()
        {
            return;
        }
    }
    let mut tick = tokio::time::interval(NOTIFY_INTERVAL);
    loop {
        tokio::select! {
            _ = tick.tick() => {
                let mut c = conn.take().expect("conn present between steps");
                let Ok((c, messages)) = tokio::task::spawn_blocking(move || {
                    let messages = notifications(&mut c);
                    (c, messages)
                })
                .await else {
                    return;
                };
                conn = Some(c);
                for msg in messages {
                    if send_json(&mut socket, &msg).await.is_err() {
                        return;
                    }
                }
            }
            incoming = socket.recv() => {
                let Some(request) = next_request(incoming) else {
                    return;
                };
                let Some(request) = request else {
                    continue;
                };
                let Some(id) = request.id() else {
                    continue; // a second auth frame is harmless
                };
                let mut c = conn.take().expect("conn present between steps");
                let registry = state.registry.clone();
                let source_id = source_id.clone();
                let Ok((c, reply)) = tokio::task::spawn_blocking(move || {
                    let reformat = matches!(request, ClientMessage::SetFormat { .. });
                    let reply = dispatch(&mut c, id, request);
                    if let (true, Some(shared)) = (reformat, &c.source) {
                        let retained = lock(shared).session.retained_bytes();
                        registry
                            .lock()
                            .expect("registry lock poisoned")
                            .record(&source_id, retained);
                    }
                    (c, reply)
                })
                .await else {
                    return;
                };
                conn = Some(c);
                if send_json(&mut socket, &reply).await.is_err() {
                    return;
                }
            }
        }
    }
}

fn reply(id: u64, data: impl serde::Serialize) -> ServerMessage {
    ServerMessage::Reply {
        id,
        data: serde_json::to_value(data).expect("reply data serializes"),
    }
}

fn dispatch(conn: &mut Conn, id: u64, request: ClientMessage) -> ServerMessage {
    let Some(shared) = conn.source.clone() else {
        return ServerMessage::Error {
            id,
            message: "this source is closed".into(),
        };
    };
    let mut src = lock(&shared);
    if src.closed.is_some() {
        return ServerMessage::Error {
            id,
            message: "this source is closed".into(),
        };
    }
    let view = &mut conn.view;
    match request {
        ClientMessage::Auth { .. } => unreachable!("auth carries no id"),
        ClientMessage::Close { .. } => ServerMessage::Error {
            id,
            message: "close is a request of the source list, not of a source tab".into(),
        },
        ClientMessage::Query {
            filters,
            search,
            range,
            toggled,
            ..
        } => reply(
            id,
            src.session.set_query(
                view,
                QueryInput {
                    filters: filters_from(filters),
                    search,
                    range,
                    toggled,
                },
            ),
        ),
        ClientMessage::Window {
            offset,
            limit,
            anchor,
            ..
        } => {
            let offset = match anchor {
                Some(ordinal) => src.session.locate(view, ordinal),
                None => offset,
            };
            reply(id, src.session.window(view, offset, limit))
        }
        ClientMessage::Entry { ordinal, .. } => reply(id, src.session.entry(view, ordinal)),
        ClientMessage::Buckets {
            span, full_span, ..
        } => reply(id, src.session.buckets(view, span, full_span)),
        ClientMessage::ToggleChain { root, .. } => reply(
            id,
            serde_json::json!({ "rowCount": src.session.toggle_chain(view, root) }),
        ),
        ClientMessage::SetFormat {
            format,
            tz_offset_minutes,
            tz_name,
            ..
        } => {
            let format = match format.as_deref() {
                None | Some("auto") => None,
                Some(raw) => match format_from_str(raw) {
                    Some(f) => Some(f),
                    None => {
                        return ServerMessage::Error {
                            id,
                            message: format!("unknown format: {raw}"),
                        }
                    }
                },
            };
            let tz = match (tz_name, tz_offset_minutes) {
                (Some(_), Some(_)) => {
                    return ServerMessage::Error {
                        id,
                        message: "tzOffsetMinutes and tzName are mutually exclusive".into(),
                    }
                }
                (Some(name), None) => Some(TzOverride::Name(name)),
                (None, minutes) => minutes.map(TzOverride::OffsetMinutes),
            };
            match src.session.set_format(format, tz) {
                Ok(stats) => reply(id, stats),
                Err(message) => ServerMessage::Error { id, message },
            }
        }
        ClientMessage::Stats { .. } => reply(
            id,
            serde_json::json!({ "source": src.info(), "stats": src.session.stats() }),
        ),
    }
}

/// Everything that changed since this connection was last told, as cumulative values.
/// A source that went away is announced once and then let go of.
fn notifications(conn: &mut Conn) -> Vec<ServerMessage> {
    let Some(shared) = conn.source.clone() else {
        return Vec::new();
    };
    let mut src = lock(&shared);
    if let Some(reason) = src.closed {
        drop(src);
        conn.source = None;
        conn.view = View::new();
        return vec![ServerMessage::Closed { reason }];
    }
    let mut out = Vec::new();
    let epoch = src.session.epoch();
    if conn.sent.epoch != Some(epoch) {
        if conn.sent.epoch.is_some() {
            out.push(ServerMessage::Reset);
            conn.sent.counts = None;
        }
        conn.sent.epoch = Some(epoch);
    }
    if let Some(total) = src.total_bytes {
        let progress = (
            src.session.bytes_ingested(),
            total.max(src.session.bytes_ingested()),
            src.session.is_finished() || src.error.is_some(),
        );
        if conn.sent.progress != Some(progress) {
            out.push(ServerMessage::Progress {
                bytes: progress.0,
                total: progress.1,
                done: progress.2,
            });
            conn.sent.progress = Some(progress);
        }
    }
    let revision = src.session.revision();
    if conn.sent.revision != Some(revision) || conn.sent.counts.is_none() {
        let (matched, row_count) = src.session.counts(&mut conn.view);
        let counts = (src.session.total(), matched, row_count);
        if conn.sent.counts != Some(counts) {
            out.push(ServerMessage::Grown {
                total: counts.0,
                matched,
                row_count,
            });
            conn.sent.counts = Some(counts);
        }
        conn.sent.revision = Some(revision);
    }
    let evicted = src.session.evicted_lines();
    if evicted != conn.sent.evicted {
        out.push(ServerMessage::Evicted { count: evicted });
        conn.sent.evicted = evicted;
    }
    if src.ended && !conn.sent.ended {
        out.push(ServerMessage::Ended);
        conn.sent.ended = true;
    }
    out
}

async fn send_json(socket: &mut WebSocket, msg: &ServerMessage) -> Result<(), axum::Error> {
    let json = serde_json::to_string(msg).expect("ServerMessage always serializes");
    socket.send(Message::Text(json.into())).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hv(s: &str) -> HeaderValue {
        HeaderValue::from_str(s).unwrap()
    }

    #[test]
    fn host_must_name_the_bound_address_and_port() {
        let lo: SocketAddr = "127.0.0.1:7891".parse().unwrap();
        assert!(host_allowed(Some(&hv("127.0.0.1:7891")), lo));
        assert!(host_allowed(Some(&hv("localhost:7891")), lo));
        assert!(!host_allowed(Some(&hv("attacker.example:7891")), lo));
        assert!(!host_allowed(Some(&hv("127.0.0.1:80")), lo));
        assert!(!host_allowed(Some(&hv("10.0.0.2:7891")), lo));
        assert!(!host_allowed(None, lo));
        let any: SocketAddr = "0.0.0.0:7891".parse().unwrap();
        assert!(host_allowed(Some(&hv("192.168.1.5:7891")), any));
        assert!(!host_allowed(Some(&hv("localhost:7891")), any));
        assert!(!host_allowed(Some(&hv("evil.test:7891")), any));
        let v6: SocketAddr = "[::1]:7891".parse().unwrap();
        assert!(host_allowed(Some(&hv("[::1]:7891")), v6));
    }

    #[test]
    fn percent_decoding_restores_the_file_name() {
        assert_eq!(percent_decode("app%20log%E2%9C%93.txt"), "app log✓.txt");
        assert_eq!(percent_decode("plain.log"), "plain.log");
        assert_eq!(percent_decode("bad%zz"), "bad%zz");
    }
}
