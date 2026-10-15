//! HTTP + WebSocket server (`local-server`, `query-api`, `security` specs, ADR-0010).

use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::extract::ws::{CloseFrame, Message, WebSocket, WebSocketUpgrade};
use axum::extract::{DefaultBodyLimit, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post};
use axum::{extract::Request, Json, Router};
use futures_util::StreamExt;
use looqlog_core::dto::format_from_str;
use looqlog_core::session::{filters_from, QueryInput};
use looqlog_core::View;

use crate::assets::{render_index, APP_CSS, APP_JS};
use crate::protocol::{ClientMessage, ServerMessage};
use crate::source::{self, lock, Shared};

/// How long a freshly upgraded `/ws` connection has to present the token before it is
/// closed without any log data being sent.
const AUTH_TIMEOUT: Duration = Duration::from_secs(5);

/// Notification coalescing interval (design.md D2): at most one of each per tick.
const NOTIFY_INTERVAL: Duration = Duration::from_millis(100);

pub const TOKEN_HEADER: &str = "x-looqlog-token";
pub const FILENAME_HEADER: &str = "x-looqlog-filename";

pub struct AppState {
    /// Pre-rendered JSON for the `__LOOQLOG_MODE__` placeholder.
    pub mode_json: String,
    /// Per-process random token, embedded in the page and required by `/ws` and
    /// `/upload`. Never placed in a URL.
    pub token: String,
    pub source: Shared,
    /// The address the listener is bound to, for the `Host` check.
    pub bound: SocketAddr,
}

pub fn build_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/", get(index_handler))
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

