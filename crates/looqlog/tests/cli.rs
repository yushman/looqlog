//! End-to-end tests against the compiled `looqlog` binary, covering the `cli`,
//! `local-server`, `stdin-stream`, `query-api`, `file-ingest` and `security` spec
//! scenarios that need a real process (argv parsing, actual TCP bind, signal handling,
//! `/ws` transport, `/upload`).
//!
//! Sandbox note: this environment has no real TTY attached to the test process
//! (`[ -t 0 ]` is false), so "file mode" scenarios use the Unix `script` utility to
//! allocate a pseudo-terminal for the child's stdin — the same trick a human running
//! `looqlog app.log` from an interactive shell gets for free. The `script`-based helper
//! is written for BSD/macOS `script` (this dev machine); Linux's `script` takes
//! `-qc "cmd args" /dev/null` instead, so `pty_file_mode` is `cfg(target_os =
//! "macos")`-gated and skipped elsewhere rather than silently asserting the wrong
//! thing.

use std::collections::HashSet;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::{Child, Command, Stdio};
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_looqlog")
}

/// Ports this test binary has already handed out via `free_port()`, kept for the
/// life of the process. Cargo runs these tests on multiple threads, so two threads
/// calling `free_port()` around the same time could otherwise get the identical
/// port: thread A binds `127.0.0.1:0`, reads the port, and drops the listener;
/// before thread A's child ever binds it, thread B's own `bind("127.0.0.1:0")` can
/// be reissued that exact same port by the OS. Entries here are never removed, so
/// once this process has handed a port to one test it will never hand that same
/// port to another — closing the intra-process half of the race regardless of how
/// the OS reissues freed ports.
static CLAIMED_PORTS: LazyLock<Mutex<HashSet<u16>>> = LazyLock::new(|| Mutex::new(HashSet::new()));

/// Ask the OS for a free port, immediately release it, and guarantee no other test
/// in this process has already been handed that same port (see `CLAIMED_PORTS`).
///
/// What this does *not* guarantee: that the port is still free by the time a child
/// process binds it. Anything outside this test binary — another process on the
/// machine grabbing it first, or the OS simply reissuing it to an unrelated caller
/// — can still take it in the gap between this call returning and the child's
/// `bind()`. That cross-process half of the race can't be closed by allocation
/// alone; `spawn_with_retry` below covers it with a bounded retry instead.
fn free_port() -> u16 {
    let mut claimed = CLAIMED_PORTS.lock().unwrap();
    loop {
        let port = TcpListener::bind("127.0.0.1:0")
            .expect("bind ephemeral port")
            .local_addr()
            .unwrap()
            .port();
        if claimed.insert(port) {
            return port;
        }
        // Already handed to another test in this process — the OS reissued a port
        // we're still relying on elsewhere. Try again rather than let two children
        // race for it.
    }
}

/// How many fresh ports `spawn_with_retry` will try before giving up. Bounds the
/// cross-process race described on `free_port()`: enough to ride out a transient
/// port grab by something else on the machine, small enough that a genuinely broken
/// binary fails fast instead of hanging the test suite.
const SPAWN_ATTEMPTS: u32 = 3;

/// Pick a port via `free_port()`, spawn the child with it via `spawn`, and wait up
/// to `timeout` for the server to come up. If it never does, kill the child, claim a
/// fresh port, and try again — the only lever left against the cross-process race,
/// since the chosen port may simply have been taken by something else between
/// allocation and the child's own `bind()`. Returns the child together with the
/// port it actually ended up serving on, which may differ from the port of a failed
/// earlier attempt.
fn spawn_with_retry(timeout: Duration, mut spawn: impl FnMut(u16) -> Child) -> (Child, u16) {
    let mut last_port = 0;
    for attempt in 1..=SPAWN_ATTEMPTS {
        let port = free_port();
        last_port = port;
        let mut child = spawn(port);
        if wait_until_serving(port, timeout) {
            return (child, port);
        }
        let _ = child.kill();
        let _ = child.wait();
        eprintln!(
            "spawn attempt {attempt}/{SPAWN_ATTEMPTS} on port {port} never came up \
             within {timeout:?}; retrying with a new port"
        );
    }
    panic!(
        "looqlog never started serving after {SPAWN_ATTEMPTS} attempts (last port \
         tried: {last_port}) — likely something outside this test binary is racing \
         for the same ephemeral ports"
    );
}

/// Minimal HTTP/1.1 GET, no external HTTP client dependency needed for a status-line
/// check.
fn http_get_status(port: u16, path: &str) -> Option<u16> {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).ok()?;
    stream.set_read_timeout(Some(Duration::from_secs(2))).ok()?;
    write!(
        stream,
        "GET {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n"
    )
    .ok()?;
    let mut reader = BufReader::new(stream);
    let mut status_line = String::new();
    reader.read_line(&mut status_line).ok()?;
    status_line
        .split_whitespace()
        .nth(1)
        .and_then(|code| code.parse().ok())
}

fn wait_until_serving(port: u16, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if http_get_status(port, "/") == Some(200) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(30));
    }
    false
}

/// Spawn `looqlog` in stdin mode (sidesteps the TTY-detection question entirely, since
/// `--stdin` forces the mode regardless of what stdin actually is).
fn spawn_stdin_mode(port: u16, extra_args: &[&str]) -> Child {
    Command::new(bin())
        .arg("--stdin")
        .arg("--port")
        .arg(port.to_string())
        .args(extra_args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn looqlog")
}

/// Spawn `looqlog` under a real pseudo-terminal via BSD `script`, so stdin actually
/// looks like a TTY and the CLI resolves to file mode without `--stdin`.
#[cfg(target_os = "macos")]
fn spawn_pty_file_mode(port: u16, extra_args: &[&str]) -> Child {
    Command::new("script")
        .arg("-q")
        .arg("/dev/null")
        .arg(bin())
        .arg("--port")
        .arg(port.to_string())
        .args(extra_args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn looqlog under script(1)")
}

fn read_all_nonblocking(child: &mut Child, timeout: Duration) -> (String, String) {
    // Give the process a moment to print its startup output, then kill it and
    // collect everything it wrote so far.
    std::thread::sleep(timeout);
    let _ = child.kill();
    let mut stdout = String::new();
    let mut stderr = String::new();
    if let Some(mut out) = child.stdout.take() {
        let _ = out.read_to_string(&mut stdout);
    }
    if let Some(mut err) = child.stderr.take() {
        let _ = err.read_to_string(&mut stderr);
    }
    let _ = child.wait();
    (stdout, stderr)
}

// ---- cli spec: argument surface -------------------------------------------------

#[test]
fn help_lists_every_documented_flag_with_defaults() {
    let output = Command::new(bin())
        .arg("--help")
        .output()
        .expect("run --help");
    assert!(output.status.success());
    let help = String::from_utf8_lossy(&output.stdout);
    for flag in [
        "--port",
        "--host",
        "--open",
        "--no-browser",
        "--stdin",
        "--max-lines",
        "--version",
        "--help",
    ] {
        assert!(help.contains(flag), "--help missing {flag}:\n{help}");
    }
    assert!(help.contains("7891"), "missing --port default:\n{help}");
    assert!(
        help.contains("127.0.0.1"),
        "missing --host default:\n{help}"
    );
}

#[test]
fn unknown_flag_is_rejected_loudly_and_starts_no_server() {
    let output = Command::new(bin())
        .arg("--colour")
        .output()
        .expect("run --colour");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("--colour"),
        "error should name the flag:\n{stderr}"
    );
}

// ---- local-server spec: bind, static assets -------------------------------------

#[test]
fn default_bind_serves_200() {
    let (mut child, port) = spawn_with_retry(Duration::from_secs(5), |p| spawn_stdin_mode(p, &[]));
    assert_eq!(http_get_status(port, "/"), Some(200));
    assert_eq!(http_get_status(port, "/assets/index.js"), Some(200));
    assert_eq!(http_get_status(port, "/assets/index.css"), Some(200));
    // `local-server` spec, "No WASM route": nothing serves a module or a worker.
    for gone in ["/assets/worker.js", "/wasm/core.js", "/wasm/core.wasm"] {
        assert_eq!(
            http_get_status(port, gone),
            Some(404),
            "{gone} is still served"
        );
    }
    let _ = child.kill();
    let _ = child.wait();
}

#[test]
fn port_zero_allocates_a_free_port_and_reports_it() {
    let mut child1 = spawn_stdin_mode(0, &[]);
    std::thread::sleep(Duration::from_millis(400));
    let (stdout1, _) = read_all_nonblocking(&mut child1, Duration::from_millis(200));
    let port1 = extract_port(&stdout1).expect("port in banner 1");
    assert_ne!(port1, 0);

    let mut child2 = spawn_stdin_mode(0, &[]);
    std::thread::sleep(Duration::from_millis(400));
    let (stdout2, _) = read_all_nonblocking(&mut child2, Duration::from_millis(200));
    let port2 = extract_port(&stdout2).expect("port in banner 2");
    assert_ne!(port2, 0);

    assert_ne!(port1, port2, "two --port 0 runs allocated the same port");
}

fn extract_port(banner: &str) -> Option<u16> {
    let idx = banner.find("http://")?;
    let rest = &banner[idx + "http://".len()..];
    let colon = rest.find(':')?;
    let after_colon = &rest[colon + 1..];
    let end = after_colon
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(after_colon.len());
    after_colon[..end].parse().ok()
}

#[test]
fn occupied_port_fails_cleanly_naming_the_port() {
    let occupying = TcpListener::bind("127.0.0.1:0").unwrap();
    let occupied_port = occupying.local_addr().unwrap().port();

    let output = Command::new(bin())
        .arg("--stdin")
        .arg("--port")
        .arg(occupied_port.to_string())
        .stdin(Stdio::null())
        .output()
        .expect("run looqlog against an occupied port");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains(&occupied_port.to_string()),
        "error should name the port {occupied_port}:\n{stderr}"
    );
    assert!(
        stderr.contains("--port 0"),
        "error should suggest --port 0:\n{stderr}"
    );
    assert!(
        !stderr.to_lowercase().contains("panic"),
        "should not panic:\n{stderr}"
    );
    drop(occupying);
}

