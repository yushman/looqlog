## 1. Decision records

- [x] 1.1 Write `docs/adr/0010-parse-on-the-backend-thin-client.md` (supersedes ADR-0002 and
      ADR-0007; Consequences note that ADR-0004's per-client backpressure gap is obsolete while
      the ring buffer and snapshot-on-connect stand)
- [x] 1.2 Change the status lines of ADR-0002 and ADR-0007 to `Superseded by ADR-0010`; leave
      their bodies untouched
- [x] 1.3 Update `CLAUDE.md`: architecture diagram, key-constraints list (0002/0007 superseded,
      0010 added), privacy-asymmetry section rewritten to "never leaves your machine" for both
      modes, silent-failure list (gap → eviction marker; WASM latency → native benchmark)

## 2. Engine in `looqlog-core`

- [x] 2.1 Add `index` module: port `web/src/entry-index.ts` (input-order store, sorted timestamp
      refs, timestampless set, chain members, level counts, front eviction, half-open range
      queries, robust span) with its tests ported from `entry-index.test.ts`
- [x] 2.2 Add `query` module: port `search-query.ts` (`re:` prefix, literal escape,
      `field=value` extraction against known fields) and `predicate.ts` (field filters, haystack,
      chain-aware root/member predicate, match ranges) using the `regex` crate; port
      `search-query.test.ts` and `predicate.test.ts`; unsupported look-around/backreference
      yields a compile error with a readable message
- [x] 2.3 Add `bucket` module: port `timeline-bucket.ts` with its test
- [x] 2.4 Add `Session`: `Parser` + index + retained raw lines + current query; `ingest`,
      `finish`, `set_query` (generation counter), `window`, `entry`, `buckets`, `stats`,
      `set_format` (re-parse retained raw lines), detection hold for streams with timeout,
      eviction at a configurable limit; all result types `Serialize`; no I/O, no async
- [x] 2.5 Unit tests for `Session`: filter+search counts, compile error keeps previous match
      set, stale generation, window past end, chain structure in rows, eviction updates counts
      and range results, format override re-parse
- [x] 2.6 `cargo bench` for parse MB/s (JSONL, logfmt, plain), re-filter at 10k and 1M entries,
      retained bytes per MiB of input; record results in `docs/devlog.md`

## 3. Backend wiring in `looqlog`

- [x] 3.1 Replace the raw-line ring buffer in `stdin.rs` with ingestion into a shared
      `Session` (`Arc<Mutex<_>>`), eviction at `--max-lines`; stdin reader still never blocks on
      clients
- [x] 3.2 File mode: read the argv path on a blocking task in 64 KiB chunks into the session;
      read failure becomes a session error state and a terminal message, not a startup failure;
      apply warn/hard cap using the measured footprint from 2.6
- [x] 3.3 Rewrite `protocol.rs`: tagged requests (`query`, `window`, `entry`, `buckets`,
      `set_format`) with `id`, replies with `id` and generation, notifications (`progress`,
      `grown`, `evicted`, `ended`, `reset`), error replies; JSON round-trip tests including
      envelope-like log text
- [x] 3.4 Rewrite `handle_socket` in `server.rs`: token handshake first, then request dispatch on
      a blocking pool; per-client notification coalescing at 100 ms with cumulative values;
      `ended` immediately after auth if stdin already closed; first notification carries totals
- [x] 3.5 Add `POST /upload`: token header, Origin and Host checks (403 before reading body),
      streamed body into a fresh session that replaces the current one only on successful start;
      cancels an in-flight ingest; `reset` to all clients; rejected in stdin mode; failure leaves
      the previous source intact
- [x] 3.6 Add the `Host` check to `/ws`; drop `'wasm-unsafe-eval'` from CSP; remove
      `/wasm/core.js`, `/wasm/core.wasm`, `/assets/worker.js` routes and their `include_bytes!`
- [x] 3.7 CLI: banner names the opened file; nonexistent path prints the error and keeps serving;
      `--host` warning names files and stdin
- [x] 3.8 Integration tests (`tokio` + WS client): stdin line becomes queryable; late connect
      after EOF gets `ended`; eviction announced; foreign Origin/Host rejected on `/ws` and
      `/upload`; upload replaces source; route enumeration contains no `.wasm`/worker

