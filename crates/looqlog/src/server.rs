//! HTTP + WebSocket server (`local-server`, `stdin-stream`, `security` specs).

use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::extract::ws::{CloseFrame, Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::get;
use axum::{extract::Request, Router};
use tokio::sync::broadcast;

use crate::assets::{render_index, APP_CSS, APP_JS, CORE_JS, CORE_WASM, WORKER_JS};
use crate::protocol::{ClientMessage, ServerMessage, SnapshotLine};
use crate::stdin::{self, StdinBuffer, StdinEvent};

/// How long a freshly-upgraded `/ws` connection has to present a valid auth token
/// (`security` spec, "WebSocket token handshake") before it is closed without any
/// stdin data being sent. Generous enough that the page's own JS (which sends the
/// token as its very first action after `onopen`) never trips it under normal load,
/// short enough that a connection that never authenticates doesn't linger.
const AUTH_TIMEOUT: Duration = Duration::from_secs(5);

pub struct AppState {
    /// Pre-rendered HTML fragment for the `__LOOQLOG_HINT__` placeholder (already
    /// escaped; see `assets::html_escape`).
    pub hint_html: String,
    /// Pre-rendered JSON for the `__LOOQLOG_MODE__` placeholder (`{"mode":"file"}` or
    /// `{"mode":"stdin","maxLines":N}`), read by `looqlog-app.ts` to decide which UI to
    /// mount (`app-shell`/`live-tail-ui` specs).
    pub mode_json: String,
    /// Per-process random token (`security` spec, design.md D1), embedded verbatim
    /// into the served page and required as the first `/ws` message before any
    /// stdin data streams. Generated once at startup in `main.rs`; the same value
    /// is served to every page load, including reloads, for the life of the
    /// process — never placed in a URL or query string.
    pub token: String,
    /// `None` in file mode: there is nothing to subscribe to (`/ws` still exists —
    /// see `local-server` spec's route enumeration — but no lines will ever arrive).
    pub stdin_tx: Option<broadcast::Sender<StdinEvent>>,
    /// The shared ring buffer (ADR-0004), read once per connection for the
    /// snapshot. `Some` exactly when `stdin_tx` is `Some`.
    pub stdin_buffer: Option<Arc<Mutex<StdinBuffer>>>,
}

/// Route layout mirrors the Vite build output (`assets.rs` doc comment): the app
/// bundle and worker chunk under `/assets/`, the wasm-bindgen glue and `core.wasm`
/// under `/wasm/`. All served from memory — the server never reads these from the
/// filesystem at request time (`local-server` spec).
pub fn build_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/", get(index_handler))
        .route("/assets/index.js", get(app_js_handler))
        .route("/assets/index.css", get(app_css_handler))
        .route("/assets/worker.js", get(worker_js_handler))
        .route("/wasm/core.js", get(core_js_handler))
        .route("/wasm/core.wasm", get(core_wasm_handler))
        .route("/ws", get(ws_handler))
        .with_state(state)
        .layer(middleware::from_fn(add_security_headers))
}

/// `security` spec, "Content Security Policy on every response". `default-src
/// 'self'` covers `connect-src`/`worker-src` by fallback (same-origin `/ws` and the
/// `assets/worker.js` Worker need nothing extra). One deliberate addition beyond
/// bare `default-src 'self'` (design.md D2 — verified against a real Chromium
/// instance via Playwright, not assumed):
///
/// - `'wasm-unsafe-eval'` on `script-src`: some browser engines still gate
///   `WebAssembly.instantiate`/`instantiateStreaming` behind an explicit
///   script-src allowance even for same-origin, streaming-compiled modules;
///   `'wasm-unsafe-eval'` grants exactly that (WASM compilation only — unlike
///   `'unsafe-eval'`, it does not permit `eval()`/`new Function()` on arbitrary
///   strings).
///
/// `style-src` stays as strict as `default-src`: the virtual-scrolled entry table
/// inserts its row-positioning rules into the already-linked `/assets/index.css`
/// through the CSS object model instead of writing `style` attributes
/// (`looqlog-entry-table.ts`, `resizable-table-columns` design D2), so nothing on the
/// page needs `'unsafe-inline'`.
///
/// With this policy the WASM module compiles, the worker starts, the table renders
/// and scrolls its full dataset, and no CSP violation is reported in the console.
/// Firefox and Safari are PRD §11 targets this sandbox cannot drive interactively —
/// recorded as an environment limitation in docs/devlog.md, not silently assumed to
/// pass.
async fn add_security_headers(request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    response.headers_mut().insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(
            "default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; style-src 'self'",
        ),
    );
    response
}

