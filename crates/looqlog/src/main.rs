mod assets;
mod cli;
mod protocol;
mod server;
mod source;
mod stdin;

use std::io::Write;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use cli::{Cli, FALLBACK_PORTS};
use server::AppState;
use source::Registry;

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

    let (args, port_explicit) = Cli::parse_with_port_source();
    let reads_stdin = args.reads_stdin();

    // `cli` spec: --max-lines only bounds the stdin source, but silently ignoring an
    // explicit value is exactly the "quiet surprise" this project's testing rules
    // forbid — say so.
    if args.max_lines.is_some() && !reads_stdin {
        eprintln!("note: --max-lines has no effect without stdin (ignored)");
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
            "WARNING: binding to {host} makes log contents — the opened file, uploaded \
             files and stdin — reachable from the network (ADR-0003). Anyone who can \
             reach this host and port and obtain the page token can read them, and they \
             may contain credentials or PII."
        );
    }

    let (listener, busy) = bind(host, args.port, port_explicit).await;
    let bound = listener
        .local_addr()
        .expect("bound listener has a local address");
    let url = format!("http://{host}:{}", bound.port());

    // `security` spec: one random token per process, embedded in the served page and
    // required by `/ws`, `/upload` and close. Generated once, so every page load for
    // the life of this process — first load, reload, a second tab — can authenticate.
    let token = generate_token();

    let registry = Arc::new(Mutex::new(Registry::default()));
    let mut lines = Vec::new();
    for path in &args.paths {
        // `file-ingest` spec: an unreadable path does not stop the server; its source
        // shows the same error and offers an upload instead.
        let (id, outcome) = source::open_argv(&registry, path);
        match outcome {
            Ok(None) => {}
            Ok(Some(warning)) => println!("warning: {}: {warning}", path.display()),
            Err(err) => eprintln!("error: {err}"),
        }
        lines.push((path.display().to_string(), id));
    }
    if reads_stdin {
        let id = stdin::spawn_stdin_reader(&registry, args.max_lines_or_default());
        lines.push((
            format!("stdin (max-lines: {})", args.max_lines_or_default()),
            id,
        ));
    }

    let sweeper = registry.clone();
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(source::SWEEP_INTERVAL);
        loop {
            tick.tick().await;
            let unloaded = sweeper
                .lock()
                .expect("registry lock poisoned")
                .sweep(Instant::now());
            if unloaded > 0 {
                tracing::info!("unloaded {unloaded} upload(s) with no tab open");
            }
        }
    });

    let state = Arc::new(AppState {
        token,
        registry,
        max_lines: args.max_lines_or_default(),
        bound,
    });
    let router = server::build_router(state);

    let source_urls: Vec<(String, String)> = lines
        .into_iter()
        .map(|(name, id)| (name, format!("{url}/l/{id}")))
        .collect();
    print_banner(&url, busy, &source_urls);

    if args.open && !args.no_browser {
        let targets: Vec<&str> = if source_urls.is_empty() {
            vec![url.as_str()]
        } else {
            source_urls.iter().map(|(_, u)| u.as_str()).collect()
        };
        for target in targets {
            if let Err(err) = webbrowser::open(target) {
                println!("could not open a browser automatically ({err})");
                println!("open the URLs above manually");
                println!("{SSH_FORWARD_HINT}");
                break;
            }
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

/// Binds `port`, or with no explicit `--port` the first free one of 7891–7900
/// (`local-server` spec). Returns the listener and the busy default port it skipped, if
/// any. Every failure exits with a message naming the port or range and `--port 0`.
async fn bind(host: IpAddr, port: u16, explicit: bool) -> (tokio::net::TcpListener, Option<u16>) {
    let ports = if explicit {
        port..=port
    } else {
        FALLBACK_PORTS
    };
    for candidate in ports.clone() {
        let addr = SocketAddr::new(host, candidate);
        match tokio::net::TcpListener::bind(addr).await {
            Ok(listener) => {
                let busy = (candidate != *ports.start()).then_some(*ports.start());
                return (listener, busy);
            }
            Err(err) if err.kind() == std::io::ErrorKind::AddrInUse => continue,
            Err(err) => {
                eprintln!("error: could not bind {addr}: {err}");
                std::process::exit(1);
            }
        }
    }
    if explicit {
        eprintln!("error: port {port} is already in use; try `--port 0` to pick a free port");
    } else {
        eprintln!(
            "error: ports {}–{} are all in use; try `--port 0` to pick a free port",
            ports.start(),
            ports.end()
        );
    }
    std::process::exit(1);
}

/// 128 bits of OS-sourced randomness (`rand::thread_rng`, itself seeded from the
/// OS CSPRNG), hex-encoded. `security` spec, "WebSocket token handshake".
fn generate_token() -> String {
    use rand::RngCore;
    let mut bytes = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut bytes);
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn print_banner(url: &str, busy: Option<u16>, sources: &[(String, String)]) {
    println!("looqlog v{}", env!("CARGO_PKG_VERSION"));
    if let Some(port) = busy {
        println!("port {port} is busy (another looqlog?), using the next free one");
    }
    println!("\u{2192} {url}  (all sources)");
    if sources.is_empty() {
        println!("  open a log file in the browser");
    }
    for (name, source_url) in sources {
        println!("  {name} \u{2192} {source_url}");
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