/// `local-server`, "Second process falls back" and "Whole default range busy": with no
/// `--port`, 7891 busy means the next free port up to 7900; all of them busy fails.
/// One test, because both scenarios hold the same fixed ports.
#[test]
fn default_port_falls_back_within_the_range_and_fails_when_it_is_full() {
    let holders: Vec<Option<TcpListener>> = (7891..=7900)
        .map(|p| TcpListener::bind(("127.0.0.1", p)).ok())
        .collect();
    let output = Command::new(bin())
        .arg("--stdin")
        .stdin(Stdio::null())
        .output()
        .expect("run looqlog with the whole default range held");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("7891") && stderr.contains("7900") && stderr.contains("--port 0"),
        "error should name the range and suggest --port 0:\n{stderr}"
    );
    assert!(!stderr.to_lowercase().contains("panic"), "{stderr}");

    // Free everything but 7891 that this test held; whatever else is busy stays busy.
    let mut holders = holders.into_iter();
    let _held_7891 = holders.next().unwrap();
    drop(holders.collect::<Vec<_>>());
    let mut child = Command::new(bin())
        .arg("--stdin")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn looqlog without --port");
    let (stdout, stderr) = read_all_nonblocking(&mut child, Duration::from_millis(500));
    let port = extract_port(&stdout).unwrap_or_else(|| panic!("no port in:\n{stdout}{stderr}"));
    assert!((7892..=7900).contains(&port), "bound {port}:\n{stdout}");
    assert!(
        stdout.contains("port 7891 is busy"),
        "the banner should say 7891 was busy:\n{stdout}"
    );
}

/// `cli`, "Several paths" and "Banner lists every source".
#[test]
fn banner_lists_the_landing_and_every_source() {
    let a = temp_log("banner-a.log", "level=info msg=a\n");
    let b = temp_log("banner-b.log", "level=info msg=b\n");
    let (mut child, port) =
        spawn_with_retry(Duration::from_secs(5), |p| spawn_stdin_mode(p, &[&a, &b]));
    let (stdout, _) = read_all_nonblocking(&mut child, Duration::from_millis(100));
    assert!(
        stdout.contains(&format!("http://127.0.0.1:{port}  (all sources)")),
        "{stdout}"
    );
    let source_lines: Vec<&str> = stdout
        .lines()
        .filter(|l| l.contains(&format!("http://127.0.0.1:{port}/l/")))
        .collect();
    assert_eq!(source_lines.len(), 3, "{stdout}");
    assert!(source_lines[0].contains("banner-a.log"));
    assert!(source_lines[1].contains("banner-b.log"));
    assert!(source_lines[2].contains("stdin"));
    let ids: HashSet<&str> = source_lines
        .iter()
        .map(|l| l.rsplit("/l/").next().unwrap().trim())
        .collect();
    assert_eq!(ids.len(), 3);
    assert!(ids.iter().all(|id| id.len() == 8));
}

// ---- cli spec: exposure warning --------------------------------------------------

#[test]
fn binding_to_all_interfaces_warns_before_the_banner() {
    let (mut child, _port) = spawn_with_retry(Duration::from_secs(5), |p| {
        spawn_stdin_mode(p, &["--host", "0.0.0.0"])
    });
    let (stdout, _) = read_all_nonblocking(&mut child, Duration::from_millis(100));
    let warn_idx = stdout.find("WARNING").expect("exposure warning printed");
    let banner_idx = stdout.find("looqlog v").expect("banner printed");
    assert!(
        warn_idx < banner_idx,
        "warning should print before the banner:\n{stdout}"
    );
    assert!(stdout.contains("0.0.0.0"));
    assert!(
        stdout.contains("file") && stdout.contains("stdin"),
        "the warning should name both file and stdin exposure:\n{stdout}"
    );
}

#[test]
fn loopback_bind_stays_quiet() {
    let (mut child, _port) = spawn_with_retry(Duration::from_secs(5), |p| spawn_stdin_mode(p, &[]));
    let (stdout, _) = read_all_nonblocking(&mut child, Duration::from_millis(100));
    assert!(
        !stdout.contains("WARNING"),
        "default loopback bind should not warn:\n{stdout}"
    );
}

// ---- cli spec + file-ingest: file mode, ADR-0010 --------------------------------

#[cfg(target_os = "macos")]
#[test]
fn nonexistent_path_still_starts_and_names_the_error() {
    let (mut child, _port) = spawn_with_retry(Duration::from_secs(5), |p| {
        spawn_pty_file_mode(p, &["does-not-exist-12345.log"])
    });
    let _ = child.kill();
    // Under `script(1)`, the child's stdout and stderr are both written to the
    // pseudo-terminal and mirrored onto `script`'s own stdout, not kept as separate
    // streams — check the combined output.
    let (stdout, _) = read_all_nonblocking(&mut child, Duration::from_millis(100));
    assert!(
        stdout.contains("could not open") && stdout.contains("does-not-exist-12345.log"),
        "expected an error naming the path and the reason:\n{stdout}"
    );
}