## 4. Thin client in `web/`

- [x] 4.1 Add a query client over `/ws` (request ids, generation discard, reconnect with
      backoff re-issuing the current query and window, notification handlers) with unit tests
- [x] 4.2 Entry table reads windows through the query client with a two-screen overscan cache;
      browser holds no full entry list
- [x] 4.3 Filter bar and search send `query` on change; chips come from the field inventory in
      replies; `field=value` tokens come from the reply; inline compile error keeps the previous
      view
- [x] 4.4 Timeline renders backend `buckets`; range drag sends `query`; throttled refresh on
      `grown`
- [x] 4.5 Detail pane fetches `entry(ordinal)`; evicted entry shows "no longer retained"
- [x] 4.6 File open: argv file shows immediately with progress; picker/drop uploads via
      `POST /upload` with the token header; size warn/cap; upload errors show the backend reason
- [x] 4.7 Live tail: indicator states from notifications, lines/sec from `grown` deltas,
      eviction marker above the first retained row (also on late connect), autoscroll/resume
      unchanged in behavior
- [x] 4.8 Format override (`#format=` and detection UI) sends `set_format`; `reset` triggers
      re-query; url-state grammar unchanged
- [x] 4.9 Mode indicator and all UI copy: "read by the local looqlog process, never leaves the
      machine" for both modes; no "never leaves your browser" anywhere
- [x] 4.10 Delete `worker.ts`, `parser-worker-client.ts`, `parser-api.ts`, `bridge.ts`,
      `core-wasm-types.ts`, `wasm-types.ts`, `entry-index.ts`, `predicate.ts`, `search-query.ts`,
      `timeline-bucket.ts` and their tests once nothing imports them; drop `comlink`;
      `tsc --noEmit` and `vitest` pass

## 5. Build, packaging, CI

- [x] 5.1 Remove `crates/looqlog-wasm` from the workspace and the repo
- [x] 5.2 `scripts/build-frontend.sh`: drop `wasm-pack`/`wasm-opt`; vendored artifacts are the JS
      bundle and stylesheet only; remove `core.wasm`/`core.js`/`worker.js` from
      `crates/looqlog/assets/`; rebuild and commit artifacts
- [x] 5.3 `.github/workflows/ci.yml` and `release.yml`: remove WASM toolchain steps and pins;
      size record covers binary and bundle; drift check covers JS/CSS only
- [x] 5.4 `scripts/smoke-release-binary.sh`: no WASM route check; stdin line becomes queryable by
      an authenticated WS client; `scripts/gen-bench-fixture.py` adjusted if WASM-specific
- [x] 5.5 `cargo package --list` contains no `.wasm`

## 6. Docs

- [x] 6.1 `README.md`: remove WebAssembly/browser-parsing claims and ADR-0002/0007 references;
      `looqlog app.log` opens the file directly; upload from the page; privacy section says
      "never leaves your machine" for both modes; `--host` exposes files and stdin; regex dialect
      note (no look-around/backreferences); eviction marker replaces gap marker; size cap numbers
      from 2.6
- [x] 6.2 `README.ru.md`: the same changes in the same commit
- [x] 6.3 `docs/devlog.md`: entry for this change with the benchmark numbers and the measured
      filter round trip

## 7. Verification

- [x] 7.1 `cargo test --workspace`, `cargo clippy -- -D warnings`, `tsc --noEmit`, `vitest` pass
- [x] 7.2 Manual run with Playwright: `looqlog tests/fixtures/sample.jsonl` shows entries with no
      picker; upload a second fixture replaces it; filter, search (incl. `re:[unclosed` error),
      range drag, detail pane, `#format=` override work; no CSP violation in console
- [x] 7.3 Manual run: `yes | head -n 300000 | looqlog --max-lines 100000` shows the eviction
      marker with the right count; reload keeps history; killing the process shows the lost
      connection message
- [x] 7.4 grep both READMEs and `web/src` for "never leaves your browser", "WebAssembly",
      "WASM" and the Russian equivalents; no stale claims remain
- [x] 7.5 `openspec validate backend-query-engine --strict` passes
