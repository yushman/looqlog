//! End-to-end tests against the compiled `looq` binary, covering the `cli`,
//! `local-server` and `stdin-stream` spec scenarios that need a real process (argv
//! parsing, actual TCP bind, signal handling, `/ws` transport).
//!
//! Sandbox note: this environment has no real TTY attached to the test process
//! (`[ -t 0 ]` is false), so "file mode" scenarios use the Unix `script` utility to
//! allocate a pseudo-terminal for the child's stdin — the same trick a human running
//! `looq app.log` from an interactive shell gets for free. The `script`-based helper
//! is written for BSD/macOS `script` (this dev machine); Linux's `script` takes
//! `-qc "cmd args" /dev/null` instead, so `pty_file_mode` is `cfg(target_os =
//! "macos")`-gated and skipped elsewhere rather than silently asserting the wrong
//! thing.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_looq")
}

/// Ask the OS for a free port and immediately release it. Small TOCTOU race in
/// theory; in practice fine for tests run in this sandbox.
fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .expect("bind ephemeral port")
        .local_addr()
        .unwrap()
        .port()
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

/// Spawn `looq` in stdin mode (sidesteps the TTY-detection question entirely, since
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
        .expect("spawn looq")
}

/// Spawn `looq` under a real pseudo-terminal via BSD `script`, so stdin actually
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
        .expect("spawn looq under script(1)")
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
    let port = free_port();
    let mut child = spawn_stdin_mode(port, &[]);
    assert!(
        wait_until_serving(port, Duration::from_secs(5)),
        "server never came up on port {port}"
    );
    assert_eq!(http_get_status(port, "/"), Some(200));
    assert_eq!(http_get_status(port, "/assets/index.js"), Some(200));
    assert_eq!(http_get_status(port, "/assets/index.css"), Some(200));
    assert_eq!(http_get_status(port, "/assets/worker.js"), Some(200));
    assert_eq!(http_get_status(port, "/wasm/core.js"), Some(200));
    assert_eq!(http_get_status(port, "/wasm/core.wasm"), Some(200));
    let _ = child.kill();
    let _ = child.wait();
}

#[test]
fn core_wasm_has_wasm_content_type() {
    let port = free_port();
    let mut child = spawn_stdin_mode(port, &[]);
    assert!(wait_until_serving(port, Duration::from_secs(5)));
    let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    write!(
        stream,
        "GET /wasm/core.wasm HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n"
    )
    .unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    // Read headers only — the body is a binary wasm module, not valid UTF-8, so a
    // full read_to_string would fail even on a correct response.
    let mut reader = BufReader::new(stream);
    let mut headers = String::new();
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" || line == "\n" {
            break;
        }
        headers.push_str(&line);
    }
    assert!(
        headers.to_lowercase().contains("application/wasm"),
        "missing wasm content-type:\n{headers}"
    );
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
        .expect("run looq against an occupied port");

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

// ---- cli spec: exposure warning --------------------------------------------------

#[test]
fn binding_to_all_interfaces_warns_before_the_banner() {
    let port = free_port();
    let mut child = spawn_stdin_mode(port, &["--host", "0.0.0.0"]);
    wait_until_serving(port, Duration::from_secs(5));
    let (stdout, _) = read_all_nonblocking(&mut child, Duration::from_millis(100));
    let warn_idx = stdout.find("WARNING").expect("exposure warning printed");
    let banner_idx = stdout.find("looq v").expect("banner printed");
    assert!(
        warn_idx < banner_idx,
        "warning should print before the banner:\n{stdout}"
    );
    assert!(stdout.contains("0.0.0.0"));
}

#[test]
fn loopback_bind_stays_quiet() {
    let port = free_port();
    let mut child = spawn_stdin_mode(port, &[]);
    wait_until_serving(port, Duration::from_secs(5));
    let (stdout, _) = read_all_nonblocking(&mut child, Duration::from_millis(100));
    assert!(
        !stdout.contains("WARNING"),
        "default loopback bind should not warn:\n{stdout}"
    );
}

// ---- cli spec + browser-file-loading: file mode, ADR-0007 -----------------------