#[cfg(target_os = "macos")]
#[test]
fn max_lines_note_printed_without_stdin_but_not_by_default() {
    let (mut child, _port) = spawn_with_retry(Duration::from_secs(5), |p| {
        spawn_pty_file_mode(p, &["--max-lines", "5"])
    });
    let (stdout, _) = read_all_nonblocking(&mut child, Duration::from_millis(100));
    assert!(
        stdout.contains("--max-lines has no effect without stdin"),
        "expected the no-op note:\n{stdout}"
    );

    let (mut child2, _port2) =
        spawn_with_retry(Duration::from_secs(5), |p| spawn_pty_file_mode(p, &[]));
    let (stdout2, _) = read_all_nonblocking(&mut child2, Duration::from_millis(100));
    assert!(
        !stdout2.contains("--max-lines"),
        "should not mention --max-lines when not passed:\n{stdout2}"
    );
}

// ---- stdin-stream / query-api specs ---------------------------------------------

fn messages(window: &serde_json::Value) -> Vec<String> {
    window["data"]["rows"]
        .as_array()
        .expect("rows array")
        .iter()
        .map(|r| r["message"].as_str().unwrap().to_string())
        .collect()
}

#[tokio::test]
async fn stdin_lines_become_queryable_by_every_client_in_order() {
    use ws_client::connect_ws;

    let (mut child, port) = spawn_with_retry(Duration::from_secs(5), |p| spawn_stdin_mode(p, &[]));
    let (mut a, mut b) = (connect_ws(port).await, connect_ws(port).await);

    let mut stdin = child.stdin.take().unwrap();
    writeln!(stdin, "hi").unwrap();
    writeln!(stdin, "second").unwrap();
    writeln!(stdin, "third").unwrap();
    stdin.flush().unwrap();

    for client in [&mut a, &mut b] {
        client.wait_for_total(3, Duration::from_secs(5)).await;
        let window = client
            .request(serde_json::json!({"type": "window", "offset": 0, "limit": 10}))
            .await;
        assert_eq!(messages(&window), vec!["hi", "second", "third"]);
        assert_eq!(window["data"]["rows"][0]["ordinal"], 1);
    }

    let _ = child.kill();
    let _ = child.wait();
}

#[tokio::test]
async fn eof_sends_ended_and_entries_stay_queryable() {
    use ws_client::connect_ws;

    let (mut child, port) = spawn_with_retry(Duration::from_secs(5), |p| spawn_stdin_mode(p, &[]));
    let mut client = connect_ws(port).await;
    let mut stdin = child.stdin.take().unwrap();
    writeln!(stdin, "a").unwrap();
    writeln!(stdin, "b").unwrap();
    drop(stdin);

    assert!(
        client
            .wait_for(Duration::from_secs(3), |m| m["type"] == "ended")
            .await
            .is_some(),
        "expected an ended notification after EOF"
    );
    assert!(wait_until_serving(port, Duration::from_secs(2)));
    let window = client
        .request(serde_json::json!({"type": "window", "offset": 0, "limit": 10}))
        .await;
    assert_eq!(messages(&window), vec!["a", "b"]);

    let _ = child.kill();
    let _ = child.wait();
}

#[tokio::test]
async fn connecting_after_stdin_already_closed_gets_ended_and_the_history() {
    use ws_client::connect_ws;

    let (mut child, port) = spawn_with_retry(Duration::from_secs(5), |p| spawn_stdin_mode(p, &[]));
    let mut stdin = child.stdin.take().unwrap();
    for i in 0..500 {
        writeln!(stdin, "early line {i}").unwrap();
    }
    drop(stdin);
    tokio::time::sleep(Duration::from_millis(500)).await;

    let mut client = connect_ws(port).await;
    let first = client
        .recv(Duration::from_secs(3))
        .await
        .expect("a first notification");
    assert_eq!(
        first["type"], "grown",
        "the first notification carries totals: {first}"
    );
    assert_eq!(first["total"], 500);
    assert!(client
        .wait_for(Duration::from_secs(3), |m| m["type"] == "ended")
        .await
        .is_some());
    let window = client
        .request(serde_json::json!({"type": "window", "offset": 0, "limit": 1000}))
        .await;
    assert_eq!(messages(&window).len(), 500);

    let _ = child.kill();
    let _ = child.wait();
}

#[tokio::test]
async fn eviction_is_announced_with_the_cumulative_count() {
    use ws_client::connect_ws;

    let (mut child, port) = spawn_with_retry(Duration::from_secs(5), |p| {
        spawn_stdin_mode(p, &["--max-lines", "100"])
    });
    let mut client = connect_ws(port).await;
    let mut stdin = child.stdin.take().unwrap();
    for i in 0..150 {
        writeln!(stdin, "line {i}").unwrap();
    }
    stdin.flush().unwrap();

    let evicted = client
        .wait_for(Duration::from_secs(5), |m| {
            m["type"] == "evicted" && m["count"] == 50
        })
        .await;
    assert!(evicted.is_some(), "expected evicted with count 50");
    let window = client
        .request(serde_json::json!({"type": "window", "offset": 0, "limit": 1}))
        .await;
    assert_eq!(window["data"]["rows"][0]["ordinal"], 51);
    assert_eq!(window["data"]["total"], 100);

    let _ = child.kill();
    let _ = child.wait();
}

/// `query-api`, "Fast producer, bounded messages": notifications are coalesced to one
/// per 100 ms, and the last one carries the exact totals.
#[tokio::test]
async fn fast_producer_gets_coalesced_cumulative_notifications() {
    use ws_client::connect_ws;

    let (mut child, port) = spawn_with_retry(Duration::from_secs(5), |p| spawn_stdin_mode(p, &[]));
    let mut client = connect_ws(port).await;
    let mut stdin = child.stdin.take().unwrap();
    let start = Instant::now();
    for i in 0..10_000 {
        writeln!(stdin, "flood line {i}").unwrap();
    }
    stdin.flush().unwrap();

    let mut grown = 0;
    let mut last_total = 0;
    while last_total < 10_000 {
        let msg = client
            .recv(Duration::from_secs(5))
            .await
            .expect("notifications until the totals are exact");
        if msg["type"] == "grown" {
            grown += 1;
            last_total = msg["total"].as_u64().unwrap();
        }
    }
    let elapsed = start.elapsed();
    let ticks = elapsed.as_millis() as usize / 100 + 2;
    eprintln!("[query-api] 10,000 lines → {grown} grown notifications in {elapsed:?}");
    assert!(
        grown <= ticks,
        "{grown} grown notifications in {elapsed:?} is more than one per tick"
    );

    let _ = child.kill();
    let _ = child.wait();
}

/// `stdin-stream`, "Producer is not blocked by a slow client": a connected client that
/// never reads does not stall the reader or the server.
#[tokio::test]
async fn a_client_that_never_reads_does_not_block_the_producer() {
    use ws_client::connect_ws;

    let (mut child, port) = spawn_with_retry(Duration::from_secs(5), |p| spawn_stdin_mode(p, &[]));
    let _silent = connect_ws(port).await;
    let mut stdin = child.stdin.take().unwrap();
    let start = Instant::now();
    for i in 0..20_000 {
        writeln!(stdin, "flood line {i}").unwrap();
    }
    stdin.flush().unwrap();
    assert!(
        start.elapsed() < Duration::from_secs(3),
        "writing 20,000 lines took {:?}",
        start.elapsed()
    );
    let mut other = connect_ws(port).await;
    other.wait_for_total(20_000, Duration::from_secs(5)).await;

    let _ = child.kill();
    let _ = child.wait();
}

