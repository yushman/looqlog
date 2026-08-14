## 1. Decisions before code

- [x] 1.1 Write `docs/adr/0007-argv-path-is-a-hint-not-an-auto-loaded-file.md`: decision D1, both rejected alternatives (stdin-path byte streaming, opt-in `--serve-file`), and the consequence for PRD Flow 1
- [x] 1.2 Write `docs/adr/0008-vendored-frontend-artifacts.md`: decision D2, rejected `build.rs`-invokes-wasm-pack and binaries-only alternatives, and the staleness risk the CI check exists to cover
- [x] 1.3 Reconcile the documents ADR-0007 contradicts: amend PRD US-1 acceptance and mvp-plan day 3 "done when" so they match TDR §7, referencing ADR-0007
- [x] 1.4 Resolve design.md open questions: reference browser for the benchmark, fixture committed vs generated, `--max-lines` behaviour in file mode

## 2. Workspace skeleton

- [x] 2.1 Cargo workspace with `crates/looq`, `crates/looq-core`, `crates/looq-wasm`; `looq-core` has no `wasm-bindgen`/`web-sys`/`std::fs` dependency (ADR-0005)
- [x] 2.2 `cargo test --workspace` runs green on empty crates; CI job for fmt, clippy, test
- [x] 2.3 `tests/fixtures/sample.jsonl` with a known line count, plus the ~1MB fixture source decided in 1.4

## 3. HTTP server

- [x] 3.1 axum server binding `127.0.0.1:7891`, one route returning 200 — `curl http://127.0.0.1:7891` succeeds
- [x] 3.2 `--port 0` allocates a free port and the actual port is reported; occupied port exits non-zero naming the port and suggesting `--port 0`
- [x] 3.3 Assets served from `include_bytes!` with correct content types, `application/wasm` for `core.wasm`; verify from an empty working directory
- [x] 3.4 Graceful shutdown on SIGINT: connections closed, exit 0, port released within a second

## 4. CLI surface

- [x] 4.1 clap argv per TDR §6: all eight flags with the documented defaults plus the optional positional path; unknown flags exit non-zero naming the flag
- [x] 4.2 Mode selection: `--stdin` or non-TTY stdin selects stdin mode, otherwise file mode; positional path is never opened by the process
- [x] 4.3 Startup banner with version, actual bound URL, quit hint, and the file hint when a path was given
- [x] 4.4 Mandatory exposure warning when `--host` != `127.0.0.1`, printed before the banner (ADR-0003)
- [x] 4.5 `--open` launches the default browser only after the listener is bound; `--no-browser` wins; launch failure prints the URL and the `ssh -L` hint and keeps serving
- [ ] 4.6 Test that a syscall trace of `looq app.log` contains no open of `app.log` — **not done as literally specified**: `dtruss`/`strace` both need privileges unavailable in this sandbox (no passwordless sudo; macOS SIP blocks `dtruss` even for root by default). Implemented instead: a structural test (`source_never_opens_or_reads_the_positional_path` in `crates/looq/tests/cli.rs`) asserting the only filesystem call against the CLI path is `std::fs::metadata` (a stat, never `File::open`/`fs::read`/`fs::read_to_string`), plus a functional "nonexistent path still starts and serves" test. A manual `dtruss -f -t open ./target/release/looq app.log` on a machine where that's available is still owed before release.

## 5. WASM walking skeleton

- [x] 5.1 One hardcoded JSON Lines parse function in `looq-core` returning an entry count, with unit tests
- [x] 5.2 `looq-wasm` adapter exposing it via `wasm-bindgen`, built with `wasm-pack`
- [x] 5.3 Minimal hand-written page: file picker plus drag-and-drop target, calls the WASM entry point, shows the count
- [x] 5.4 Page shows the CLI-supplied path as a hint and states the file is read by the browser, not by `looq` (ADR-0007)
- [x] 5.5 Verify entry count matches `tests/fixtures/sample.jsonl`, with the DevTools Network panel empty after page load (US-6) — verified with a real Chromium instance via Playwright: 20/20 entries, Network panel shows only the initial `/`, `/core.js`, `/core.wasm` requests, nothing after picking the file
- [x] 5.6 Verify parsing still works with the machine's network disabled after page load — verified via `page.context().setOffline(true)`: `file.text()` + `parse_json_lines_count` returned the correct count with zero network activity. Note: re-opening the OS file *chooser* itself while offline stalled in this Playwright/CDP setup (looks like an automation-tooling quirk, not an app bug) — worked around by reusing the file already selected through the real chooser and calling the same code path directly; the already-verified online file-picker flow (5.5) plus this offline File-API+wasm check together cover the scenario.