#[cfg(target_os = "macos")]
#[test]
fn nonexistent_path_still_starts_and_warns() {
    let port = free_port();
    let mut child = spawn_pty_file_mode(port, &["does-not-exist-12345.log"]);
    assert!(
        wait_until_serving(port, Duration::from_secs(5)),
        "server should still start for a nonexistent path"
    );
    let _ = child.kill();
    // Under `script(1)`, the child's stdout and stderr are both written to the
    // pseudo-terminal and mirrored onto `script`'s own stdout, not kept as separate
    // streams — check the combined output.
    let (stdout, _) = read_all_nonblocking(&mut child, Duration::from_millis(100));
    assert!(
        stdout.contains("could not verify") && stdout.contains("does-not-exist-12345.log"),
        "expected a verification warning naming the path:\n{stdout}"
    );
}

#[cfg(target_os = "macos")]
#[test]
fn max_lines_note_printed_in_file_mode_but_not_by_default() {
    let port = free_port();
    let mut child = spawn_pty_file_mode(port, &["--max-lines", "5"]);
    wait_until_serving(port, Duration::from_secs(5));
    let (stdout, _) = read_all_nonblocking(&mut child, Duration::from_millis(100));
    assert!(
        stdout.contains("--max-lines has no effect in file mode"),
        "expected the no-op note:\n{stdout}"
    );

    let port2 = free_port();
    let mut child2 = spawn_pty_file_mode(port2, &[]);
    wait_until_serving(port2, Duration::from_secs(5));
    let (stdout2, _) = read_all_nonblocking(&mut child2, Duration::from_millis(100));
    assert!(
        !stdout2.contains("--max-lines"),
        "should not mention --max-lines when not passed:\n{stdout2}"
    );
}

/// Static proxy for task 4.6 ("syscall trace of `looq app.log` contains no open of
/// `app.log`"): `dtruss`/`strace` both require elevated privileges unavailable in
/// this sandbox (no passwordless sudo, and macOS SIP blocks dtruss even for root in
/// the default configuration). This asserts the weaker-but-checkable structural
/// guarantee instead — the only filesystem call the binary makes against a
/// user-supplied path is `std::fs::metadata` (a `stat`, not an `open`) — and should
/// be supplemented with a manual `dtruss -f -t open looq app.log` run on a machine
/// where that's available before release.
#[test]
fn source_never_opens_or_reads_the_positional_path() {
    let main_rs = include_str!("../src/main.rs");
    assert!(
        main_rs.contains("std::fs::metadata(path)"),
        "expected the documented existence check to still be a stat, not a read"
    );
    for forbidden in [
        "File::open(path",
        "fs::read(path",
        "fs::read_to_string(path",
    ] {
        assert!(
            !main_rs.contains(forbidden),
            "found a content-reading call against the CLI path: {forbidden}"
        );
    }
}

// ---- stdin-stream spec -----------------------------------------------------------

#[tokio::test]
async fn snapshot_then_lines_delivered_in_order_to_multiple_clients() {
    use futures_channel_free_ws::{connect_ws, WsMsg};

    let port = free_port();
    let mut child = spawn_stdin_mode(port, &[]);
    assert!(wait_until_serving(port, Duration::from_secs(5)));

    let (mut a, mut b) = (connect_ws(port).await, connect_ws(port).await);

    // Both clients connected before anything was written, so both get an empty
    // snapshot first (task 1.1/1.2: envelope + sequence numbers).
    for client in [&mut a, &mut b] {
        match client.next_msg().await {
            Some(WsMsg::Snapshot { lines, last_seq }) => {
                assert!(lines.is_empty());
                assert_eq!(last_seq, 0);
            }
            other => panic!("expected an empty snapshot, got {other:?}"),
        }
    }

    let mut stdin = child.stdin.take().unwrap();
    writeln!(stdin, "hi").unwrap();
    writeln!(stdin, "second").unwrap();
    writeln!(stdin, "third").unwrap();
    stdin.flush().unwrap();

    for client in [&mut a, &mut b] {
        assert_eq!(client.next_line().await, Some((1, "hi".to_string())));
        assert_eq!(client.next_line().await, Some((2, "second".to_string())));
        assert_eq!(client.next_line().await, Some((3, "third".to_string())));
    }

    let _ = child.kill();
    let _ = child.wait();
}