/// The filter round trip on 10k entries (`filtering` spec, TDR §11 50 ms target): one
/// `query` request to its reply over a real socket. Printed for docs/devlog.md; the
/// bound asserted here is loose because this runs against a debug build.
#[tokio::test]
async fn filter_round_trip_on_ten_thousand_entries() {
    use ws_client::connect_ws;

    let (mut child, port) = spawn_with_retry(Duration::from_secs(5), |p| spawn_stdin_mode(p, &[]));
    let mut stdin = child.stdin.take().unwrap();
    let levels = ["INFO", "DEBUG", "WARN", "ERROR"];
    for i in 0..10_000 {
        writeln!(
            stdin,
            r#"{{"ts":"2026-08-09T12:00:00Z","level":"{}","service":"api","msg":"handled request {i}","status":200}}"#,
            levels[i % 4]
        )
        .unwrap();
    }
    drop(stdin);
    let mut client = connect_ws(port).await;
    client.wait_for_total(10_000, Duration::from_secs(10)).await;

    let mut samples = Vec::new();
    for filter in ["ERROR", "INFO", "ERROR", "WARN", "ERROR"] {
        let start = Instant::now();
        let reply = client
            .request(serde_json::json!({"type": "query", "filters": {"level": [filter]}}))
            .await;
        samples.push(start.elapsed());
        assert_eq!(reply["data"]["matched"], 2_500);
    }
    samples.sort();
    let median = samples[samples.len() / 2];
    eprintln!(
        "[filtering] level filter round trip on 10k entries: median {median:?}, all {samples:?}"
    );
    assert!(
        median < Duration::from_millis(500),
        "median round trip {median:?}"
    );

    let _ = child.kill();
    let _ = child.wait();
}

#[tokio::test]
async fn two_connections_keep_their_own_filters() {
    use ws_client::connect_ws;

    let (mut child, port) = spawn_with_retry(Duration::from_secs(5), |p| spawn_stdin_mode(p, &[]));
    let mut stdin = child.stdin.take().unwrap();
    for (i, level) in ["ERROR", "INFO", "ERROR"].iter().enumerate() {
        writeln!(stdin, r#"{{"level":"{level}","msg":"m{i}"}}"#).unwrap();
    }
    drop(stdin);
    let (mut a, mut b) = (connect_ws(port).await, connect_ws(port).await);
    a.wait_for_total(3, Duration::from_secs(5)).await;
    b.wait_for_total(3, Duration::from_secs(5)).await;

    a.request(serde_json::json!({"type": "query", "filters": {"level": ["ERROR"]}}))
        .await;
    b.request(serde_json::json!({"type": "query"})).await;
    let wa = a
        .request(serde_json::json!({"type": "window", "offset": 0, "limit": 10}))
        .await;
    let wb = b
        .request(serde_json::json!({"type": "window", "offset": 0, "limit": 10}))
        .await;
    assert_eq!(messages(&wa), vec!["m0", "m2"]);
    assert_eq!(messages(&wb).len(), 3);

    let _ = child.kill();
    let _ = child.wait();
}

/// `query-api`, "Unauthenticated query is refused".
#[tokio::test]
async fn a_query_before_the_token_closes_the_connection_without_data() {
    use futures_util::{SinkExt, StreamExt};
    use tokio_tungstenite::tungstenite::Message;

    let (mut child, port) = spawn_with_retry(Duration::from_secs(5), |p| spawn_stdin_mode(p, &[]));
    let mut stdin = child.stdin.take().unwrap();
    writeln!(stdin, "secret").unwrap();
    stdin.flush().unwrap();

    let (mut ws, _) = tokio_tungstenite::connect_async(format!("ws://127.0.0.1:{port}/ws"))
        .await
        .expect("connect to /ws");
    ws.send(Message::Text(
        r#"{"type":"window","id":1,"offset":0,"limit":10}"#.into(),
    ))
    .await
    .unwrap();
    let outcome = tokio::time::timeout(Duration::from_secs(8), async {
        loop {
            match ws.next().await {
                Some(Ok(Message::Close(_))) | None | Some(Err(_)) => return "closed",
                Some(Ok(Message::Text(_))) => return "got data",
                Some(Ok(_)) => continue,
            }
        }
    })
    .await;
    assert_eq!(outcome, Ok("closed"));

    let _ = child.kill();
    let _ = child.wait();
}

/// `security` spec, "Rebound hostname is rejected".
#[test]
fn websocket_upgrade_with_a_foreign_host_is_rejected() {
    let (mut child, port) = spawn_with_retry(Duration::from_secs(5), |p| spawn_stdin_mode(p, &[]));
    let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    write!(
        stream,
        "GET /ws HTTP/1.1\r\nHost: attacker.example:{port}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n\
         Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\n\
         Origin: http://attacker.example:{port}\r\n\r\n"
    )
    .unwrap();
    let mut status_line = String::new();
    BufReader::new(stream).read_line(&mut status_line).unwrap();
    assert!(
        status_line.contains("403"),
        "expected 403, got: {status_line}"
    );

    let _ = child.kill();
    let _ = child.wait();
}

// ---- file-ingest / security: /upload ----------------------------------------------

/// Raw `POST /upload`; returns the status code and the response body.
fn post_upload(port: u16, headers: &[(&str, String)], body: &[u8]) -> (u16, String) {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let mut request = format!(
        "POST /upload HTTP/1.1\r\nConnection: close\r\nContent-Type: application/octet-stream\r\nContent-Length: {}\r\n",
        body.len()
    );
    for (name, value) in headers {
        request.push_str(&format!("{name}: {value}\r\n"));
    }
    request.push_str("\r\n");
    stream.write_all(request.as_bytes()).unwrap();
    stream.write_all(body).unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).ok();
    let status = response
        .split_whitespace()
        .nth(1)
        .and_then(|c| c.parse().ok())
        .unwrap_or(0);
    let body = response.split("\r\n\r\n").nth(1).unwrap_or("").to_string();
    (status, body)
}

fn same_origin_headers(port: u16, token: &str) -> Vec<(&'static str, String)> {
    vec![
        ("Host", format!("127.0.0.1:{port}")),
        ("Origin", format!("http://127.0.0.1:{port}")),
        ("X-Looqlog-Token", token.to_string()),
        ("X-Looqlog-Filename", "second.jsonl".to_string()),
    ]
}

#[test]
fn upload_without_token_or_from_another_origin_is_forbidden() {
    let (mut child, port) = spawn_with_retry(Duration::from_secs(5), |p| spawn_stdin_mode(p, &[]));
    let token = ws_client::fetch_token(port);
    let body = b"{\"msg\":\"x\"}\n";

    let mut no_token = same_origin_headers(port, &token);
    no_token.retain(|(n, _)| *n != "X-Looqlog-Token");
    assert_eq!(post_upload(port, &no_token, body).0, 403);

    let mut foreign = same_origin_headers(port, &token);
    foreign[1] = ("Origin", "http://evil.example".to_string());
    assert_eq!(post_upload(port, &foreign, body).0, 403);

    let mut rebound = same_origin_headers(port, &token);
    rebound[0] = ("Host", format!("attacker.example:{port}"));
    rebound[1] = ("Origin", format!("http://attacker.example:{port}"));
    assert_eq!(post_upload(port, &rebound, body).0, 403);

    let _ = child.kill();
    let _ = child.wait();
}