async fn index_handler(State(state): State<Arc<AppState>>) -> Html<String> {
    Html(render_index(
        &state.hint_html,
        &state.mode_json,
        &state.token,
    ))
}

async fn app_js_handler() -> impl IntoResponse {
    ([(header::CONTENT_TYPE, "application/javascript")], APP_JS)
}

async fn app_css_handler() -> impl IntoResponse {
    ([(header::CONTENT_TYPE, "text/css")], APP_CSS)
}

async fn worker_js_handler() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "application/javascript")],
        WORKER_JS,
    )
}

async fn core_js_handler() -> impl IntoResponse {
    ([(header::CONTENT_TYPE, "application/javascript")], CORE_JS)
}

async fn core_wasm_handler() -> impl IntoResponse {
    ([(header::CONTENT_TYPE, "application/wasm")], CORE_WASM)
}

/// `security` spec, "WebSocket origin check": rejects a cross-origin upgrade
/// attempt before any WebSocket handshake completes and before any stdin data can
/// flow. A browser always sends `Origin` on a WebSocket handshake, same-origin or
/// not; a missing `Origin` header means the client isn't a browser page at all
/// (e.g. `wscat`, this crate's own integration tests) and is let through — the
/// threat this check closes is specifically "a page on another origin", which is
/// exactly the case that always carries an `Origin` header. The token handshake
/// below is the second, independent gate that applies regardless of `Origin`.
async fn ws_handler(
    ws: WebSocketUpgrade,
    headers: HeaderMap,
    State(state): State<Arc<AppState>>,
) -> Response {
    if let Some(origin) = headers.get(header::ORIGIN) {
        if !origin_matches_host(origin, headers.get(header::HOST)) {
            tracing::warn!("rejected /ws upgrade: Origin does not match Host");
            return (StatusCode::FORBIDDEN, "origin mismatch").into_response();
        }
    }
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

/// Compares the `Origin` header's `scheme://host[:port]` authority against the
/// `Host` header's `host[:port]` of the same request — deliberately relative to
/// what the browser actually used to reach this server (the `Host` header), not a
/// value recomputed from `--host`/`--port`, so this stays correct when bound to
/// `0.0.0.0` and reached via any of several interface addresses.
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

/// `security` spec, "WebSocket token handshake": the first message on a freshly
/// upgraded socket must be `{"type":"auth","token":"..."}` matching the
/// per-process token embedded in the served page (design.md D1). Anything else —
/// no message within [`AUTH_TIMEOUT`], a wrong token, a non-auth message first, or
/// the client closing — fails authentication; the caller closes the socket without
/// ever subscribing to stdin or reading the snapshot, so no data is sent either way.
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
                // Ignore ping/pong/binary frames while waiting for the auth text
                // frame — a control frame first is not a protocol violation, just
                // not the answer being waited for.
                Some(Ok(_)) => continue,
                Some(Err(_)) => return false,
            }
        }
    })
    .await;
    matches!(result, Ok(true))
}