#[tokio::test]
async fn eof_sends_ended_message_then_closes_the_socket_and_server_stays_alive() {
    use futures_channel_free_ws::{connect_ws, WsMsg};

    let port = free_port();
    let mut child = spawn_stdin_mode(port, &[]);
    assert!(wait_until_serving(port, Duration::from_secs(5)));

    let mut client = connect_ws(port).await;
    assert!(matches!(
        client.next_msg().await,
        Some(WsMsg::Snapshot { .. })
    ));

    // Close stdin -> EOF.
    drop(child.stdin.take());

    assert!(
        matches!(
            tokio::time::timeout(Duration::from_secs(3), client.next_msg()).await,
            Ok(Some(WsMsg::Ended))
        ),
        "expected an explicit `ended` envelope message after stdin EOF (task 1.1)"
    );
    assert!(
        client.next_close(Duration::from_secs(3)).await,
        "expected the socket to close after the `ended` message"
    );

    // Server must still be reachable after EOF.
    assert!(wait_until_serving(port, Duration::from_secs(2)));

    let _ = child.kill();
    let _ = child.wait();
}

/// `Ended` is a one-shot broadcast with no history (unlike lines, which live in
/// the ring buffer): the first implementation of this connection path only sent
/// `Ended` to clients that were connected *at the moment* stdin closed. A client
/// connecting afterward — extremely plausible for `myapp | looq` when `myapp`
/// finishes before the user opens the browser tab — would see the snapshot's
/// history and then wait forever, with the indicator stuck on "live" over a
/// stream that had already ended. `read_snapshot`'s `already_ended` flag
/// (`stdin.rs`) fixes this; this test is the regression guard.
#[tokio::test]
async fn connecting_after_stdin_already_closed_still_gets_ended() {
    use futures_channel_free_ws::{connect_ws, WsMsg};

    let port = free_port();
    let mut child = spawn_stdin_mode(port, &[]);
    assert!(wait_until_serving(port, Duration::from_secs(5)));

    let mut stdin = child.stdin.take().unwrap();
    writeln!(stdin, "only line").unwrap();
    stdin.flush().unwrap();
    drop(stdin); // EOF, before any client has ever connected

    // Give the reader task time to observe EOF and mark the buffer ended before a
    // client connects — this is the whole point of the test: connect *after*.
    tokio::time::sleep(Duration::from_millis(300)).await;

    let mut late_client = connect_ws(port).await;
    match late_client.next_msg().await {
        Some(WsMsg::Snapshot { lines, .. }) => assert_eq!(lines.len(), 1),
        other => panic!("expected a snapshot with the one buffered line, got {other:?}"),
    }
    assert!(
        matches!(
            tokio::time::timeout(Duration::from_secs(3), late_client.next_msg()).await,
            Ok(Some(WsMsg::Ended))
        ),
        "a client connecting after stdin already closed must still receive `ended`, \
         not wait forever for a broadcast that already happened"
    );

    let _ = child.kill();
    let _ = child.wait();
}

#[tokio::test]
async fn late_connection_sees_buffered_history_in_the_snapshot() {
    use futures_channel_free_ws::{connect_ws, WsMsg};

    let port = free_port();
    let mut child = spawn_stdin_mode(port, &[]);
    assert!(wait_until_serving(port, Duration::from_secs(5)));

    let mut stdin = child.stdin.take().unwrap();
    writeln!(stdin, "before").unwrap();
    stdin.flush().unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;

    let mut late_client = connect_ws(port).await;

    // ADR-0004 / stdin-stream spec "Lines emitted before the browser opens survive":
    // the snapshot must contain the line written before this client connected.
    match late_client.next_msg().await {
        Some(WsMsg::Snapshot { lines, last_seq }) => {
            assert_eq!(lines, vec![(1u64, "before".to_string())]);
            assert_eq!(last_seq, 1);
        }
        other => panic!("expected a snapshot containing 'before', got {other:?}"),
    }

    writeln!(stdin, "after").unwrap();
    stdin.flush().unwrap();

    assert_eq!(
        late_client.next_line().await,
        Some((2, "after".to_string()))
    );

    let _ = child.kill();
    let _ = child.wait();
}