/// `file-ingest`, "Upload next to stdin": an upload becomes its own source.
#[tokio::test]
async fn an_upload_next_to_stdin_becomes_its_own_source() {
    use ws_client::{connect_source, connect_ws, list_sources};

    let (mut child, port) = spawn_with_retry(Duration::from_secs(5), |p| spawn_stdin_mode(p, &[]));
    let mut stdin_tab = connect_ws(port).await;
    let mut stdin = child.stdin.take().unwrap();
    writeln!(stdin, "from stdin").unwrap();
    stdin.flush().unwrap();
    stdin_tab.wait_for_total(1, Duration::from_secs(5)).await;

    let token = ws_client::fetch_token(port);
    let body = b"{\"level\":\"WARN\",\"msg\":\"uploaded one\"}\n{\"level\":\"WARN\",\"msg\":\"uploaded two\"}\n";
    let (status, response) = post_upload(port, &same_origin_headers(port, &token), body);
    assert_eq!(status, 201, "{response}");
    let id = serde_json::from_str::<serde_json::Value>(&response).unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(id.len(), 8);

    let mut upload_tab = connect_source(port, &id).await;
    upload_tab.wait_for_total(2, Duration::from_secs(5)).await;
    let window = upload_tab
        .request(serde_json::json!({"type": "window", "offset": 0, "limit": 10}))
        .await;
    assert_eq!(messages(&window), vec!["uploaded one", "uploaded two"]);
    let stats = upload_tab
        .request(serde_json::json!({"type": "stats"}))
        .await;
    assert_eq!(stats["data"]["source"]["name"], "second.jsonl");
    assert_eq!(stats["data"]["source"]["kind"], "upload");

    let window = stdin_tab
        .request(serde_json::json!({"type": "window", "offset": 0, "limit": 10}))
        .await;
    assert_eq!(messages(&window), vec!["from stdin"]);
    let kinds: Vec<_> = list_sources(port)
        .await
        .iter()
        .map(|s| s["kind"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(kinds, ["stdin", "upload"]);

    let _ = child.kill();
    let _ = child.wait();
}

/// `file-ingest`, "Aborted upload leaves nothing behind".
#[tokio::test]
async fn an_aborted_upload_leaves_no_source() {
    let (mut child, port) = spawn_with_retry(Duration::from_secs(5), |p| spawn_stdin_mode(p, &[]));
    let token = ws_client::fetch_token(port);
    let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    let mut request = "POST /upload HTTP/1.1\r\nContent-Type: application/octet-stream\r\nContent-Length: 1000000\r\n".to_string();
    for (name, value) in same_origin_headers(port, &token) {
        request.push_str(&format!("{name}: {value}\r\n"));
    }
    request.push_str("\r\n");
    stream.write_all(request.as_bytes()).unwrap();
    for i in 0..200 {
        writeln!(stream, "{{\"msg\":\"partial {i}\"}}").unwrap();
    }
    stream.flush().unwrap();
    tokio::time::sleep(Duration::from_millis(300)).await;
    let during = ws_client::list_sources(port).await;
    assert_eq!(
        during.len(),
        2,
        "the upload is listed while it arrives: {during:?}"
    );
    drop(stream);

    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let sources = ws_client::list_sources(port).await;
        if sources.len() == 1 {
            assert_eq!(sources[0]["kind"], "stdin");
            break;
        }
        assert!(
            Instant::now() < deadline,
            "partial upload still listed: {sources:?}"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    let _ = child.kill();
    let _ = child.wait();
}

/// Writes `contents` to a file unique to this test process and returns its path.
fn temp_log(name: &str, contents: &str) -> String {
    let path = std::env::temp_dir().join(format!("looqlog-test-{}-{name}", std::process::id()));
    std::fs::write(&path, contents).unwrap();
    path.display().to_string()
}

/// Source ids by name suffix, from the landing list.
async fn source_id(port: u16, name_suffix: &str) -> String {
    ws_client::list_sources(port)
        .await
        .iter()
        .find(|s| s["name"].as_str().unwrap().ends_with(name_suffix))
        .unwrap_or_else(|| panic!("no source named *{name_suffix}"))["id"]
        .as_str()
        .unwrap()
        .to_string()
}

/// `query-api`, "Two tabs, two sources" and "Override does not leak to another source".
#[tokio::test]
async fn two_sources_keep_their_own_entries_and_format() {
    use ws_client::connect_source;

    let a = temp_log("two-a.log", "level=info msg=alpha\nlevel=warn msg=beta\n");
    let b = temp_log("two-b.log", "plain gamma line\n");
    let (mut child, port) =
        spawn_with_retry(Duration::from_secs(5), |p| spawn_stdin_mode(p, &[&a, &b]));
    drop(child.stdin.take());
    let (id_a, id_b) = (
        source_id(port, "two-a.log").await,
        source_id(port, "two-b.log").await,
    );
    assert_ne!(id_a, id_b);
    let mut tab_a = connect_source(port, &id_a).await;
    let mut tab_b = connect_source(port, &id_b).await;
    tab_a.wait_for_total(2, Duration::from_secs(5)).await;
    tab_b.wait_for_total(1, Duration::from_secs(5)).await;
    let wa = tab_a
        .request(serde_json::json!({"type": "window", "offset": 0, "limit": 10}))
        .await;
    let wb = tab_b
        .request(serde_json::json!({"type": "window", "offset": 0, "limit": 10}))
        .await;
    assert_eq!(messages(&wa), vec!["alpha", "beta"]);
    assert_eq!(messages(&wb), vec!["plain gamma line"]);

    let mut tab_a2 = connect_source(port, &id_a).await;
    tab_a2.wait_for_total(2, Duration::from_secs(5)).await;
    tab_a
        .request(serde_json::json!({"type": "set_format", "format": "plain"}))
        .await;
    assert!(tab_a2
        .wait_for(Duration::from_secs(3), |m| m["type"] == "reset")
        .await
        .is_some());
    assert!(
        tab_b
            .wait_for(Duration::from_millis(600), |m| m["type"] == "reset")
            .await
            .is_none(),
        "the other source's tab was reset"
    );
    let stats = tab_b.request(serde_json::json!({"type": "stats"})).await;
    assert_eq!(stats["data"]["stats"]["format"], "plain");
    let stats = tab_a2.request(serde_json::json!({"type": "stats"})).await;
    assert_eq!(stats["data"]["stats"]["format"], "plain");
    assert!(
        stats["data"]["stats"]["detection"].is_null()
            || stats["data"]["stats"]["detection"]["format"] != "logfmt"
    );

    let _ = child.kill();
    let _ = child.wait();
}

/// `set_format` with `tzName`: offset-less timestamps are read in that zone, per-line DST.
#[tokio::test]
async fn set_format_with_tz_name_parses_offsetless_timestamps_in_that_zone() {
    use ws_client::connect_source;

    let path = temp_log(
        "tzname.log",
        "{\"ts\":\"2026-07-01 12:00:00\",\"msg\":\"summer\"}\n{\"ts\":\"2026-01-01 12:00:00\",\"msg\":\"winter\"}\n",
    );
    let (mut child, port) =
        spawn_with_retry(Duration::from_secs(5), |p| spawn_stdin_mode(p, &[&path]));
    drop(child.stdin.take());
    let id = source_id(port, "tzname.log").await;
    let mut tab = connect_source(port, &id).await;
    tab.wait_for_total(2, Duration::from_secs(5)).await;
    tab.request(serde_json::json!({"type": "set_format", "tzName": "Europe/Berlin"}))
        .await;
    let w = tab
        .request(serde_json::json!({"type": "window", "offset": 0, "limit": 10}))
        .await;
    let stamps: Vec<&str> = w["data"]["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["timestamp"].as_str().unwrap())
        .collect();
    assert_eq!(
        stamps,
        vec!["2026-07-01T10:00:00+00:00", "2026-01-01T11:00:00+00:00"]
    );

    let bad = tab
        .request_raw(serde_json::json!({"type": "set_format", "tzName": "Europe/Moskow"}))
        .await;
    assert_eq!(bad["type"], "error", "{bad}");
    assert_eq!(bad["message"], "unknown time zone: Europe/Moskow");

    let _ = child.kill();
    let _ = child.wait();
}

/// `tzOffsetMinutes` and `tzName` together are refused and change nothing.
#[tokio::test]
async fn set_format_rejects_tz_offset_together_with_tz_name() {
    use ws_client::connect_source;

    let path = temp_log(
        "tzboth.log",
        "{\"ts\":\"2026-07-01 12:00:00\",\"msg\":\"summer\"}\n",
    );
    let (mut child, port) =
        spawn_with_retry(Duration::from_secs(5), |p| spawn_stdin_mode(p, &[&path]));
    drop(child.stdin.take());
    let id = source_id(port, "tzboth.log").await;
    let mut tab = connect_source(port, &id).await;
    let mut other = connect_source(port, &id).await;
    tab.wait_for_total(1, Duration::from_secs(5)).await;
    other.wait_for_total(1, Duration::from_secs(5)).await;
    let bad = tab
        .request_raw(serde_json::json!({
            "type": "set_format", "tzOffsetMinutes": 60, "tzName": "Europe/Berlin"
        }))
        .await;
    assert_eq!(bad["type"], "error", "{bad}");
    assert_eq!(
        bad["message"],
        "tzOffsetMinutes and tzName are mutually exclusive"
    );
    assert!(
        other
            .wait_for(Duration::from_millis(600), |m| m["type"] == "reset")
            .await
            .is_none(),
        "a rejected set_format reset the source"
    );
    let w = tab
        .request(serde_json::json!({"type": "window", "offset": 0, "limit": 10}))
        .await;
    assert_eq!(
        w["data"]["rows"][0]["timestamp"],
        "2026-07-01T12:00:00+00:00"
    );

    let _ = child.kill();
    let _ = child.wait();
}

/// `query-api`, "Stale link after restart" and `local-server`, "Unknown id still loads
/// the page".
#[tokio::test]
async fn an_unknown_id_gets_the_page_and_closed_unknown() {
    let (mut child, port) = spawn_with_retry(Duration::from_secs(5), |p| spawn_stdin_mode(p, &[]));
    assert_eq!(http_get_status(port, "/l/zzzzzzzz"), Some(200));
    let mut tab = ws_client::connect_source(port, "zzzzzzzz").await;
    let first = tab
        .recv(Duration::from_secs(3))
        .await
        .expect("a first message");
    assert_eq!(
        first,
        serde_json::json!({"type": "closed", "reason": "unknown"})
    );
    let reply = tab
        .request_raw(serde_json::json!({"type": "window", "offset": 0, "limit": 10}))
        .await;
    assert_eq!(reply["type"], "error", "{reply}");

    let _ = child.kill();
    let _ = child.wait();
}

/// `source-registry`, "Closing from the landing", and stdin cannot be closed.
#[tokio::test]
async fn closing_from_the_landing_tells_the_tabs() {
    use ws_client::{connect_landing, connect_source};

    let a = temp_log("close-a.log", "level=info msg=alpha\n");
    let (mut child, port) =
        spawn_with_retry(Duration::from_secs(5), |p| spawn_stdin_mode(p, &[&a]));
    let id = source_id(port, "close-a.log").await;
    let stdin_id = source_id(port, "stdin").await;
    let mut tab = connect_source(port, &id).await;
    tab.wait_for_total(1, Duration::from_secs(5)).await;

    let mut landing = connect_landing(port).await;
    let listed = landing
        .wait_for(Duration::from_secs(3), |m| {
            m["type"] == "sources"
                && m["sources"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|s| s["id"] == id.as_str() && s["tabs"] == 1)
        })
        .await;
    assert!(listed.is_some(), "the landing shows the tab count");
    let refused = landing
        .request_raw(serde_json::json!({"type": "close", "source": stdin_id}))
        .await;
    assert_eq!(refused["type"], "error");
    assert!(refused["message"].as_str().unwrap().contains("stdin"));
    landing
        .request(serde_json::json!({"type": "close", "source": id}))
        .await;

    let closed = tab
        .wait_for(Duration::from_secs(3), |m| m["type"] == "closed")
        .await;
    assert_eq!(
        closed,
        Some(serde_json::json!({"type": "closed", "reason": "closed"}))
    );
    let gone = landing
        .wait_for(Duration::from_secs(3), |m| {
            m["type"] == "sources" && m["sources"].as_array().unwrap().len() == 1
        })
        .await;
    assert!(gone.is_some(), "the closed source left the list");
    let mut reloaded = connect_source(port, &id).await;
    assert_eq!(
        reloaded.recv(Duration::from_secs(3)).await,
        Some(serde_json::json!({"type": "closed", "reason": "closed"}))
    );

    let _ = child.kill();
    let _ = child.wait();
}

fn post_close(port: u16, id: &str, headers: &[(&str, String)]) -> u16 {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let mut request =
        format!("POST /sources/{id}/close HTTP/1.1\r\nConnection: close\r\nContent-Length: 0\r\n");
    for (name, value) in headers {
        request.push_str(&format!("{name}: {value}\r\n"));
    }
    request.push_str("\r\n");
    stream.write_all(request.as_bytes()).unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).ok();
    response
        .split_whitespace()
        .nth(1)
        .and_then(|c| c.parse().ok())
        .unwrap_or(0)
}

/// `security`, "Cross-site close"; stdin close refused with 409.
#[tokio::test]
async fn close_over_http_is_checked_and_refuses_stdin() {
    let a = temp_log("http-close.log", "level=info msg=alpha\n");
    let (mut child, port) =
        spawn_with_retry(Duration::from_secs(5), |p| spawn_stdin_mode(p, &[&a]));
    let token = ws_client::fetch_token(port);
    let id = source_id(port, "http-close.log").await;
    let stdin_id = source_id(port, "stdin").await;

    let mut foreign = same_origin_headers(port, &token);
    foreign[1] = ("Origin", "http://evil.example".to_string());
    assert_eq!(post_close(port, &id, &foreign), 403);
    let mut no_token = same_origin_headers(port, &token);
    no_token.retain(|(n, _)| *n != "X-Looqlog-Token");
    assert_eq!(post_close(port, &id, &no_token), 403);
    assert_eq!(ws_client::list_sources(port).await.len(), 2, "still open");

    assert_eq!(
        post_close(port, &stdin_id, &same_origin_headers(port, &token)),
        409
    );
    assert_eq!(
        post_close(port, &id, &same_origin_headers(port, &token)),
        204
    );
    assert_eq!(
        post_close(port, &id, &same_origin_headers(port, &token)),
        404
    );

    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(target_os = "macos")]
fn fixture_path() -> String {
    format!(
        "{}/../../tests/fixtures/sample.jsonl",
        env!("CARGO_MANIFEST_DIR")
    )
}

/// `file-ingest`, "Zero-click open": the argv file is read by the backend.
#[cfg(target_os = "macos")]
#[tokio::test]
async fn the_argv_file_is_queryable_without_a_picker() {
    use ws_client::connect_ws;

    let path = fixture_path();
    let (mut child, port) =
        spawn_with_retry(Duration::from_secs(5), |p| spawn_pty_file_mode(p, &[&path]));
    let mut client = connect_ws(port).await;
    client.wait_for_total(20, Duration::from_secs(5)).await;
    let reply = client
        .request(serde_json::json!({"type": "query", "filters": {"level": ["ERROR"]}}))
        .await;
    assert_eq!(reply["data"]["matched"], 2);
    let stats = client.request(serde_json::json!({"type": "stats"})).await;
    assert!(stats["data"]["source"]["name"]
        .as_str()
        .unwrap()
        .ends_with("sample.jsonl"));
    assert_eq!(stats["data"]["stats"]["format"], "json");

    let _ = child.kill();
    let _ = child.wait();
}

/// task 2.7 (memory): ten times `--max-lines` written, buffer never exceeds
/// `--max-lines` and process RSS does not grow proportionally to total lines
/// written. The `Session` unit tests in `looqlog-core` prove the structural bound;
/// this is the process-level companion check.
#[cfg(target_os = "macos")]
#[test]
fn process_memory_stays_bounded_over_ten_times_max_lines() {
    let max_lines = 2_000;
    let (mut child, _port) = spawn_with_retry(Duration::from_secs(5), |p| {
        spawn_stdin_mode(p, &["--max-lines", &max_lines.to_string()])
    });

    let pid = child.id();
    let rss_kb = |pid: u32| -> Option<u64> {
        let output = Command::new("ps")
            .args(["-o", "rss=", "-p", &pid.to_string()])
            .output()
            .ok()?;
        String::from_utf8_lossy(&output.stdout).trim().parse().ok()
    };

    let mut stdin = child.stdin.take().unwrap();
    let write_n = |stdin: &mut std::process::ChildStdin, n: usize| {
        for i in 0..n {
            writeln!(stdin, "line {i} of a moderately long representative log message with a few fields level=INFO service=api").unwrap();
        }
        stdin.flush().unwrap();
    };

    write_n(&mut stdin, max_lines); // fills the buffer once
    std::thread::sleep(Duration::from_millis(300));
    let rss_after_1x = rss_kb(pid).expect("read RSS after 1x max-lines");

    write_n(&mut stdin, max_lines * 9); // writes 9 more buffer-fulls (10x total)
    std::thread::sleep(Duration::from_millis(300));
    let rss_after_10x = rss_kb(pid).expect("read RSS after 10x max-lines");

    let growth_kb = rss_after_10x.saturating_sub(rss_after_1x);
    eprintln!(
        "[task 2.7] RSS after 1x max-lines ({max_lines}): {rss_after_1x}KB; after 10x: \
         {rss_after_10x}KB; growth: {growth_kb}KB"
    );
    assert!(
        growth_kb < 20_000,
        "RSS grew {growth_kb}KB after writing 9x more max-lines with a bounded \
         buffer (before: {rss_after_1x}KB, after: {rss_after_10x}KB) — buffer may \
         not be bounded in the running process"
    );

    let _ = child.kill();
    let _ = child.wait();
}

#[test]
fn producer_is_not_blocked_by_a_missing_client() {
    let (mut child, port) = spawn_with_retry(Duration::from_secs(5), |p| spawn_stdin_mode(p, &[]));

    let mut stdin = child.stdin.take().unwrap();
    let start = Instant::now();
    for i in 0..5000 {
        writeln!(stdin, "line {i}").unwrap();
    }
    stdin.flush().unwrap();
    drop(stdin);
    assert!(
        start.elapsed() < Duration::from_secs(3),
        "writing to stdin with no client connected took too long: {:?}",
        start.elapsed()
    );

    // Process should still be responsive afterward.
    assert!(wait_until_serving(port, Duration::from_secs(2)));
    let _ = child.kill();
    let _ = child.wait();
}

// ---- local-server spec: graceful shutdown ----------------------------------------

#[cfg(unix)]
#[test]
fn ctrl_c_exits_zero_within_one_second_and_releases_the_port() {
    // `spawn_with_retry` only retries the "did the server come up" step; by the
    // time it returns, `port` is confirmed serving. The assertion this test exists
    // for — that the port is free again after SIGINT — runs afterward as its own,
    // separate check below, so a retry here cannot mask a genuine port-release
    // regression.
    let (mut child, port) = spawn_with_retry(Duration::from_secs(5), |p| spawn_stdin_mode(p, &[]));

    let pid = child.id();
    Command::new("kill")
        .args(["-INT", &pid.to_string()])
        .status()
        .expect("send SIGINT");

    let start = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().expect("try_wait") {
            break status;
        }
        assert!(
            start.elapsed() < Duration::from_secs(1),
            "process did not exit within one second of SIGINT"
        );
        std::thread::sleep(Duration::from_millis(10));
    };
    assert!(status.success(), "expected exit 0, got {status:?}");

    // Port must be immediately reusable.
    assert!(
        TcpListener::bind(("127.0.0.1", port)).is_ok(),
        "port {port} was not released promptly after shutdown"
    );
}

// ---- security spec: CSP, origin check, token handshake ---------------------------

#[test]
fn every_response_carries_the_csp_header() {
    let (mut child, port) = spawn_with_retry(Duration::from_secs(5), |p| spawn_stdin_mode(p, &[]));

    let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    write!(
        stream,
        "GET / HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n"
    )
    .unwrap();
    let mut reader = BufReader::new(stream);
    let mut headers = String::new();
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" || line == "\n" {
            break;
        }
        headers.push_str(&line);
    }
    let lower = headers.to_lowercase();
    assert!(
        lower.contains("content-security-policy"),
        "expected a CSP header:\n{headers}"
    );
    assert!(
        lower.contains("default-src 'self'"),
        "expected default-src 'self':\n{headers}"
    );
    // `security` spec, "Inline styles are not permitted": the virtual scroller
    // positions rows through CSSOM rules in the linked stylesheet, so `style-src`
    // is as strict as `default-src`. A reappearing `'unsafe-inline'` would be a
    // silent widening of the policy.
    assert!(
        lower.contains("style-src 'self'"),
        "expected style-src 'self':\n{headers}"
    );
    assert!(
        !lower.contains("'unsafe-inline'"),
        "expected no 'unsafe-inline' anywhere in the policy:\n{headers}"
    );
    assert!(
        !lower.contains("wasm-unsafe-eval"),
        "expected no WASM allowance in the policy:\n{headers}"
    );

    let _ = child.kill();
    let _ = child.wait();
}