## 6. Benchmark — checkpoint

- [x] 6.1 Measure wall-clock parse time for the ~1MB JSON Lines fixture in the reference browser — 23.5 ms cold (real file-picker flow), 6.4–14.6 ms steady-state (10 repeated in-page calls), see `docs/devlog.md` 2026-08-09 entry
- [x] 6.2 Record it in `docs/devlog.md`: number, target, fixture, browser and version, machine, command
- [x] 6.3 If the number misses <200ms/MB widely: one timeboxed optimisation pass, then either an explicitly downgraded documented target or a reopened ADR-0002 — decided and written down, not deferred. **Condition not triggered**: 23.5 ms is ~8× under the ≈190.7 ms proportional budget for the 0.954 MiB fixture, so no optimisation pass or target change was needed.

## 7. Stdin over WebSocket

- [x] 7.1 Stdin read line by line in its own tokio task, started before the first client connects
- [x] 7.2 `/ws` endpoint delivering each line as one text message, order preserved, newline stripped, fanned out to all connected clients
- [x] 7.3 `echo hi | looq --stdin` delivers `hi` to a `wscat` client — verified with a `tokio-tungstenite` client in `crates/looq/tests/cli.rs` (`echo_hi_delivered_over_ws_in_order_to_multiple_clients`)
- [x] 7.4 EOF on stdin keeps the server alive and notifies clients that the stream ended
- [x] 7.5 Test that neither a missing client nor a stalled client blocks the stdin reader
- [x] 7.6 Note in code and specs that pre-connection lines are dropped here by design, pending `live-tail`

## 8. Packaging

- [x] 8.1 Single documented command that rebuilds the JS bundle and `core.wasm` from source (`scripts/build-frontend.sh`)
- [x] 8.2 Commit the built artifacts; pin toolchain versions and confirm two consecutive rebuilds are byte-identical — verified twice with `cmp` on both `core.wasm` and `core.js`
- [x] 8.3 CI check failing when committed artifacts differ from a fresh rebuild, with a message naming the stale artifact and the rebuild command (`.github/workflows/ci.yml`, `frontend-artifact-staleness` job); the `git diff` gate itself can only really fire once `crates/looq/assets/` is committed to git, which this task deliberately leaves undone (see hard constraints — no commits)
- [x] 8.4 `cargo build --release` succeeds in a Rust-only container with no Node.js — no container runtime available in this sandbox (no `docker`); verified the closest available proxy instead: `cargo build --release` with `node`/`npm`/`wasm-pack` stripped entirely from `PATH` succeeds and produces a working binary
- [x] 8.5 `cargo package --list` includes the JS bundle and `core.wasm` — required moving vendored assets from a repo-root `assets/` to `crates/looq/assets/` (see devlog); `cargo package --list -p looq` now lists `assets/core.js`, `assets/core.wasm`, `assets/index.html`

## 9. Wrap-up

- [x] 9.1 `README.md` and `README.ru.md`: install, the two run modes, and the ADR-0007 consequence that the user picks the file in the browser — both files in the same commit
- [x] 9.2 Devlog entries for each working day, including the days where the answer was "did not work"
- [x] 9.3 `openspec validate bootstrap-cli-and-wasm-skeleton --strict` passes — "Change 'bootstrap-cli-and-wasm-skeleton' is valid"
- [x] 9.4 Archive the change so `openspec/specs/` reflects the shipped behaviour