async fn index_handler(State(state): State<Arc<AppState>>) -> Html<String> {
    Html(render_index(&state.mode_json, &state.token))
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

/// `POST /upload` (`file-ingest` spec, design.md D4): checked before a byte of the body
/// is read, then streamed into a fresh session that replaces the current source once
/// the first chunk has arrived. A failure before that leaves the previous source intact.
async fn upload_handler(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    body: Body,
) -> Response {
    let host = headers.get(header::HOST);
    let token_ok = headers
        .get(TOKEN_HEADER)
        .and_then(|t| t.to_str().ok())
        .is_some_and(|t| t == state.token);
    let origin_ok = headers
        .get(header::ORIGIN)
        .is_some_and(|o| origin_matches_host(o, host));
    if !token_ok || !origin_ok || !host_allowed(host, state.bound) {
        tracing::warn!("rejected /upload: token, Origin or Host check failed");
        return upload_error(StatusCode::FORBIDDEN, "forbidden");
    }
    if lock(&state.source).stdin {
        return upload_error(
            StatusCode::CONFLICT,
            "this looqlog process is showing a stdin stream; it does not accept file uploads",
        );
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
    let my_id = {
        let mut src = lock(&state.source);
        let id = src.replace(Some(name.clone()), declared);
        src.session.ingest(&first);
        id
    };
    let mut received = first.len() as u64;
    while let Some(chunk) = stream.next().await {
        let mut src = lock(&state.source);
        if src.id != my_id {
            return upload_error(
                StatusCode::CONFLICT,
                format!("{name} was replaced by another file"),
            );
        }
        match chunk {
            Ok(chunk) => {
                received += chunk.len() as u64;
                if received > source::CAP_BYTES {
                    let message = source::cap_error(&name, received);
                    src.error = Some(message.clone());
                    src.session.finish();
                    return upload_error(StatusCode::PAYLOAD_TOO_LARGE, message);
                }
                src.session.ingest(&chunk);
            }
            Err(err) => {
                let message = format!("reading {name} failed: {err}");
                src.error = Some(message.clone());
                src.session.finish();
                return upload_error(StatusCode::BAD_REQUEST, message);
            }
        }
    }
    let mut src = lock(&state.source);
    if src.id != my_id {
        return upload_error(
            StatusCode::CONFLICT,
            format!("{name} was replaced by another file"),
        );
    }
    src.total_bytes = Some(received);
    src.session.finish();
    Json(serde_json::json!({ "entries": src.session.total() })).into_response()
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
/// `{"type":"auth","token":...}` with the per-process token. Anything else — including
/// a query sent first — fails, and nothing is ever sent on that socket.
async fn authenticate(socket: &mut WebSocket, expected_token: &str) -> bool {
    let result = tokio::time::timeout(AUTH_TIMEOUT, async {
        loop {
            match socket.recv().await {
                Some(Ok(Message::Text(text))) => {
                    return matches!(
                        serde_json::from_str::<ClientMessage>(&text),
                        Ok(ClientMessage::Auth { token }) if token == expected_token
                    );
                }
                Some(Ok(Message::Close(_))) | None => return false,
                Some(Ok(_)) => continue,
                Some(Err(_)) => return false,
            }
        }
    })
    .await;
    matches!(result, Ok(true))
}

/// What this connection was last told, so each tick sends only what changed.
#[derive(Default)]
struct Sent {
    source: Option<(u64, u64)>,
    revision: Option<u64>,
    counts: Option<(usize, usize, usize)>,
    evicted: u64,
    progress: Option<(u64, u64, bool)>,
    ended: bool,
}

/// One connection's query state: its view and the source it was built against.
struct Conn {
    view: View,
    source_id: u64,
    sent: Sent,
}

impl Conn {
    fn sync_source(&mut self, src_id: u64) {
        if self.source_id != src_id {
            self.view = View::new();
            self.source_id = src_id;
        }
    }
}

/// `/ws` connection loop (`query-api` spec): authenticate, then answer requests and
/// push notifications coalesced to [`NOTIFY_INTERVAL`] with cumulative values. The first
/// notifications right after authentication carry the current totals, so a late or
/// reconnecting client positions itself without a snapshot transfer.
async fn handle_socket(mut socket: WebSocket, state: Arc<AppState>) {
    if !authenticate(&mut socket, &state.token).await {
        let _ = socket
            .send(Message::Close(Some(CloseFrame {
                code: 4001,
                reason: "authentication required".into(),
            })))
            .await;
        return;
    }

    let initial_id = lock(&state.source).id;
    let mut conn = Some(Conn {
        view: View::new(),
        source_id: initial_id,
        sent: Sent::default(),
    });
    let mut tick = tokio::time::interval(NOTIFY_INTERVAL);
    loop {
        tokio::select! {
            _ = tick.tick() => {
                let shared = state.source.clone();
                let mut c = conn.take().expect("conn present between steps");
                let Ok((c, messages)) = tokio::task::spawn_blocking(move || {
                    let messages = notifications(&shared, &mut c);
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
                let text = match incoming {
                    Some(Ok(Message::Text(text))) => text,
                    Some(Ok(Message::Close(_))) | None | Some(Err(_)) => return,
                    Some(Ok(_)) => continue,
                };
                let request = match serde_json::from_str::<ClientMessage>(&text) {
                    Ok(request) => request,
                    Err(err) => {
                        tracing::warn!("unparsable /ws request: {err}");
                        continue;
                    }
                };
                let Some(id) = request.id() else {
                    continue; // a second auth frame is harmless
                };
                let shared = state.source.clone();
                let mut c = conn.take().expect("conn present between steps");
                let Ok((c, reply)) = tokio::task::spawn_blocking(move || {
                    let reply = dispatch(&shared, &mut c, id, request);
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

fn dispatch(shared: &Shared, conn: &mut Conn, id: u64, request: ClientMessage) -> ServerMessage {
    let mut src = lock(shared);
    conn.sync_source(src.id);
    let view = &mut conn.view;
    match request {
        ClientMessage::Auth { .. } => unreachable!("auth carries no id"),
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
            match src.session.set_format(format, tz_offset_minutes) {
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
fn notifications(shared: &Shared, conn: &mut Conn) -> Vec<ServerMessage> {
    let mut src = lock(shared);
    let mut out = Vec::new();
    let current = (src.id, src.session.epoch());
    if conn.sent.source != Some(current) {
        if conn.sent.source.is_some() {
            out.push(ServerMessage::Reset);
            conn.sent.counts = None;
        }
        conn.sync_source(src.id);
        conn.sent.source = Some(current);
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