/// The served page must embed a nonempty token distinct across restarts — the raw
/// material `web/src/token.ts` reads and `/ws`'s auth handshake checks against.
#[test]
fn served_page_embeds_a_nonempty_per_process_token() {
    let (mut child, port) = spawn_with_retry(Duration::from_secs(5), |p| spawn_stdin_mode(p, &[]));

    let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    write!(
        stream,
        "GET / HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n"
    )
    .unwrap();
    let mut body = String::new();
    stream.read_to_string(&mut body).ok();
    let start_tag = "<template id=\"token-template\">";
    let start = body.find(start_tag).expect("token-template present") + start_tag.len();
    let end = body[start..]
        .find("</template>")
        .expect("token-template closed")
        + start;
    let token = &body[start..end];
    assert!(!token.is_empty(), "token must not be empty");
    assert_ne!(
        token, "__LOOQLOG_TOKEN__",
        "placeholder was never substituted"
    );
    assert!(
        token.chars().all(|c| c.is_ascii_hexdigit()),
        "expected a hex token, got: {token}"
    );

    let _ = child.kill();
    let _ = child.wait();
}

/// `security` spec, "Foreign origin is rejected": a WebSocket upgrade request
/// carrying an `Origin` that does not match the request's own `Host` must be
/// refused before the handshake completes (no `101 Switching Protocols`), and
/// therefore before any stdin data could possibly be sent.
#[tokio::test]
async fn cross_origin_websocket_upgrade_is_rejected() {
    let (mut child, port) = spawn_with_retry(Duration::from_secs(5), |p| spawn_stdin_mode(p, &[]));

    let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    write!(
        stream,
        "GET /ws HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n\
         Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\n\
         Origin: http://evil.example\r\n\r\n"
    )
    .unwrap();
    let mut reader = BufReader::new(stream);
    let mut status_line = String::new();
    reader.read_line(&mut status_line).unwrap();
    assert!(
        !status_line.contains("101"),
        "cross-origin upgrade should be rejected, got: {status_line}"
    );
    assert!(
        status_line.contains("403"),
        "expected 403 for a mismatched Origin, got: {status_line}"
    );

    let _ = child.kill();
    let _ = child.wait();
}