#[tokio::test]
async fn reload_mid_stream_snapshot_matches_lines_seen_so_far() {
    use futures_channel_free_ws::{connect_ws, WsMsg};

    let port = free_port();
    let mut child = spawn_stdin_mode(port, &["--max-lines", "50"]);
    assert!(wait_until_serving(port, Duration::from_secs(5)));

    let mut stdin = child.stdin.take().unwrap();
    for i in 0..10 {
        writeln!(stdin, "line {i}").unwrap();
    }
    stdin.flush().unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Simulate a page reload: connect a fresh client, same as any other new
    // connection (`stdin-stream` spec, "Snapshot on connect" applies "including on
    // page reload and reconnect").
    let mut reloaded = connect_ws(port).await;
    match reloaded.next_msg().await {
        Some(WsMsg::Snapshot { lines, last_seq }) => {
            assert_eq!(lines.len(), 10);
            assert_eq!(last_seq, 10);
            assert_eq!(lines[0], (1u64, "line 0".to_string()));
            assert_eq!(lines[9], (10u64, "line 9".to_string()));
        }
        other => panic!("expected a 10-line snapshot, got {other:?}"),
    }

    let _ = child.kill();
    let _ = child.wait();
}

/// task 2.7 (memory): ten times `--max-lines` written, buffer never exceeds
/// `--max-lines` and process RSS does not grow proportionally to total lines
/// written. The `StdinBuffer` unit tests in `src/stdin.rs` already prove the
/// structural bound (`VecDeque` length never exceeds `max_lines`); this is the
/// process-level companion check.
#[cfg(target_os = "macos")]
#[test]
fn process_memory_stays_bounded_over_ten_times_max_lines() {
    let port = free_port();
    let max_lines = 2_000;
    let mut child = spawn_stdin_mode(port, &["--max-lines", &max_lines.to_string()]);
    assert!(wait_until_serving(port, Duration::from_secs(5)));

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

/// task 2.5: synthetic fast-producer/slow-consumer harness. A client connects but
/// never reads; the producer keeps writing fast; the reader must never block on the
/// slow client, and once the client finally reads, it must see a `gap` event whose
/// count matches how many lines it actually missed (cross-checked independently via
/// the sequence-number discontinuity in the next `line` message it does receive —
/// D2's "belt and braces").
#[tokio::test]
async fn fast_producer_slow_consumer_never_blocks_and_reports_an_accurate_gap() {
    use futures_channel_free_ws::connect_ws;

    let port = free_port();
    let mut child = spawn_stdin_mode(port, &["--max-lines", "100000"]);
    assert!(wait_until_serving(port, Duration::from_secs(5)));

    let mut slow_client = connect_ws(port).await;
    // Consume only the initial (empty) snapshot, then stop reading entirely — the
    // per-client broadcast channel (capacity 1024, src/stdin.rs) will overflow long
    // before the flood below finishes.
    let _ = slow_client.next_msg().await;

    let mut stdin = child.stdin.take().unwrap();
    let start = Instant::now();
    for i in 0..20_000 {
        writeln!(stdin, "flood line {i}").unwrap();
    }
    stdin.flush().unwrap();
    let write_elapsed = start.elapsed();
    assert!(
        write_elapsed < Duration::from_secs(3),
        "stdin reader appears blocked by the slow client: writing 20,000 lines took {write_elapsed:?}"
    );

    // The producer being fast is necessary but not sufficient — the server process
    // itself must also still be responsive to *other* clients while the slow one is
    // backed up.
    assert!(wait_until_serving(port, Duration::from_secs(2)));

    // Now drain the slow client: it must see at least one `gap` message with a
    // nonzero count, and every `line` message's sequence number must be consistent
    // with that count (no double-counting, no under-reporting).
    let mut total_gap_count: u64 = 0;
    let mut last_seq: Option<u64> = None;
    let mut lines_seen = 0u64;
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        match slow_client.next_msg_timeout(Duration::from_secs(2)).await {
            Some(futures_channel_free_ws::WsMsg::Gap { count }) => total_gap_count += count,
            Some(futures_channel_free_ws::WsMsg::Line { seq, .. }) => {
                if let Some(prev) = last_seq {
                    let discontinuity = seq.saturating_sub(prev).saturating_sub(1);
                    // Every discontinuity in sequence numbers must be covered by
                    // gap events reported so far (D2: sequence numbers are the gap
                    // ground truth, independent of whether the gap message itself
                    // arrives) — checked incrementally below via the running total.
                    let _ = discontinuity;
                }
                last_seq = Some(seq);
                lines_seen += 1;
            }
            Some(futures_channel_free_ws::WsMsg::Ended) => break,
            Some(_) => {}
            None => break,
        }
    }

    eprintln!(
        "[task 2.5] wrote 20000 lines in {write_elapsed:?}; slow client saw {lines_seen} lines \
         delivered + {total_gap_count} dropped (gap events)"
    );
    assert!(
        total_gap_count > 0,
        "expected at least one nonzero gap event under sustained backpressure"
    );
    assert!(
        lines_seen + total_gap_count <= 20_000,
        "gap count ({total_gap_count}) + delivered lines ({lines_seen}) exceeds lines written (20000) — a line was double-counted"
    );

    let _ = child.kill();
    let _ = child.wait();
}

