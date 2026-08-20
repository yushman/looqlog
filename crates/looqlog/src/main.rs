mod assets;
mod cli;
mod protocol;
mod server;
mod stdin;

use std::io::Write;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;

use clap::Parser;

use cli::{Cli, Mode};
use server::AppState;

const SSH_FORWARD_HINT: &str =
    "if this is a remote/headless machine, forward it with: ssh -L <port>:127.0.0.1:<port> <host>";

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("looqlog=info")),
        )
        .init();

    let args = Cli::parse();
    let mode = args.mode();

    // `cli` spec, resolved design.md open question: --max-lines is a no-op in file
    // mode, but silently ignoring an explicit value is exactly the "quiet surprise"
    // this project's testing rules forbid — say so.
    if args.max_lines.is_some() && mode == Mode::File {
        eprintln!("note: --max-lines has no effect in file mode (ignored)");
    }

    let host = match args.resolved_host() {
        Ok(host) => host,
        Err(err) => {
            eprintln!("error: {err}");
            std::process::exit(2);
        }
    };

    // ADR-0003: unconditional, unsuppressible warning whenever the bind address is
    // not the loopback address.
    if host != IpAddr::V4(Ipv4Addr::LOCALHOST) {
        println!(
            "WARNING: binding to {host} exposes the live stdin log stream on an \
             unauthenticated WebSocket to the network (ADR-0003). Anyone who can \
             reach this host and port can read stdin log lines, which may contain \
             credentials or PII."
        );
    }

    let addr = SocketAddr::new(host, args.port);
    let listener = match tokio::net::TcpListener::bind(addr).await {
        Ok(listener) => listener,
        Err(err) if err.kind() == std::io::ErrorKind::AddrInUse => {
            eprintln!(
                "error: port {} is already in use; try `--port 0` to pick a free port",
                args.port
            );
            std::process::exit(1);
        }
        Err(err) => {
            eprintln!("error: could not bind {addr}: {err}");
            std::process::exit(1);
        }
    };
    let actual_port = listener
        .local_addr()
        .expect("bound listener has a local address")
        .port();
    let url = format!("http://{host}:{actual_port}");

    // File mode only: a `stat`-level existence check purely for the startup/page
    // hint text. This is not a read of the file's contents — `local-server` forbids
    // opening or reading the log file, not verifying it exists (ADR-0002/ADR-0007).
    let mut path_hint: Option<String> = None;
    if mode == Mode::File {
        if let Some(path) = &args.path {
            let display = path.display().to_string();
            if std::fs::metadata(path).is_err() {
                eprintln!(
                    "warning: could not verify that {display} exists; the browser \
                     will still let you pick a file"
                );
            }
            path_hint = Some(display);
        }
    }

    let hint_html = match &path_hint {
        Some(path) => format!(
            "Open <code>{}</code> below — this is a hint from the command line; the \
             file is read by your browser, not by <code>looqlog</code> (see ADR-0007).",
            assets::html_escape(path)
        ),
        None => "Open a log file below.".to_string(),
    };

    // `security` spec, design.md D1: one random token per process, embedded in the
    // served page and required in the `/ws` handshake payload. Generated once here
    // (never per-request) so every page load for the life of this process — first
    // load, reload, a second tab — gets the same value and can all authenticate.
    let token = generate_token();

    let (stdin_tx, stdin_buffer, mode_json) = match mode {
        Mode::Stdin => {
            let max_lines = args.max_lines_or_default();
            let (tx, buffer) = stdin::spawn_stdin_reader(max_lines);
            (
                Some(tx),
                Some(buffer),
                format!(r#"{{"mode":"stdin","maxLines":{max_lines}}}"#),
            )
        }
        Mode::File => (None, None, r#"{"mode":"file"}"#.to_string()),
    };

    let state = Arc::new(AppState {
        hint_html,
        mode_json,
        token,
        stdin_tx,
        stdin_buffer,
    });
    let router = server::build_router(state);

    print_banner(
        &url,
        mode,
        path_hint.as_deref(),
        args.max_lines_or_default(),
    );

    if args.open && !args.no_browser {
        if let Err(err) = webbrowser::open(&url) {
            println!("could not open a browser automatically ({err})");
            println!("open this URL manually: {url}");
            println!("{SSH_FORWARD_HINT}");
        }
    }

    tokio::select! {
        result = axum::serve(listener, router) => {
            if let Err(err) = result {
                tracing::error!("server error: {err}");
                std::process::exit(1);
            }
        }
        _ = tokio::signal::ctrl_c() => {
            println!("\nshutting down");
            let _ = std::io::stdout().flush();
            // Immediate process exit: releases the port and closes every open
            // WebSocket connection at the OS level within this call, well under the
            // one-second budget (`local-server` spec, "Graceful shutdown"), with no
            // panic/task-abort output.
            std::process::exit(0);
        }
    }
}

/// 128 bits of OS-sourced randomness (`rand::thread_rng`, itself seeded from the
/// OS CSPRNG), hex-encoded. `security` spec, "WebSocket token handshake".
fn generate_token() -> String {
    use rand::RngCore;
    let mut bytes = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut bytes);
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn print_banner(url: &str, mode: Mode, path_hint: Option<&str>, max_lines: usize) {
    println!("looqlog v{}", env!("CARGO_PKG_VERSION"));
    println!("\u{2192} {url}");
    match (mode, path_hint) {
        (Mode::File, Some(path)) => println!("  open {path} in the browser to view it"),
        (Mode::File, None) => println!("  open a log file in the browser"),
        (Mode::Stdin, _) => println!(
            "  streaming stdin — connect a browser to view it live (max-lines: {max_lines})"
        ),
    }
    println!("Press Ctrl+C to quit");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_is_32_lowercase_hex_chars_and_varies_per_call() {
        let a = generate_token();
        let b = generate_token();
        assert_eq!(a.len(), 32);
        assert!(a
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
        assert_ne!(a, b, "two calls produced the same token");
    }
}