/// `security` spec, "The application's own page connects": the same-origin case
/// (`Origin` matching `Host`) must still succeed — verifies the check has a live
/// negative case, not just an unconditional refusal.
#[tokio::test]
async fn same_origin_websocket_upgrade_succeeds() {
    let (mut child, port) = spawn_with_retry(Duration::from_secs(5), |p| spawn_stdin_mode(p, &[]));

    let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    write!(
        stream,
        "GET /ws HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n\
         Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\n\
         Origin: http://127.0.0.1:{port}\r\n\r\n"
    )
    .unwrap();
    let mut reader = BufReader::new(stream);
    let mut status_line = String::new();
    reader.read_line(&mut status_line).unwrap();
    assert!(
        status_line.contains("101"),
        "same-origin upgrade should succeed, got: {status_line}"
    );

    let _ = child.kill();
    let _ = child.wait();
}

/// `security` spec, "Connection without a token": a client that connects and sends
/// nothing must be closed within the server's handshake timeout, without ever
/// receiving the snapshot or any line.
#[tokio::test]
async fn websocket_connection_without_a_token_is_closed_without_data() {
    use futures_util::StreamExt;
    use tokio_tungstenite::tungstenite::Message;

    let (mut child, port) = spawn_with_retry(Duration::from_secs(5), |p| spawn_stdin_mode(p, &[]));

    let url = format!("ws://127.0.0.1:{port}/ws");
    let (mut ws, _) = tokio_tungstenite::connect_async(url)
        .await
        .expect("connect to /ws");

    let outcome = tokio::time::timeout(Duration::from_secs(8), async {
        loop {
            match ws.next().await {
                Some(Ok(Message::Close(_))) | None => return "closed",
                Some(Ok(Message::Text(_))) => return "got data",
                Some(Ok(_)) => continue,
                Some(Err(_)) => return "closed",
            }
        }
    })
    .await;
    assert_eq!(
        outcome,
        Ok("closed"),
        "a connection that never authenticates should be closed, never sent data"
    );

    let _ = child.kill();
    let _ = child.wait();
}