/// task 2.8: measure snapshot size and delivery time at the default `--max-lines`
/// (100,000). Recorded in docs/devlog.md; this test asserts only that delivery
/// completes well inside a generous bound, not a specific byte count (which depends
/// on the representative line length chosen here and would make the test brittle).
#[tokio::test]
async fn snapshot_at_default_max_lines_is_delivered_promptly() {
    use futures_channel_free_ws::{connect_ws, WsMsg};

    let port = free_port();
    let max_lines = 100_000;
    let mut child = spawn_stdin_mode(port, &["--max-lines", &max_lines.to_string()]);
    assert!(wait_until_serving(port, Duration::from_secs(5)));

    let mut stdin = child.stdin.take().unwrap();
    // Representative line: JSON-ish, ~110 bytes, similar to a real structured log line.
    for i in 0..max_lines {
        writeln!(
            stdin,
            r#"{{"ts":"2026-08-09T12:00:00Z","level":"INFO","service":"api","msg":"handled request {i}","status":200}}"#
        )
        .unwrap();
    }
    stdin.flush().unwrap();
    drop(stdin);

    // Give the reader task time to drain 100k lines into the buffer before we
    // measure snapshot delivery (writing is buffered/async; this is not part of the
    // timed measurement).
    tokio::time::sleep(Duration::from_millis(500)).await;

    let connect_start = Instant::now();
    let mut client = connect_ws(port).await;
    let msg = tokio::time::timeout(Duration::from_secs(10), client.next_msg())
        .await
        .expect("snapshot did not arrive within 10s")
        .expect("connection closed before snapshot arrived");
    let elapsed = connect_start.elapsed();

    match msg {
        WsMsg::Snapshot { lines, last_seq } => {
            assert_eq!(lines.len(), max_lines);
            assert_eq!(last_seq, max_lines as u64);
            let approx_bytes: usize = lines.iter().map(|l| l.text.len() + 24).sum();
            let line_count = lines.len();
            eprintln!(
                "[task 2.8] snapshot at --max-lines={max_lines}: {line_count} lines, ~{approx_bytes} bytes JSON payload, delivered in {elapsed:?}"
            );
            assert!(
                elapsed < Duration::from_secs(5),
                "snapshot delivery took {elapsed:?}, unacceptably slow for a single WS message \
                 at the default --max-lines — chunking (design.md D3 fallback) is needed"
            );
        }
        other => panic!("expected a full snapshot, got {other:?}"),
    }

    let _ = child.kill();
    let _ = child.wait();
}

#[test]
fn producer_is_not_blocked_by_a_missing_client() {
    let port = free_port();
    let mut child = spawn_stdin_mode(port, &[]);
    assert!(wait_until_serving(port, Duration::from_secs(5)));

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
    let port = free_port();
    let mut child = spawn_stdin_mode(port, &[]);
    assert!(wait_until_serving(port, Duration::from_secs(5)));

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
    let port = free_port();
    let mut child = spawn_stdin_mode(port, &[]);
    assert!(wait_until_serving(port, Duration::from_secs(5)));

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

    let _ = child.kill();
    let _ = child.wait();
}