/// `/ws` connection loop (ADR-0004, `stdin-stream` spec). Every new connection —
/// first connect, reload, or reconnect — must authenticate first (`security`
/// spec), then gets a snapshot of the current ring buffer before anything live,
/// then switches to streaming.
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

    let (Some(tx), Some(buffer)) = (state.stdin_tx.as_ref(), state.stdin_buffer.as_ref()) else {
        // File mode: nothing will ever be published on this endpoint. Keep the
        // socket open (closing immediately would be a confusing, undocumented
        // surprise) but there is nothing to loop on beyond client-initiated close.
        while socket.recv().await.transpose().ok().flatten().is_some() {}
        return;
    };

    // Subscribe *before* reading the snapshot. `stdin.rs`'s reader always pushes to
    // the buffer before broadcasting the same line, so any line broadcast after this
    // subscribe() is either (a) not yet in the snapshot we're about to read (we'll
    // see it live) or (b) already in it (we'll see it live too, but seq <= last_seq
    // below drops the duplicate). Either order is safe; only the *reverse* order
    // (reading the snapshot first) could lose a line that arrives in between.
    let mut rx = tx.subscribe();
    let (snapshot_lines, last_seq, already_ended) = stdin::read_snapshot(buffer);
    let snapshot_dtos: Vec<SnapshotLine> = snapshot_lines
        .into_iter()
        .map(|l| SnapshotLine {
            seq: l.seq,
            text: l.text,
        })
        .collect();
    if send_json(
        &mut socket,
        &ServerMessage::Snapshot {
            lines: &snapshot_dtos,
            last_seq,
        },
    )
    .await
    .is_err()
    {
        return;
    }

    // `Ended` is a one-shot broadcast (stdin.rs's module doc): a client connecting
    // after stdin already closed would otherwise wait forever for a message that
    // was already sent to nobody. `already_ended` (read under the same lock as the
    // snapshot, so it can't race a concurrent EOF landing mid-connect) lets this
    // connection get the same outcome a client that was present at EOF gets,
    // instead of a permanently "live" indicator over a stream that is actually over.
    if already_ended {
        let _ = send_json(&mut socket, &ServerMessage::Ended).await;
        let _ = socket
            .send(Message::Close(Some(CloseFrame {
                code: 1000,
                reason: "stdin stream ended".into(),
            })))
            .await;
        return;
    }

    loop {
        tokio::select! {
            event = rx.recv() => {
                match event {
                    Ok(StdinEvent::Line { seq, text }) => {
                        // Already delivered as part of the snapshot above — see the
                        // ordering comment on `rx.subscribe()`.
                        if seq <= last_seq {
                            continue;
                        }
                        if send_json(&mut socket, &ServerMessage::Line { seq, text: &text }).await.is_err() {
                            break;
                        }
                    }
                    Ok(StdinEvent::Ended) => {
                        let _ = send_json(&mut socket, &ServerMessage::Ended).await;
                        let _ = socket
                            .send(Message::Close(Some(CloseFrame {
                                code: 1000,
                                reason: "stdin stream ended".into(),
                            })))
                            .await;
                        break;
                    }
                    Err(broadcast::error::RecvError::Lagged(count)) => {
                        // The per-client broadcast buffer overflowed: exactly `count`
                        // messages were dropped before this receiver could read them
                        // (D2/D4 — never silent). The stdin reader was never blocked
                        // by this; only this client's own delivery fell behind.
                        if send_json(&mut socket, &ServerMessage::Gap { count }).await.is_err() {
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
            incoming = socket.recv() => {
                match incoming {
                    Some(Ok(_)) => continue, // this change does not read client messages
                    _ => break,              // client closed or the socket errored
                }
            }
        }
    }
}

/// Serialize `msg` and send it as a WebSocket text frame.
async fn send_json(socket: &mut WebSocket, msg: &ServerMessage<'_>) -> Result<(), axum::Error> {
    let json = serde_json::to_string(msg).expect("ServerMessage always serializes");
    socket.send(Message::Text(json.into())).await
}