/// Same as above, but with an explicit wrong token rather than silence.
#[tokio::test]
async fn websocket_connection_with_wrong_token_is_closed_without_data() {
    use futures_util::{SinkExt, StreamExt};
    use tokio_tungstenite::tungstenite::Message;

    let (mut child, port) = spawn_with_retry(Duration::from_secs(5), |p| spawn_stdin_mode(p, &[]));

    let url = format!("ws://127.0.0.1:{port}/ws");
    let (mut ws, _) = tokio_tungstenite::connect_async(url)
        .await
        .expect("connect to /ws");
    ws.send(Message::Text(
        r#"{"type":"auth","token":"not-the-real-token"}"#.into(),
    ))
    .await
    .expect("send bogus auth");

    let outcome = tokio::time::timeout(Duration::from_secs(8), async {
        loop {
            match ws.next().await {
                Some(Ok(Message::Close(_))) | None => return "closed",
                Some(Ok(Message::Text(_))) => return "got data",
                Some(Ok(_)) => continue,
                Some(Err(_)) => return "closed",
            }
        }
    })
    .await;
    assert_eq!(
        outcome,
        Ok("closed"),
        "a wrong token should be closed, never sent data"
    );

    let _ = child.kill();
    let _ = child.wait();
}

/// Tiny `/ws` client used only by these tests: authenticates with the token from the
/// served page, then sends `query-api` requests and collects notifications as JSON.
mod ws_client {
    use std::collections::VecDeque;
    use std::io::{Read, Write};
    use std::time::Duration;

    use futures_util::{SinkExt, StreamExt};
    use serde_json::Value;
    use tokio_tungstenite::tungstenite::Message;

    pub struct WsClient {
        inner: tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
        next_id: u64,
        /// Notifications received while waiting for a reply.
        pending: VecDeque<Value>,
    }

    /// Pulls the per-process token out of `#token-template`, the element the real
    /// page reads (`web/src/token.ts`).
    pub fn fetch_token(port: u16) -> String {
        let mut stream = std::net::TcpStream::connect(("127.0.0.1", port))
            .expect("connect to fetch the page for its token");
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        write!(
            stream,
            "GET / HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n"
        )
        .unwrap();
        let mut body = String::new();
        stream.read_to_string(&mut body).ok();
        let start_tag = "<template id=\"token-template\">";
        let start = body
            .find(start_tag)
            .map(|i| i + start_tag.len())
            .expect("token-template present in served page");
        let end = body[start..]
            .find("</template>")
            .map(|i| start + i)
            .expect("token-template closed");
        body[start..end].to_string()
    }

    /// A tab on the process's only source.
    pub async fn connect_ws(port: u16) -> WsClient {
        let sources = list_sources(port).await;
        assert_eq!(sources.len(), 1, "expected exactly one source: {sources:?}");
        connect_source(port, sources[0]["id"].as_str().unwrap()).await
    }

    /// The landing's source list, as its first `sources` message carries it.
    pub async fn list_sources(port: u16) -> Vec<Value> {
        let mut landing = connect_landing(port).await;
        let msg = landing
            .wait_for(Duration::from_secs(5), |m| m["type"] == "sources")
            .await
            .expect("a sources message on the landing connection");
        msg["sources"].as_array().unwrap().clone()
    }

    pub async fn connect_landing(port: u16) -> WsClient {
        connect_as(port, Value::Null).await
    }

    pub async fn connect_source(port: u16, id: &str) -> WsClient {
        connect_as(port, id.into()).await
    }

    async fn connect_as(port: u16, source: Value) -> WsClient {
        let token = fetch_token(port);
        let (mut inner, _) = tokio_tungstenite::connect_async(format!("ws://127.0.0.1:{port}/ws"))
            .await
            .expect("connect to /ws");
        let auth =
            serde_json::json!({"type": "auth", "token": token, "source": source}).to_string();
        inner
            .send(Message::Text(auth.into()))
            .await
            .expect("send auth handshake");
        WsClient {
            inner,
            next_id: 1,
            pending: VecDeque::new(),
        }
    }

    impl WsClient {
        async fn read(&mut self, timeout: Duration) -> Option<Value> {
            loop {
                match tokio::time::timeout(timeout, self.inner.next()).await {
                    Ok(Some(Ok(Message::Text(text)))) => {
                        return Some(
                            serde_json::from_str(&text).unwrap_or_else(|err| {
                                panic!("invalid envelope JSON: {err}: {text}")
                            }),
                        )
                    }
                    Ok(Some(Ok(Message::Close(_)))) => return None,
                    Ok(Some(Ok(_))) => continue,
                    Ok(Some(Err(_))) | Ok(None) | Err(_) => return None,
                }
            }
        }

        /// The next notification, in arrival order.
        pub async fn recv(&mut self, timeout: Duration) -> Option<Value> {
            match self.pending.pop_front() {
                Some(msg) => Some(msg),
                None => self.read(timeout).await,
            }
        }

        /// The first notification satisfying `pred` within `timeout`; earlier ones are
        /// dropped.
        pub async fn wait_for(
            &mut self,
            timeout: Duration,
            pred: impl Fn(&Value) -> bool,
        ) -> Option<Value> {
            let deadline = tokio::time::Instant::now() + timeout;
            loop {
                let left = deadline.saturating_duration_since(tokio::time::Instant::now());
                let msg = self.recv(left).await?;
                if pred(&msg) {
                    return Some(msg);
                }
            }
        }

        pub async fn wait_for_total(&mut self, total: u64, timeout: Duration) {
            let seen = self
                .wait_for(timeout, |m| {
                    m["type"] == "grown" && m["total"].as_u64().is_some_and(|t| t >= total)
                })
                .await;
            assert!(
                seen.is_some(),
                "never saw a grown notification with total >= {total}"
            );
        }

        /// Sends `body` with a fresh id and returns its reply, panicking on an error reply.
        pub async fn request(&mut self, body: Value) -> Value {
            let msg = self.request_raw(body).await;
            assert_eq!(msg["type"], "reply", "error reply: {msg}");
            msg
        }

        /// Sends `body` with a fresh id and returns its reply or error.
        pub async fn request_raw(&mut self, mut body: Value) -> Value {
            let id = self.next_id;
            self.next_id += 1;
            body["id"] = id.into();
            self.inner
                .send(Message::Text(body.to_string().into()))
                .await
                .expect("send request");
            loop {
                let msg = self
                    .read(Duration::from_secs(10))
                    .await
                    .expect("connection closed before the reply");
                if msg["id"] == id {
                    return msg;
                }
                self.pending.push_back(msg);
            }
        }
    }
}