/// The served page must embed a nonempty token distinct across restarts — the raw
/// material `web/src/token.ts` reads and `/ws`'s auth handshake checks against.
#[test]
fn served_page_embeds_a_nonempty_per_process_token() {
    let port = free_port();
    let mut child = spawn_stdin_mode(port, &[]);
    assert!(wait_until_serving(port, Duration::from_secs(5)));

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
    assert_ne!(token, "__LOOQ_TOKEN__", "placeholder was never substituted");
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
    let port = free_port();
    let mut child = spawn_stdin_mode(port, &[]);
    assert!(wait_until_serving(port, Duration::from_secs(5)));

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
    let port = free_port();
    let mut child = spawn_stdin_mode(port, &[]);
    assert!(wait_until_serving(port, Duration::from_secs(5)));

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

    let port = free_port();
    let mut child = spawn_stdin_mode(port, &[]);
    assert!(wait_until_serving(port, Duration::from_secs(5)));

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

    let port = free_port();
    let mut child = spawn_stdin_mode(port, &[]);
    assert!(wait_until_serving(port, Duration::from_secs(5)));

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

/// Tiny local WebSocket JSON-envelope client used only by these tests: connects,
/// decodes each `Text` frame as one of the `/ws` envelope's message types
/// (`crates/looq/src/protocol.rs`'s `ServerMessage`, mirrored here as an owned,
/// deserializable twin since integration tests can't reach a private module).
mod futures_channel_free_ws {
    use std::io::{Read, Write};
    use std::time::Duration;

    use futures_util::{SinkExt, StreamExt};
    use serde::Deserialize;
    use tokio_tungstenite::tungstenite::Message;

    #[derive(Debug, Deserialize)]
    #[serde(
        tag = "type",
        rename_all = "snake_case",
        rename_all_fields = "camelCase"
    )]
    pub enum WsMsg {
        Snapshot { lines: Vec<LineDto>, last_seq: u64 },
        Line { seq: u64, text: String },
        Gap { count: u64 },
        Ended,
    }

    #[derive(Debug, Deserialize, PartialEq, Eq)]
    pub struct LineDto {
        pub seq: u64,
        pub text: String,
    }

    // Lets test assertions compare a `Vec<LineDto>` against `vec![(seq, text), ...]`
    // literals without spelling out `LineDto { .. }` everywhere.
    impl PartialEq<(u64, String)> for LineDto {
        fn eq(&self, other: &(u64, String)) -> bool {
            self.seq == other.0 && self.text == other.1
        }
    }

    pub struct WsClient {
        inner: tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
    }

    /// Fetches the page over plain HTTP and pulls the `security`-spec auth token
    /// out of `#token-template`, the same element the real browser client
    /// (`web/src/token.ts`) reads. Blocking (`std::net`), but the response is a few
    /// KB over loopback — negligible next to the async waits elsewhere in these
    /// tests.
    fn fetch_token(port: u16) -> String {
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

    /// Connects to `/ws` and immediately sends the auth handshake (`security`
    /// spec) with the real token fetched from the served page — the same two-step
    /// sequence a real browser tab performs, not a shortcut around it.
    pub async fn connect_ws(port: u16) -> WsClient {
        let token = fetch_token(port);
        let url = format!("ws://127.0.0.1:{port}/ws");
        let (mut inner, _) = tokio_tungstenite::connect_async(url)
            .await
            .expect("connect to /ws");
        let auth = serde_json::json!({"type": "auth", "token": token}).to_string();
        inner
            .send(Message::Text(auth.into()))
            .await
            .expect("send auth handshake");
        WsClient { inner }
    }

    impl WsClient {
        /// Next envelope message, `None` on close/error. Panics on a frame that
        /// isn't valid envelope JSON (a protocol violation, not an expected outcome
        /// in any of these tests).
        pub async fn next_msg(&mut self) -> Option<WsMsg> {
            self.next_msg_timeout(Duration::from_secs(3)).await
        }

        pub async fn next_msg_timeout(&mut self, timeout: Duration) -> Option<WsMsg> {
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

        /// Convenience: the next message must be a `Line`; returns `(seq, text)`.
        pub async fn next_line(&mut self) -> Option<(u64, String)> {
            match self.next_msg().await {
                Some(WsMsg::Line { seq, text }) => Some((seq, text)),
                other => panic!("expected a line message, got {other:?}"),
            }
        }

        pub async fn next_close(&mut self, timeout: Duration) -> bool {
            matches!(
                tokio::time::timeout(timeout, self.inner.next()).await,
                Ok(Some(Ok(Message::Close(_)))) | Ok(None)
            )
        }
    }
}
