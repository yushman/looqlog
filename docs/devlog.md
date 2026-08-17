# Devlog

Appended at the end of every working day. Real errors, real measured numbers with the
command that produced them, decisions and their rejected alternatives.

## 2026-08-09 — `bootstrap-cli-and-wasm-skeleton`: walking skeleton end to end

Covers mvp-plan days 1–5 plus the day-23 CLI flags, implemented as one OpenSpec
change rather than day-by-day, per the change's own scope.

**Shipped:**
- Cargo workspace: `crates/looq` (bin), `crates/looq-core` (target-agnostic parser,
  ADR-0005 — no `wasm-bindgen`/`web-sys`/`std::fs`), `crates/looq-wasm`
  (`wasm-bindgen` adapter).
- `axum` HTTP server on `127.0.0.1:7891` by default; `--port 0` allocates a free port
  via the OS; occupied-port and bad-`--host` failures exit non-zero with a message
  naming the problem, no panics.
- Full `clap` argv surface (`--port`, `--host`, `--open`, `--no-browser`, `--stdin`,
  `--max-lines`, plus auto `--version`/`--help`) and the positional path.
- Mandatory, unsuppressible stdout warning when `--host != 127.0.0.1` (ADR-0003),
  printed before the ready banner.
- `--max-lines` in file mode prints a one-line "has no effect" note instead of
  silently doing nothing (resolved design.md open question).
- One hardcoded JSON-Lines parser in `looq-core` (`parse_json_lines`): counts valid
  lines, collects a warning per malformed line instead of dropping it silently.
  Exposed to the browser as `parse_json_lines_count` via `looq-wasm`.
- Hand-written page (`web/index.html`) with a file picker + drag-and-drop target,
  loading the wasm module and displaying the parsed entry count and elapsed time.
  Shows the CLI-supplied path as a hint, states plainly that the browser reads the
  file, not `looq` (ADR-0007).
- `/ws`: stdin read line-by-line in its own tokio task via `tokio::sync::broadcast`,
  fanned out to all connected clients, in order, newline stripped. No ring buffer in
  this change by design (ADR-0004 lands in `live-tail`) — a `broadcast` channel gives
  "late subscribers only see new messages" and "sender never blocks" for free, which
  happens to be exactly this change's provisional contract. EOF sends a WebSocket
  close frame (code 1000) instead of a line-shaped payload, so it can never collide
  with real log content.
- Vendored frontend artifacts committed at `crates/looq/assets/{index.html,core.js,
  core.wasm}` (ADR-0008) — **inside** the `looq` crate directory specifically, after
  discovering mid-build that `cargo package`/`cargo publish` only ships files under
  the crate root being published; a repo-root `assets/` referenced via
  `include_bytes!("../../../assets/...")` compiles and runs locally but silently
  vanishes from `cargo package --list`. Caught by actually running `cargo package
  --list -p looq` rather than assuming the include path would travel with the crate.
  `scripts/build-frontend.sh` rebuilds them from `web/` + `crates/looq-wasm` via
  `wasm-pack build --target web`; two consecutive rebuilds are byte-identical
  (verified with `cmp`, both `core.wasm` and `core.js`).
- `.github/workflows/ci.yml`: fmt, clippy (`-D warnings`), `cargo test --workspace`,
  a Node-less `cargo build --release` + `cargo package --list` job, and a frontend
  staleness job that rebuilds and fails on `git diff` against the committed assets.
- ADR-0007 (argv path is a hint, not an auto-loaded file) and ADR-0008 (vendored
  frontend artifacts) written before the code that depends on them; PRD US-1 and
  mvp-plan day 3 amended to match TDR §7 instead of contradicting it.
- `tests/fixtures/sample.jsonl` (20 known lines, committed) and
  `scripts/gen-bench-fixture.py` (deterministic ~1MB generator, not committed —
  confirmed byte-identical across two runs via `diff`).

**Tests:** 28 total, all passing — `cargo test --workspace`: 8 unit tests in `looq`
(cli mode-selection logic, asset templating/escaping, stdin broadcast semantics),
16 integration tests in `crates/looq/tests/cli.rs` against the actual compiled
binary (bind/port/content-type, exposure warning ordering, `/ws` echo/order/
multi-client/EOF/late-connect, SIGINT-to-exit-0-within-1s, producer-not-blocked),
4 unit tests in `looq-core` (JSON-Lines counting, blank-line skipping, malformed-line
warning, empty input).

**Sandbox limitation, noted rather than glossed over:** this environment has no real
TTY (`[ -t 0 ]` is false for the test process), so file-mode integration tests use
`script -q /dev/null <bin> <args>` (BSD/macOS) to get a real pseudo-terminal —
gated `#[cfg(target_os = "macos")]` and skipped elsewhere rather than silently
asserting the wrong thing on Linux CI. Task 4.6's actual ask — a syscall trace
proving no `open()` of the positional path — needs `dtruss`/`strace`, both of which
need privileges this sandbox doesn't have (no passwordless sudo; macOS SIP blocks
`dtruss` even for root by default). Implemented instead as a structural check
(`source_never_opens_or_reads_the_positional_path`, greps `main.rs` for the absence
of `File::open(path`/`fs::read(path`/`fs::read_to_string(path`, and the presence of
the documented `std::fs::metadata(path)` stat-only check) plus the functional
"nonexistent path still starts and serves" test. **A manual
`dtruss -f -t open ./target/release/looq app.log` run on a machine where that's
available is still owed before release.**

**Day-4 benchmark — the riskiest assumption in the project:**

```
python3 scripts/gen-bench-fixture.py > target/bench-1mb.jsonl
# generated 7987 lines, 1000114 bytes (0.954 MiB)
```

Measured by driving a real Chromium instance (Playwright MCP) against the release
binary (`cargo build --release && ./target/release/looq --stdin --port 7999`),
navigating to `http://127.0.0.1:7999/`, and picking `target/bench-1mb.jsonl` through
the page's actual file input (the same code path a user hits, not a synthetic
in-page-only call).

- **Browser:** Chromium 151.0.0.0 (via Playwright), `navigator.userAgent`:
  `Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like
  Gecko) Chrome/151.0.0.0 Safari/537.36`
- **Machine:** Apple M4, macOS 15.7.7 (arm64)
- **Result, first real parse through the UI** (`file.text()` + `parse_json_lines_count`,
  timed with `performance.now()` around the wasm call, displayed by the page itself):
  **23.5 ms** for 1,000,114 bytes / 7987 entries.
- **Result, steady state** (10 repeated in-page calls on the already-read text,
  same wasm export, no UI/File-API overhead): ranged from 14.6 ms (first, still
  JIT-warming) down to 6.4–6.9 ms once warmed up.
- **Target (TDR §11):** < 200 ms/MB. Proportionally for this 0.954 MiB fixture that's
  ≈190.7 ms.
- **Verdict: comfortably under target** — 23.5 ms cold is ~8× under the proportional
  budget; the warmed-up steady state is ~28× under. ADR-0002 survives contact with
  reality; no optimization pass or downgraded target needed (task 6.3 not triggered).
  Caveat honestly: this is a trivial hardcoded JSON-Lines counter (`serde_json::Value`
  parse + a counter), not the real multi-format parser with field extraction and
  indexing that `log-parsing-core` will build — today's number is a floor on
  plausibility, not a guarantee the real parser hits the same ratio.

Also verified while driving the real browser: the Network panel shows exactly three
requests for the whole session (`/`, `/core.js`, `/core.wasm`) — none after picking
either fixture, matching US-6. Parsing was also confirmed to work with
`page.context().setOffline(true)` (Playwright's network emulation) using the same
file already selected via the real OS file chooser — direct evaluation of `file.text()`
+ `parse_json_lines_count` under `setOffline(true)` returned the correct count (20)
with zero network activity. (Note: re-triggering the *file chooser* itself while
offline stalled — Playwright's `setInputFiles`/file-chooser dialog path appears to
depend on something in Chromium's automation plumbing that offline-mode blocks; this
looks like a Playwright/CDP quirk, not an application bug, since the actual File
API + wasm parse path was independently confirmed offline-safe.)

**Decisions made without a human available to ask, recorded per the task's own
instructions:**
- Benchmark reference browser: Chromium via Playwright (resolved design.md open
  question) — the only automatable, reproducible option in this environment.
- `--max-lines` in file mode: parsed, reported, prints a no-op note rather than
  erroring or staying silent.
- Vendored assets live inside `crates/looq/assets/`, not a repo-root `assets/` as
  the proposal's directory sketch implied — required for `cargo package` to actually
  ship them; recorded here rather than filed as a silent deviation from the change's
  own "Impact" section.

## 2026-08-09 — `log-parsing-core`: real parsers replace the JSON-only stub

Covers mvp-plan days 6, 7, 8 and 13, implemented as one OpenSpec change per the
change's own scope (they all write to `looq-core` and the same spec surface).

**What shipped:** the incremental `Parser` (byte chunks in, `Entry` values out,
holding a trailing partial line across chunk boundaries — same code path for file
mode and stdin mode, design.md D1); JSON Lines, logfmt and plain-text parsers, all
routed through one shared `extract()` for timestamp/level/message precedence so that
logic exists in exactly one place; format auto-detection over the first 100
non-empty lines at an 80% threshold (JSON → logfmt → plain priority order),
reporting the chosen format, match fraction and threshold-vs-fallback outcome;
`Entry` with optional timestamp/level, message, input ordinal, arbitrary fields;
UTF-8 decoding with a per-line latin-1 fallback that is correct across chunk
boundaries because decoding is only attempted once a whole line has been
reassembled; bounded, aggregated diagnostics (line, reason, detail; exact
per-reason counts kept past the retention cap); a field inventory with a per-field
distinct-value cap and a `high_cardinality` flag. 57 tests in `looq-core`: 35 unit
(one per spec scenario, roughly) plus 22 integration tests in
`crates/looq-core/tests/parser_integration.rs` against 9 fixtures in
`crates/looq-core/tests/fixtures/` (one per format, custom fields, malformed,
latin-1, out-of-order/missing timestamps, a 5-line stack trace, multi-byte UTF-8),
covering the chunk-split harness (whole input vs. adversarial 1/3/7/17/64-byte
chunk splits, including mid-multi-byte-character), the 1,000,000-bad-line and
50,000-distinct-value cap tests, and the "every skipped line is accounted for"
invariant. `cargo test --workspace`: 81 passed, 0 failed (8 + 16 in `looq`, 35 + 22
in `looq-core`).

**Benchmark — native (task 6.2), `cargo bench -p looq-core`, Apple M4, macOS
15.7.7, release profile (opt-level="s", lto=true), `criterion` 0.5, 100 samples per
format, deterministic ~1MB generated fixtures matching
`scripts/gen-bench-fixture.py`'s shape:**

| format | mean time / 1MB | throughput | vs. TDR §11 (<200ms/MB) |
|---|---|---|---|
| JSON   | 70.0 ms | 13.6 MiB/s | 2.9x under |
| logfmt | 94.9 ms | 10.0 MiB/s | 2.1x under |
| plain  | 119.6 ms | 8.0 MiB/s | 1.7x under |

All three comfortably inside target even in the slowest case (plain, which pays
for a leading-timestamp scan attempt on every line). Command:
`cargo bench -p looq-core` (bench defined in `crates/looq-core/benches/parse.rs`,
`harness = false`).

**`core.wasm` size (task 6.4) — first measurement blew the ~300KB TDR §5 budget by
~3.4x:**

```
scripts/build-frontend.sh   # with serde_json + regex + chrono
ls -la crates/looq/assets/core.wasm   # 1,046,290 bytes (1021.8 KB)
```

Isolated the cause: `level.rs`'s scanner was already hand-rolled (no `regex`); the
only `regex` usage left was the single leading-timestamp pattern in
`timestamp.rs`. Replaced it with a hand-rolled byte scanner
(`timestamp::match_leading_timestamp`, same
`\d{4}-\d{2}-\d{2}[T ]\d{2}:\d{2}:\d{2}(\.\d+)?(Z|[+-]\d{2}:?\d{2})?` pattern,
byte-by-byte, every branch proven to only advance across matched ASCII so the
returned slice is always a valid `str` boundary) and dropped the `regex`
dependency entirely — this is the exact fallback design.md D9's risk section
named ("a hand-written level/timestamp scanner is the fallback if the budget
breaks"), not an improvised deviation. Re-measured:

```
bash scripts/build-frontend.sh   # serde_json + chrono only
ls -la crates/looq/assets/core.wasm   # 158,016 bytes (154.3 KB)
```

154.3KB, under the ~300KB budget with headroom, vs. 44,183 bytes (43.1KB) for the
`bootstrap-cli-and-wasm-skeleton` stub build. All 57 `looq-core` tests, `cargo fmt
--check` and `cargo clippy --all-targets -- -D warnings` re-verified green after
the swap (behavior-preserving: same 35+22 tests pass). `cargo tree -p looq-core
--edges normal` confirms the runtime dependency graph is now only `chrono` and
`serde_json` (`regex` still appears transitively through `criterion`, a
dev-dependency that never ships in `core.wasm`).

**Browser re-measurement (task 6.3), same fixture and methodology as day 4** —
`target/bench-1mb.jsonl` (7987 lines / 1,000,114 bytes, `scripts/gen-bench-fixture.py`),
`cargo build --release -p looq && ./target/release/looq --stdin --port 7999`,
driven by Chromium 151.0.0.0 via Playwright MCP, file picked through the page's
real `<input type=file>` (same code path a user hits):

- Day-4 stub (`parse_json_lines_count`, JSON-only counter, no field extraction) on
  this run: **18.1 ms** for 7987 entries (page's own display) — consistent with
  day 4's original 23.5 ms cold / 6.4–6.9 ms warm on the same fixture, different
  machine run.
- Real parser (`parse_auto_detect_count` — new minimal `wasm-bindgen` export added
  to `looq-wasm` purely to get this number, no comlink/serde-wasm-bindgen/UI wiring
  per this change's non-goals; does full auto-detection, timestamp/level/field
  extraction, and field-inventory bookkeeping for every line): called 10x in-page
  via `performance.now()` around each call — **107.4 ms first call**, settling to
  **~75 ms** steady-state (74.9–79.8 ms over the last 6 calls). Both entry counts:
  7987/7987, matching the stub and the fixture's line count exactly; zero console
  warnings (clean fixture, no diagnostics expected).
- Target (TDR §11): <200ms/MB; proportionally for 0.954 MiB that's ≈190.7 ms
  (same math as day 4).
- **Verdict:** real parser is ~4-11x slower than the day-4 stub (expected — it does
  orders of magnitude more work: per-line `serde_json::Value` parse *and* field
  extraction *and* timestamp/level parsing *and* a `BTreeMap` per entry *and*
  field-inventory recording, vs. the stub's parse-and-discard), but still
  comfortably inside the TDR §11 budget: cold ~1.8x under, steady-state ~2.5x
  under. No optimization pass triggered (task 6.3's "if not met" branch not hit).

**Caps (task 6.1) — measured, not guessed.** `crates/looq-core/examples/mem_probe.rs`
(throwaway probe, not part of the public API) builds an uncapped `Diagnostics`/
`FieldInventory` with N items and prints its counts; wrapped with `/usr/bin/time -l`
(macOS, reports peak RSS) against an N=0 baseline to isolate the process's own
allocator/binary overhead:

```
cargo build -p looq-core --release --example mem_probe
/usr/bin/time -l ./target/release/examples/mem_probe diagnostics 0        # baseline: 1,343,488 bytes RSS
/usr/bin/time -l ./target/release/examples/mem_probe diagnostics 1000000  # 127,336,448 bytes RSS
/usr/bin/time -l ./target/release/examples/mem_probe fields 0             # baseline: 1,409,024 bytes RSS
/usr/bin/time -l ./target/release/examples/mem_probe fields 1000000       # 82,116,608 bytes RSS
```

Per-item cost: `(127,336,448 - 1,343,488) / 1,000,000 ≈ 126.0 bytes/diagnostic`;
`(82,116,608 - 1,409,024) / 1,000,000 ≈ 80.7 bytes/distinct field value`. Chose:

- `DEFAULT_DIAGNOSTIC_CAP = 5,000` → ~615KB retained-diagnostics memory, bounded
  well inside the WASM-heap constraint TDR §14 flags, while still keeping far more
  retained examples than a human would ever scroll through for one broken file.
- `DEFAULT_FIELD_VALUE_CAP = 10,000` → ~788KB per field. Set higher than the
  diagnostic cap deliberately: a legitimately-structured field (e.g. `status_code`
  or `service`, dozens to low-hundreds of values) should never brush the cap, while
  a `request_id`-shaped field (task 6.5's 50,000-distinct-value test) still gets
  flagged and capped well before "unbounded."

Both are per-field/global constants in `crates/looq-core/src/parser.rs`
(`Parser::with_caps` exists for a future caller — e.g. a smaller cap under
memory-constrained live tail — to override them).

**Decisions made without a human available to ask, recorded per the task's own
instructions:**

- **NEEDS HUMAN DECISION — named IANA timezones are not supported.**
  `field-extraction/spec.md`'s "Caller-supplied timezone" scenario names
  `Europe/Belgrade`, which requires a timezone database (`chrono-tz`) to resolve
  DST rules. `chrono-tz`'s embedded data was not measured directly, but given that
  `regex` alone (a much smaller crate) added ~870KB to `core.wasm`, a full IANA
  database landing in the same crate would almost certainly blow the ~300KB budget
  by a much larger margin, in the same crate this change is explicitly required to
  protect (task 6.4, design.md D9 risk). Implemented `TimeZonePolicy` as UTC-or-a-
  caller-supplied-`FixedOffset` instead — covers "naive value written in a known
  fixed local offset" (the field-extraction spec's underlying intent, per D5) but
  not DST-aware named zones. Test written against a fixed offset
  (`caller_supplied_fixed_offset` in `timestamp.rs`), not literally
  `Europe/Belgrade`. A human should confirm this scope cut before named-zone
  support is either added properly (accepting the wasm cost, or gating it behind a
  lazily-fetched/optional data source) or formally dropped from the spec.
- **Message field-name precedence** (`message`, then `msg`) is not stated
  explicitly anywhere in the specs — inferred from the log-parsing spec's own JSON
  example (`"msg":"boom"`) and the logfmt "bare tokens fold into message" scenario.
  Applied consistently to both JSON and logfmt. Documented inline in
  `crates/looq-core/src/parsers/mod.rs`.
- **Epoch magnitude thresholds** (10 / 13 / 16-digit boundaries distinguishing
  seconds/ms/µs) are a judgement call, same spirit as the detection threshold —
  documented as such in `timestamp.rs`, not claimed as derived from anything.
- **logfmt/JSON "message stays empty when nothing recognisable is present"**
  (rather than falling back to the raw line) — avoids duplicating every field's
  text back into `message` for a fully-structured line with no explicit
  message/msg key; not tested by any spec scenario, a design call.

## 2026-08-09 — `browser-app-shell`: worker+comlink bridge, typed interop, real UI shell

Covers mvp-plan days 9, 10 and 14, implemented as one OpenSpec change per the
change's own scope (the seam between the real parser and a browser page with no
structure yet).

**What shipped:**
- `web/`: a real Vite + TypeScript-strict project (`vite` 8.2.1, `typescript`
  7.0.2, `comlink` 4.4.2) replacing the hand-written skeleton page. Two `tsconfig`s
  (`tsconfig.json` for main-thread/DOM code, `tsconfig.worker.json` for the
  `WebWorker`-lib worker code) because `DOM` and `WebWorker` libs declare
  conflicting globals and can't share one program.
- `crates/looq-wasm/src/dto.rs`: hand-written DTOs (`EntryDto`,
  `DetectionResultDto`, `DiagnosticsSummaryDto`, `FieldInventoryDto`, ...) mirroring
  `looq-core`'s types field-for-field, with `#[serde(rename_all = "camelCase")]` so
  the JS side gets plain objects, not `Map`s
  (`serde_wasm_bindgen::Serializer::serialize_maps_as_objects(true)`).
  `crates/looq-wasm/src/lib.rs`'s `ParserHandle` wraps `looq_core::Parser`
  one-to-one (`new`/`feed`/`finish`/`detection`/`format`/`diagnosticsSummary`/
  `fieldInventory`); this retires both the `bootstrap-cli-and-wasm-skeleton` stub
  (`parse_json_lines_count`) and the `log-parsing-core` benchmarking export
  (`parse_auto_detect_count`) — neither is called by anything anymore.
- `web/src/worker.ts`: the WASM parser runs in a Web Worker, reached from the main
  thread through `comlink`. `web/src/bridge.ts` (`ParserBridge`) owns the worker's
  lifecycle, reads the selected file via `Blob.slice` in chunks, and reports
  progress as a fraction of bytes consumed — the file is never read into one JS
  string first.
- Cancellation (D4): `ParserBridge.cancel()`/a new `parseFile()` call bumps a
  generation counter that every `await` boundary in the read loop checks, *and*
  the worker's `startSession()` frees the previous `ParserHandle` before
  constructing a new one — a stale `feed`/`finish` call for a superseded session
  id returns `null` because the instance it names no longer exists, not because a
  flag says stop.
- Worker/instantiation failures surface as a named UI error
  (`WorkerInitError`), not an empty result — verified by blocking the
  `/wasm/core.wasm` fetch via Playwright network interception. Fixed a real bug
  found by that same test: the worker's `wasmReady` promise is created once at
  worker-module-evaluation time, so a worker that failed once failed forever;
  `ParserBridge.parseFile()` now disposes the worker on any `WorkerInitError` so
  the *next* call builds a fresh one instead of the page staying permanently wedged
  after one transient failure.
- Web Components shell (`web/src/components/looq-app.ts` and four presentational
  children: `looq-drop-target`, `looq-detection`, `looq-diagnostics`,
  `looq-entry-table`) replacing the skeleton's inline script. State (current file,
  parse result, detection, diagnostics, status) lives entirely in `looq-app`
  (design.md D7); children only dispatch a `file-selected` event upward.
  `looq-entry-table` is explicitly commented as provisional (D6), capped at 500
  rendered rows so a 50MB file's DOM doesn't itself become the bottleneck.
- Distinct empty/unopened vs. zero-entries states; fallback/low-confidence
  detection gets a visually distinct (`.warning`) treatment; diagnostics show
  per-reason counts (exact, uncapped) plus retained examples (line, reason,
  detail) in a `<details>` disclosure — never console-only.
- `scripts/build-frontend.sh` now drives `wasm-pack build` (into
  `web/public/wasm/`, which Vite's `public/` passthrough copies byte-for-byte into
  `dist/wasm/`) followed by `vite build` (fixed, unhashed output filenames via
  `rollupOptions.output.*FileNames`), then copies `web/dist/` into
  `crates/looq/assets/`. `crates/looq/src/assets.rs`/`server.rs` updated to embed
  and serve the new layout (`/assets/index.js`, `/assets/index.css`,
  `/assets/worker.js`, `/wasm/core.js`, `/wasm/core.wasm`) — the old bare
  `/core.js`/`/core.wasm` routes are gone; `crates/looq/tests/cli.rs` updated to
  match.
- `.github/workflows/ci.yml`: two new jobs — `frontend-typecheck-and-build`
  (`npm ci && npm run typecheck && npm run build`) and
  `frontend-type-check-catches-rename` (runs
  `web/scripts/verify-rename-check.sh`); `frontend-artifact-staleness` now
  installs Node so `build-frontend.sh`'s `npm ci`/`vite build` step can run.

**Tests:** 81 Rust tests, all still passing (`cargo test --workspace`: 8 + 16 in
`looq`, 35 + 22 in `looq-core`, 0 in `looq-wasm` — no unit tests added there, the
DTOs are exercised end-to-end via the browser checks below and would need
`wasm-bindgen-test` for a native run, not set up in this change). `cargo fmt --all
-- --check` and `cargo clippy --workspace --all-targets -- -D warnings` clean.
TypeScript: `npm run typecheck` (`tsc --noEmit` against both `tsconfig.json` and
`tsconfig.worker.json`) clean.

**Deliberate rename-mismatch test (task 2.2)** —
`web/scripts/verify-rename-check.sh`: renames `EntryDto.ordinal` to `lineOrdinal`
in `wasm-types.ts` only (leaving `looq-entry-table.ts`'s `entry.ordinal` read
untouched, simulating "the interface got updated but a usage site was missed"),
runs `npm run typecheck`, asserts it fails, then restores the file from a backup
unconditionally via a `trap`. Confirmed failing as expected:
```
src/components/looq-entry-table.ts(34,23): error TS2339: Property 'ordinal' does not exist on type 'EntryDto'.
```
Honesty note recorded in the script itself: since `wasm-types.ts` is hand-written,
not generated from `dto.rs`, `tsc` cannot detect a Rust-side rename directly — it
has no view of Rust source. What it *does* catch, and what design.md D2 actually
describes, is the realistic failure mode: a field renamed in Rust and in this TS
mirror (by the same person, same change) but missed at one of several usage sites
— without the check that stays `undefined` at runtime; with it, `tsc --noEmit`
fails immediately, naming the exact line.

**Chunk size (task 2.5) — measured, not guessed.** Same real UI path (file picker
→ `ParserBridge.parseFile` → worker → `ParserHandle`), `target/bench-1mb.jsonl`
(7987 lines / 1,000,114 bytes), steady-state (2nd+ file in the same page, worker
+ WASM already warm), `CHUNK_BYTES` edited and rebuilt for each value:

| chunk size | steady-state (ms) |
|---|---|
| 64 KiB  | 78.7–90.8 |
| 256 KiB | 86.5–94.0 |
| 1 MiB (whole file in 1 chunk) | 120.6–131.2 |
| 4 MiB (also 1 chunk) | 118.8–129.5 |

Counter to the initial guess ("bigger chunks = less per-call overhead"), 1 MiB and
4 MiB were consistently ~40ms *slower* than 64/256 KiB, not faster — both reduce
to a single `feed()` call for this file, so the difference isn't call-count
overhead; the likely cause is `serde-wasm-bindgen` serializing one ~8000-element
JS array per call vs. several smaller ones (not confirmed further — a profiling
pass is out of scope for this change). Chose **256 KiB** over 64 KiB: same
steady-state cost, fewer comlink round trips, and still ~200 progress updates
across a 50MB file (measured: 50MB fixture reached 70% progress by 2.15s with
max per-sample main-thread round-trip of 4ms throughout — see responsiveness
measurement below).

**Worker/WASM instantiation cost (task 2.3, eager-vs-lazy) — measured via the real
UI**, tiny (1-line) fixtures so parse time itself is negligible, isolating
worker-spin-up + WASM fetch/compile/instantiate:

```
first file after fresh page load (worker created lazily right here): 8.4ms
next 3 files, same session (worker + WASM already warm):              0.7ms, 0.5ms, 0.4ms
```

Resolves the design.md open question: **worker created lazily** (only on the
first `parseFile()` call — `ParserBridge.ensureWorker()`), but **WASM
instantiation is eager within the worker** (`worker.ts` calls `mod.default(...)`
at module-evaluation time, not deferred to the first `startSession()`). Net
effect measured above: ~8ms extra on the very first file a user opens, paid only
by someone who actually opens a file, never by someone who loads the page and
leaves.

**Worker transfer cost, isolated (task 5.2)** — same wasm binary, same 256 KiB
chunking, same `bench-1mb.jsonl`, called two ways in the same page: (a) directly
on the main thread (`ParserHandle` used straight from `page.evaluate`, no
Worker/postMessage/comlink at all) vs. (b) through the real worker+comlink
bridge:

| path | steady-state (ms) |
|---|---|
| main-thread `ParserHandle` direct call | 51.2–51.9 (4 runs) |
| worker + comlink (full bridge) | 70.9–77.9 (5 runs) |

Delta ≈ **20–25ms per ~1MB file (~30% of total)** — real and measurable, but not
dominating (parse+serialize on the wasm side is still the larger half, and total
time stays ~2.5x under the TDR §11 proportional budget below). Task 5.2's
"if it dominates, batch transfers" branch is **not triggered** — no batching
implemented.

**Re-measured ~1MB browser parse, worker boundary included (task 5.1) — supersedes
`log-parsing-core`'s task 6.3 figure, explicit three-way comparison, same fixture
(`target/bench-1mb.jsonl`, 7987 lines / 1,000,114 bytes / 0.954 MiB) and
methodology (real Chromium via Playwright MCP, file picked through the actual
`<input type=file>`) across all three:**

| change | code path | cold | steady-state |
|---|---|---|---|
| `bootstrap-cli-and-wasm-skeleton` (day 4) | main thread, JSON-only counter stub | 23.5 ms | 6.4–6.9 ms |
| `log-parsing-core` (task 6.3) | main thread, real parser, count only, no worker/typed interop | 107.4 ms | ~75 ms (74.9–79.8) |
| `browser-app-shell` (this change) | worker + comlink + typed `serde-wasm-bindgen` DTOs + chunked feeding | 96.0 ms | 70.9–77.9 ms |

**Verdict:** adding the worker boundary, full typed entry serialization, and
chunked feeding did **not** regress steady-state throughput relative to
`log-parsing-core`'s already-real-parser baseline — both land in the same ~75-78ms
band for this fixture, despite this change now shipping every entry's full typed
structure across a Worker boundary where the previous measurement shipped only a
`u32` count. The isolated transfer-cost measurement above explains why it doesn't
regress further: chunking into 256 KiB pieces (vs. one big all-at-once call)
apparently offsets the added serialization/structured-clone cost. Target (TDR
§11): <200ms/MB, ≈190.7ms proportionally for 0.954 MiB — **cold ~2.0x under,
steady-state ~2.5x under.** All runs' entry counts matched the fixture's 7987
lines exactly across every chunk-size and cold/warm variant tested.

**Bundle size vs. TDR §5 budget (task 1.4):**

```
cd web/dist && gzip -9 -c assets/index.js | wc -c   # + index.css, worker.js — see below
```

| artifact | raw | gzip |
|---|---|---|
| TS bundle (`index.js` + `index.css` + `worker.js`, incl. `comlink`) | 20,338 B | **7,949 B (7.76 KiB)** |
| `core.wasm` | 198,648 B (194.0 KiB) | 83,274 B |
| `wasm/core.js` (wasm-bindgen glue) | 13,845 B | 3,624 B |

TS bundle: **7.76 KiB gzipped vs. the <200 KiB budget — ~4% used, no pressure.**
`comlink` (4.4.2) is genuinely small as design.md predicted. `core.wasm` grew from
`log-parsing-core`'s 154.3 KiB to 194.0 KiB (the `serde`+`serde-wasm-bindgen`
dependency and the DTO/serialization code), still comfortably under the ~300 KiB
budget with ~106 KiB of headroom. `uPlot` is out of scope for this change
(proposal.md Non-Goals) so it isn't in these numbers yet — `timeline-and-table`
will need its own measurement when it lands.

**ADR-0008 byte-reproducibility (task 1.3) — re-verified at every configuration
change made in this session, not just once:** two consecutive
`scripts/build-frontend.sh` runs with no source change produce byte-identical
`crates/looq/assets/` (`diff -rq`, silent/no output = identical) every time,
including the final configuration. Two consecutive `wasm-pack build` runs alone
are also byte-identical (`diff -q` on `core.js`/`core_bg.wasm`). The CI staleness
check (`frontend-artifact-staleness`) needed one change: it didn't previously
install Node, because there was nothing to build with it — `build-frontend.sh` now
runs `npm ci && vite build` as its second stage, so the job gained an
`actions/setup-node` step. No normalized-hash fallback needed — raw byte
comparison holds.

**End-to-end proof (task group 4), real Chromium via Playwright MCP against the
release binary (`looq --stdin --port 7999`, file picked through the actual page):**
- JSON/logfmt/plain fixtures (`crates/looq-core/tests/fixtures/format-*`, 10 lines
  each) each open with 0 skipped lines, correct format detected (JSON 100%,
  logfmt 100%, plain correctly falls back at 0% match), and entries + skipped =
  total lines in every case.
- `malformed.jsonl` (6 lines, 2 malformed — 33% bad) correctly falls back to
  plain text under auto-detect (67% JSON match < 80% threshold) and all 6 lines
  become entries — this is *correct* auto-detect behavior, not a bug, and is
  distinct from testing the diagnostics-on-screen requirement.
- A purpose-built 22-line fixture (20 valid JSON + 2 malformed, 91% JSON match —
  crosses the threshold) exercises the actual diagnostics-surface requirement:
  20 entries + 2 diagnostics (`invalid_json`, `non_object_json`) = 22 = total
  lines; both shown on screen with line number, reason and detail (`line 6:
  invalid JSON — expected ident at line 1 column 2`, `line 16: JSON value is not
  an object`).
- Console: 0 errors, 0 warnings across every scenario in this session (a stray
  `wasm-bindgen` "deprecated parameters" warning was hit and fixed mid-session —
  `mod.default("/wasm/core.wasm")` as a bare string triggers it; passing
  `{ module_or_path: "/wasm/core.wasm" }` does not).
- Network panel: exactly 6 requests total for the whole session (`/`,
  `assets/index.js`, `assets/index.css`, `assets/worker.js`, `wasm/core.js`,
  `wasm/core.wasm`) — unchanged after opening 5+ different files. US-6 holds
  through the worker/bridge refactor.
- 50MB fixture (`target/bench-50mb.jsonl`, 399,350 lines, built by concatenating
  `bench-1mb.jsonl` 50x): page stayed responsive throughout — main-thread
  `page.$eval` round-trips (a real proxy for "is the event loop free") stayed at
  1–4ms the entire parse, progress reached 70% by 2.15s. Cancellation: caught the
  parse genuinely mid-flight (1% progress, 2,100 entries), opened a small file
  instead, final displayed result was exclusively the small file's 10 entries;
  waited 4s past when the 50MB parse would naturally finish — result unchanged,
  no stale entries ever appeared (wasm-bridge spec, "Opening a second file
  cancels the first parse", task 2.6).
- Fresh-instance/no-shared-inventory (task 2.7): two `ParserHandle` instances
  constructed back-to-back (mirroring `worker.ts`'s `startSession`), fed the JSON
  then the logfmt fixture — both detected correctly (100%/100%), and the
  logfmt instance's field inventory (`duration_ms, job_id, key, port, service,
  status, upstream`) does **not** contain `path`, a field that exists only in the
  JSON fixture's inventory — confirms no state leaks between instances.
- Distinct empty/zero-entries states (task 3.7): before any file, results area
  is `hidden`, hint reads "Open a log file below."; an empty file produces
  `hidden = false` with "This file contained no log lines." — visibly distinct.
- Worker/instantiation failure (task 2.8): blocked `/wasm/core.wasm` via
  Playwright network interception → UI shows `status error` class with "failed to
  instantiate the WASM parser: TypeError: Failed to fetch", not an empty result;
  confirmed the app recovers on the next file open once the block is lifted (see
  the bug-fix note above).

**Design open questions resolved (task 5.3):**
- **Streaming vs. end-of-parse rendering:** end-of-parse. `ParserBridge.parseFile`
  collects entries across all chunks and resolves once; `looq-app` only calls
  `tableEl.setEntries()` after the whole parse settles. This matches design.md's
  own Non-Goals framing ("entries become available incrementally [to the bridge],
  but the provisional table may render once at the end") rather than introducing
  a new decision — real incremental *rendering* is `timeline-and-table`'s job
  once a virtual-scrolled table exists to render into.
- **Generated vs. hand-written TS types:** hand-written (`wasm-types.ts`), not
  `tsify`-generated. Reasoning: `tsify` would add a second wasm-bindgen-adjacent
  macro dependency and a second thing that has to keep working under
  `wasm-pack --target web --no-typescript`; ADR-0008's byte-reproducibility
  constraint already makes the build pipeline something to protect, not extend.
  The rename-check script demonstrates the hand-written approach's real,
  if narrower-than-generation, safety net.
- **Eager vs. lazy worker:** resolved above (worker lazy, WASM instantiation
  eager within it) — see the instantiation-cost measurement.

**Decisions made without a human available to ask, recorded per the task's own
instructions:**
- **Wasm-bindgen glue naming:** `core.js`'s default WASM-path lookup expects a
  file named `core_bg.wasm` next to itself; this repo names the served file
  `core.wasm` (matching `local-server` spec naming and the existing route from
  `bootstrap-cli-and-wasm-skeleton`). Passed the path explicitly via
  `mod.default({ module_or_path: "/wasm/core.wasm" })` rather than renaming the
  glue's internal expectation or the served route — smaller, more local change.
- **TypeScript can't type-check a runtime-only absolute-path dynamic import.**
  `declare module "/wasm/core.js"` fails (`TS2436: Ambient module declaration
  cannot specify relative module name` — a leading `/` counts as relative).
  Worked around by typing the module shape in a normal exported interface
  (`core-wasm-types.ts`, no `declare module`) and casting the dynamic import's
  result to it — and separately, `tsc` attempts static resolution on a *literal*
  dynamic-import string even under an `as` cast (failing regardless of the cast),
  so the specifier is read from a `const` variable instead, which makes the
  import's type `Promise<any>` and lets the cast apply. Both quirks documented
  inline in `worker.ts`/`core-wasm-types.ts`.
- **Provisional table render cap (500 rows)**, not stated in any spec — added so
  the 50MB responsiveness test wasn't defeated by the DOM itself even though
  parsing runs off the main thread; `entriesEmitted` (the real count) is still
  shown in the status line regardless of how many rows are rendered, so task
  4.2's "displayed entries + skipped = total lines" invariant is satisfied by the
  reported count, not by counting rendered `<tr>` elements.
- **worker.ts has no unit tests (0 tests in `looq-wasm`)** — `wasm-bindgen-test`
  wiring (headless-browser or Node WASM test runner) wasn't set up in this
  change; correctness was instead verified via the real-browser Playwright
  checks above, which exercise the actual compiled `core.wasm` end to end. This
  is a real gap for regression protection (a change to `dto.rs` wouldn't be
  caught until someone runs the E2E checks by hand) — noted as **NEEDS HUMAN
  DECISION**: worth a follow-up (`wasm-bindgen-test` in CI) even though nothing
  in this change's spec required it.

## 2026-08-09 — `live-tail`: ring buffer, backpressure, and the live tail UI

Covers mvp-plan days 11, 12 and 21, implemented as one change per the change's own
scope: a gap event with no UI marker reports data loss to nobody, so backend and UI
land together (proposal.md).

**What shipped — backend (`crates/looq/src/`):**
- `stdin.rs`: `StdinBuffer`, a `VecDeque`-backed ring buffer sized by `--max-lines`
  (default 100,000), filled by the stdin reader task from process start regardless of
  connections (ADR-0004). Sequence numbers assigned in `push`, monotonic, starting
  at 1. Reused `tokio::sync::broadcast`'s own ring-buffer-with-`Lagged`-receivers
  behaviour as the per-client bounded outbound channel (capacity 1024) instead of
  hand-rolling one — `RecvError::Lagged(n)` already *is* "drop-oldest, tell the
  receiver exactly how many it missed", which is D2's gap-count requirement for
  free.
- `protocol.rs`: `ServerMessage` enum (`snapshot`/`line`/`gap`/`ended`), JSON via
  serde with `#[serde(tag = "type", rename_all = "snake_case", rename_all_fields =
  "camelCase")]` (D1). Raw line text never sent unlabelled; a line whose own text
  looks like an envelope is carried as an opaque string inside a real envelope's
  `text` field (task 1.3, tested).
- `server.rs`'s `handle_socket`: subscribes to the broadcast *before* reading the
  snapshot, so no line can be lost between the two (ordering proof in the code
  comment); snapshot lines with `seq <= last_seq` are deduped on the live side.
  `StdinBuffer.ended` (added mid-change, see bugs below) lets a client connecting
  after stdin already closed still receive `Ended` instead of waiting forever.
- `main.rs`: `--max-lines` now actually sizes the buffer (previously parsed and
  reported only); the served page gets a `__LOOQ_MODE__` JSON placeholder
  (`{"mode":"stdin","maxLines":N}` or `{"mode":"file"}`) alongside the existing
  `__LOOQ_HINT__` one, so the frontend knows which UI to mount without guessing from
  `/ws`'s existence.

**What shipped — frontend (`web/src/`):**
- `live-tail.ts`: `LiveTailSession` (WS lifecycle, reconnect with capped exponential
  backoff 500ms→10s, sequence-number gap detection independent of `gap` messages,
  client-side eviction at `maxLines`) and `StreamParserSession` (one long-lived
  `ParserHandle` per stream, D7 — never discarded and reconstructed like
  `ParserBridge`'s per-file model). `parser-worker-client.ts` extracted the
  worker-boot/crash-race logic out of `bridge.ts` so both share it instead of
  duplicating the `WorkerInitError` race handling.
- `components/looq-live-tail.ts`: connection-state badge (connecting/live/ended/
  disconnected), throttled lines/sec counter (own 1s timer, decoupled from
  rendering per task 4.2), 80ms batched row rendering, autoscroll that pauses on
  scroll-away with a pending count and an explicit resume, gap markers, an
  eviction note, and the stdin-mode privacy indicator (D8, worded distinctly from
  file mode's per TDR §12).
- `looq-diagnostics.ts` gained an optional `cumulative` flag (D7/task 3.5): stream
  mode's one long-lived parser instance means diagnostics/field-inventory counts
  describe everything ever seen, not what survived eviction — said explicitly
  rather than left to be misread.
- `looq-app.ts` reads the mode placeholder and mounts either the existing file-mode
  UI or `<looq-live-tail>` — stdin mode never offers a file-open control, which is
  this change's resolution of design.md's open question ("file-open during an
  active stream"): the conflict can't arise because stdin mode's shell doesn't
  expose that UI at all (task 6.1).

**Three real bugs found by verification, not by inspection — all fixed:**

1. **Mode detection silently no-op'd.** `readRunMode()` initially read
   `#mode-template`'s `.textContent`, which is always `""` for a `<template>`
   element (its children live in `.content`, a separate inert `DocumentFragment` —
   `.innerHTML` is the one that reflects it, same reason `#hint-template` elsewhere
   in this file already used `.innerHTML`, which should have been the tell). Every
   page silently rendered file-mode UI regardless of the actual run mode. Caught by
   the very first real-browser Playwright screenshot of stdin mode.
2. **Detection could permanently misdetect on a race.** The render timer's
   periodic `refreshSummary()` forces `looq_core::Parser::finish()` early (D5's
   timeout escape hatch). If that tick fired before the very first line had been
   fed — plausible on a fresh connection with an empty buffer, since the WS
   round-trip through comlink isn't instant — `finish()` finalized detection on an
   empty sample, and `finalize_detection` can never re-run once a `Parser` is
   `Active`: the entire stream then misparsed every real line as plain text,
   permanently. Two fix attempts: the first added a deadline that still fired
   unconditionally once elapsed, which just needed my own tool round-trip latency
   to exceed 800ms to reproduce the exact same bug. The real fix (`live-tail.ts`,
   `StreamParserSession`): never force detection while zero lines have ever been
   fed, regardless of elapsed time — "no data yet" and "never getting data" are
   indistinguishable from a timer, and guessing wrong is unrecoverable. Caught by
   deliberately reproducing the race (write lines immediately after navigate, and
   again after a multi-second idle period) against the real running binary, not by
   reasoning about the code.
3. **A client connecting after stdin already closed waited forever.** `Ended` was
   a one-shot broadcast with no history, unlike lines (which live in the ring
   buffer). A client connecting after EOF got the snapshot's history correctly but
   then sat on "live" forever — realistic for `myapp | looq` when `myapp` is a
   short batch job that finishes before the browser tab opens. Fixed by adding
   `StdinBuffer.ended`, set under the same lock as the buffer contents, read
   alongside the snapshot so `server.rs` can send `Ended` immediately after it
   instead of only relying on the broadcast. Caught by literally trying the PRD
   Flow 2 variant where the producer finishes before the browser connects — not
   in any originally-planned test, added afterward
   (`connecting_after_stdin_already_closed_still_gets_ended`).

A fourth, narrower gap was caught by code review rather than a failing test and
fixed the same way: a full **backend process restart** (not a network-level
reconnect) resets sequence numbers to 1, and the reconnect dedup logic
(`seq <= lastSeqSeen` skips a line as "already seen") would have silently treated
an entire new snapshot as already-delivered duplicates, since a lower `last_seq`
than previously observed is otherwise impossible. `handleSnapshot` now detects
`lastSeq < this.lastSeqSeen` as a restart and reprocesses the snapshot as a fresh
first-connect instead of deduping it away — verified against two real, separately
spawned server processes on the same port. design.md's reconnect scenarios all
assume the same running process; this fix extends coverage past what was written
down, not because the spec technically required it, but because the alternative
was silent data loss, which is this project's one hard line.

**Measurements (real numbers, exact commands):**

- **Snapshot at the default `--max-lines` (100,000)** —
  `cargo test -p looq --test cli snapshot_at_default_max_lines_is_delivered_promptly
  --release -- --nocapture`, 100,000 representative JSON lines (~110 bytes each,
  `{"ts":...,"level":"INFO","service":"api","msg":"handled request N","status":200}`):
  **~12.7MB JSON payload, delivered in ~81ms** (release build) / ~752ms (debug
  build). Chunking (D3's fallback) was not implemented — the number didn't justify
  the complexity at representative line lengths; revisit if real-world lines are
  routinely multi-KB.
- **Synthetic fast-producer/slow-consumer harness (task 2.5)** —
  `cargo test -p looq --test cli fast_producer_slow_consumer_never_blocks_and_reports_an_accurate_gap
  -- --nocapture`: 20,000 lines written to stdin while a connected client never
  reads; **write completed in 23ms** (producer never blocked); the slow client,
  once drained, saw **2,525 lines delivered + 17,475 lines reported via `gap`
  events = exactly 20,000**, no double-counting, no under-reporting.
- **Memory bound (task 2.7)** —
  `cargo test -p looq --test cli process_memory_stays_bounded_over_ten_times_max_lines
  -- --nocapture`, `--max-lines 2000`, real process RSS via `ps -o rss=`:
  **9,776KB after 1x max-lines written, 10,208KB after 10x — 432KB growth**, not the
  ~9x growth an unbounded buffer would show.
- **End-to-end live-tail latency vs. TDR §11's <100ms target (task 5.5)** —
  method: a Playwright-driven real Chrome tab against a real `looq --stdin`
  subprocess (fed via `tail -f` on a growing file so timing is controlled from
  outside the process, PRD Flow 2's actual shape); marker lines carry an embedded
  `Date.now()`-equivalent epoch-ms timestamp from the writer, a `MutationObserver`
  in the page records `Date.now()` the first time each marker's row appears in the
  DOM — both timestamps share the same OS clock, so no cross-process RPC round trip
  pollutes the measurement. Two independent 10-marker runs: **15–74ms** (avg
  42.1ms) and **12–65ms** (avg 38.5ms). Well inside the 100ms target. This
  measurement is only meaningful because `RENDER_INTERVAL_MS` (the UI's batched-
  render tick, D6) was deliberately set to 80ms, not the first-guess 400ms — at
  400ms the batching interval alone would have eaten most of the budget before
  network/parse time even entered into it.
- **Bundle size**: `core.wasm` unchanged at 194KB (this change didn't touch
  `looq-core`/`looq-wasm`); TS bundle now 23.3KB / 8.05KB gzipped (`index.js`) +
  4.8KB (`worker.js`) + 2.3KB / 0.88KB gzipped (`index.css`) — up from 8KB gzipped
  total pre-change, still well inside TDR §5's <200KB gzipped budget.

**Tests:** 95 Rust tests across the workspace (17 + 21 in `looq`, 35 + 22 in
`looq-core`, 0 in `looq-wasm`), all passing. `cargo fmt --all -- --check` and
`cargo clippy --workspace --all-targets` clean. `npm run typecheck` clean. One
`cargo test --workspace` run flaked on
`eof_sends_ended_message_then_closes_the_socket_and_server_stays_alive` (passed in
isolation and on a subsequent full re-run) — consistent with OS-level resource
contention from this sandbox spawning many real child `looq` processes under
`cargo test`'s default parallelism, not a code regression; not investigated further
since it didn't reproduce.

Group 5 end-to-end, all against a real compiled release binary + real Chrome via
Playwright, not simulated: PRD Flow 2 (`myapp | looq --open` shows LIVE, the
counter, streaming entries with correct format detection); pipe-then-connect-later
(5 lines written before any browser connected, all present in the snapshot);
reload mid-stream (history + continuity across a real page reload, detection
stayed correct); backend killed (indicator shows "Disconnected — retrying…", not a
frozen "live" view, entries already received stay visible); autoscroll
pause-with-pending-count-then-resume (scrolled away, wrote 5 lines, saw "Resume
following — 5 new", clicked it, jumped to bottom); client-side eviction at
`--max-lines` (wrote 25 lines at `--max-lines 10`, exactly 10 rows retained, note
read "15 earlier entries have been evicted"); reconnect merge without duplicates
(closed the live WebSocket directly while the same backend process kept running,
wrote 3 more lines during the drop, reconnected automatically, exactly 8 rows, no
duplicates, no gaps since nothing was actually lost).

**Decisions made without a human available to ask, recorded per the task's own
instructions:**
- **`--max-lines` is shared, not client-chosen** (design.md open question):
  the server sends its configured value via `__LOOQ_MODE__`'s `maxLines` field and
  the client evicts at that same number. A weak client choosing its own smaller
  limit was the alternative; rejected because a shared number is simpler to reason
  about and matches the backend's own bound — a client wanting to protect a weak
  machine further can still just not scroll back through everything.
- **Snapshot compression not implemented** (design.md open question): measured
  first (~81ms/~12.7MB at the default `--max-lines` in release), and the number
  didn't justify the complexity. Recorded as a revisit trigger, not a permanent
  no.
- **Render batching interval (80ms, not the initially-planned 400ms)**: caught
  during the latency-measurement task itself — 400ms alone would have eaten most
  of the <100ms end-to-end budget before any real work was measured. Not stated in
  any spec; the number was picked to keep the batching win (coalescing fast
  producers) without becoming the dominant term in what a user perceives as
  latency.
- **`StreamParserSession.SETTLE_MS = 300`**: once at least one line has been fed,
  wait 300ms for a few more before forcing detection, rather than detecting off a
  single line immediately. A judgement call between "detect as fast as possible"
  and "detect off a slightly bigger, more reliable sample" — not measured, chosen
  as a reasonable middle ground consistent with D5's "short timeout" wording.

**NEEDS HUMAN DECISION:**
- The backend-process-restart dedup fix (bug 4 above) is verified working but its
  UI is silent — the new snapshot's lines just appear next to the old process's
  lines with no visual marker that a restart happened, unlike a genuine gap. A
  dedicated "stream restarted" row kind would make this fully consistent with the
  project's "never silent" rule; not added in this change because it's a new `Row`
  variant, a new render case and new CSS for an edge case outside every scenario
  design.md actually describes. Worth a follow-up if backend restarts under an
  open tab turn out to be a real workflow (e.g. a supervisor auto-restarting a
  crashed producer).
- Format override (`#format=`) is not wired into either mode's UI yet (only into
  the worker API) — stream mode is at parity with file mode's current gap, not a
  new regression, but both are missing the URL-hash-driven override PRD/TDR
  describe. Deferred to whichever change adds URL hash state (`docs/mvp-plan.md`
  day 22), same as file mode.

## 2026-08-12 — `timeline-and-table`: time-ordered index, `uPlot` histogram, virtual table

Picked up mid-flight: a previous session had already written the real implementation
(`web/src/entry-index.ts`, `web/src/timeline-bucket.ts`, `web/src/components/looq-timeline.ts`,
`web/src/components/looq-entry-table.ts`, shell wiring in `looq-app.ts`/`looq-live-tail.ts`) and
left five debugging screenshots in the repo root, but its `tasks.md` had zero checkboxes checked
and its session was gone. Verified rather than trusted: read every changed file, ran
`cargo build --workspace`, `npm run typecheck`, `npm test` (both clean before touching anything),
then drove the real compiled release binary with Playwright to independently confirm every
scenario in the three new specs. The code was correct and complete; this session's job was mostly
verification, plus the measurements `tasks.md` explicitly requires and hadn't been recorded
anywhere.

**What shipped:**
- `entry-index.ts`: entries in input-order in a `Map` keyed by parser ordinal, plus a `sorted`
  array of `{ordinal, tsMs}` refs for timestamped entries only (D2). Insertion has a near-sorted
  fast path (push) and a binary-searched `splice` fallback. Front eviction is tombstone-based
  (O(1) per evict) with deferred batch compaction, not an O(n) filter per call (D1's "flat
  per-entry cost" requirement). `robustSpan()` uses a Tukey IQR fence (1.5x IQR beyond
  Q1/Q3) rather than min/max for the default view (D4); `fullSpan()` gives the true extremes for
  "zoom out". Timestampless entries live in their own `Set`, never enter `sorted`, never get a
  substitute time (D5).
- `timeline-bucket.ts`: a fixed ladder (1s…365d) picks the smallest width keeping bucket count
  under a target of 120 (D3, task 4.3 resolved — see below).
- `looq-timeline.ts`: owns one `uPlot` instance, redrives span/width/counts from the index on
  every `render()`/`refresh()`, dispatches `range-change` on drag (uPlot's `hooks.setSelect`) and
  never holds the active range itself past reflecting `setActiveRange()` (D7).
- `looq-entry-table.ts`: fixed 24px row height, viewport + overscan windowing (D6), replaces
  `browser-app-shell`'s 500-row-cap provisional dump (task 3.8, confirmed gone — no
  `provisional`-tagged renderer left in `web/src/`, only comments referencing its removal).
  `refresh()` anchors the visible window to the row's `ordinal` (not raw `scrollTop`), so growth
  at the tail doesn't move a paused user and front-eviction jumps cleanly to the nearest survivor
  with a banner instead of silently showing the wrong rows.
- Shell integration (D7): `looq-app.ts` (file mode) and `looq-live-tail.ts` (stream mode, its own
  shell) each own `activeRange` and are the only place it's written; timeline and table are only
  ever told about it through `setActiveRange()`, never wired to each other directly.

**Task 4.3 (open questions), resolved and kept as found:**
- **Default table order: input order.** Cheap (no sort), honest about the file, and matches D2's
  ordinal-as-input-order identity used for selection/scroll-anchoring. Timestamp order was the
  alternative; rejected because it would need a second ordering structure the anchor logic doesn't
  need otherwise, for a benefit (chronological reading) the timeline already provides visually.
- **Detail view: inline panel below the table**, not a modal or per-row expansion. A modal would
  need its own dismiss/backdrop/focus-trap machinery in a change that explicitly excludes visual
  polish (design.md non-goals); inline expansion would break the fixed-row-height invariant D6
  depends on for virtual-scroll arithmetic. A panel below the rows keeps both.
- **Target bucket count: fixed at 120**, not derived from the timeline's pixel width. The timeline
  renders at a fixed, unstyled width in this change (`release-hardening` owns responsive layout),
  and 120 is what `uPlot` needs before its first real layout pass regardless.

**Measurements (real numbers, exact method):**
- **50k-entry scroll smoothness (task 5.1)** — Playwright against the real release binary
  (`cargo build --release`, `./target/release/looq target/perf-fixtures/fixture-50k.jsonl`,
  fixture from `scripts/gen-timeline-fixture.py 50000 --jitter 5`): a `requestAnimationFrame`
  recorder wrapped a 2-second programmatic `scrollTop` sweep from 0 to max
  (`viewport.scrollHeight - viewport.clientHeight` = 1,199,520px). **123 frames, avg 16.80ms/frame
  (~59.5fps), max 32.86ms, 0 frames over the 33.3ms (30fps) threshold.** DOM row count stayed at
  36 rows for all 50,000 entries (viewport-bounded, task 3.1). Jump-to-end (`scrollTop = 0` then
  `= max` in one synchronous pair) took 0.10ms; the correct last three ordinals (49999/50000/50001)
  rendered immediately, no stall.
- **10k range-filter latency vs. TDR §11's <50ms target (task 5.2)** — same binary,
  `fixture-10k.jsonl` (10,000 entries), 20 samples dispatching `range-change` on the timeline
  element (the exact event path a real drag uses), timed synchronously around the call that
  reaches through the shell into both `setActiveRange()` calls: **avg 1.44ms, max 2.9ms** — about
  17x under target.
- **Drag responsiveness (task 5.3)** — real (not synthetic-dispatch) mouse events via
  Playwright, 40 mousemove steps over ~250px on the `uPlot` `.u-over` element on the 50k dataset,
  frame times recorded the same way as the scroll test: **27 frames, avg 17.31ms, max 34.01ms, 1
  frame over the 33.3ms threshold (3.7%).** Drag-select's actual commit (task 2.5) could not be
  reproduced through this sandbox's composite drag tool — uPlot's cursor-drag hook never fired
  from the synthesized event sequence regardless of marker placement (tried: raw `dispatchEvent`,
  `pointer-events:none` overlay markers, and markers as real DOM children of `.u-over` driven by
  the drag tool) — a tooling/timing gap in headless automation, not reproducible as a product bug:
  `uPlot`'s own drag threshold (`cursor.drag.dist`) defaults to 0, and the full pipeline was
  independently confirmed by calling `uPlot`'s own public `setSelect()` API, which invokes the
  identical `hooks.setSelect` array a real drag's internal mouseup handler does. Result: 50,000 →
  20,000 shown on selecting a sub-range, "Clear range" restored to 50,000. Also confirmed
  separately via the prior session's `fixture-json.png` screenshot (real interactive drag,
  10-line fixture): visible selection, narrowed table, working clear.
- **Index maintenance cost vs. rendering cost (task 1.6, design.md D1's revisit trigger)** — a
  throwaway `vitest` script (`EntryIndex.append` in 50-entry batches with light jitter, the
  live-tail-realistic case), run once and then deleted, not kept as a regression gate:
  **append cost per entry: 1.39us @1k, 1.07us @10k, 0.57us @50k, 0.64us @100k entries** — flat,
  does not grow with dataset size (confirms the O(1)-amortized claim in the code's own comments).
  `bucketCounts()` (a full re-bucket, what the timeline's throttled redraw costs): **0.012ms @1k,
  0.043ms @10k, 0.210ms @50k entries.** Against the ~16-17ms/frame rendering cost measured above,
  index maintenance is 3-4 orders of magnitude cheaper than rendering at every size tested — the
  revisit condition (D1: "if index maintenance dominates over rendering, move it to
  `looq-core`") is not triggered. Index stays in TypeScript.
- **Bundle size vs. TDR §5's <200KB gzipped budget** — `npm run build`: `index.js` 87.72KB /
  **34.01KB gzipped** (was 23.3KB / 8.05KB before `uPlot`), `index.css` 6.08KB / 1.89KB gzipped,
  `worker.js` unchanged at 4.86KB (no gzip measured, same as prior changes' reporting). Main-bundle
  gzip total (`index.js` + `index.css`): **35.90KB, up ~26.97KB from `uPlot`** — about 18% of the
  200KB budget used, 164KB of headroom left.

**Verified live and end-to-end (Playwright against the real binary, not simulated):**
- Three fixtures (JSON, logfmt, plain-text with mixed timestamped/timestampless entries) each
  render correctly in both the timeline and the table with zero console errors (task 4.2).
- Live-tail growth + eviction against a real `--stdin` process at `--max-lines 200` fed by a
  ~100 lines/sec producer: `LIVE` indicator, eviction note with an accurate count, and — the
  scenario design.md calls out explicitly — scrolling to a position that then got evicted out
  from under the user jumped cleanly to the nearest surviving row and raised the "no longer
  retained" banner (task 3.7), while scrolling to a position that stayed inside the retained
  window held its exact anchor ordinal across continued growth (task 3.6, confirmed at ordinal
  4550 unmoved after 300ms of continued arrivals).
- Row selection and detail view: clicking a row shows every extracted field, including a nested
  JSON object rendered as readable raw text in a `<pre>` (task 3.4, D8's "kept as text" policy).
- Absence markers: an entry with no timestamp renders "no timestamp" (not a blank cell) in both
  the table and its detail view; the timeline reports "N of M entries have no timestamp" whenever
  applicable.

**Tests:** 95 Rust tests (unchanged — no Rust code touched by this change), 14 TypeScript tests
(`entry-index.test.ts`, `timeline-bucket.test.ts`), all passing. `cargo fmt --all -- --check` and
`cargo clippy --workspace --all-targets` clean. `npm run typecheck` clean.

**Cleanup:** the five loose screenshots in the repo root (`50k-full.png`, `50k-loaded.png`,
`detail-view.png`, `fixture-json.png`, `live-tail-mid.png`) were debugging artifacts from the
prior session's manual verification, not referenced from any doc or spec (`grep` came up empty).
What each one was checking is captured above instead (50k load/scroll, detail view, JSON range
selection, live-tail eviction) — deleted after confirming the same scenarios independently.

No NEEDS HUMAN DECISION items — design.md's three open questions were explicitly assigned to
this change to resolve (task 4.3), not escalate, and are recorded above with reasoning.

## 2026-08-13 — `filtering-and-search`: one predicate, chips, regex search, URL hash

Built the layer that makes the viewer an actual filtering tool: `web/src/predicate.ts` (D1's
OR-within-field/AND-across-fields rule, D3's message+field-value search surface),
`web/src/search-query.ts` (`re:`/literal-escape compilation, `field=value` token extraction),
`web/src/url-hash.ts` (grammar + debounced `replaceState` writer), a new `EntryIndex` predicate
(`setPredicate`, D9: tested per entry on arrival, rescanned in full only on a filter change) with
incremental `levelStats` (design.md's own `predicate.ts` doc comment explains why `level` needs
its own tracking — the parser's field inventory never sees it), a new `<looq-filter-bar>`
component, and the shell wiring in both `looq-app.ts` (file mode) and `looq-live-tail.ts` (stream
mode) that computes one predicate and feeds table/timeline/counts from it.

**Two real bugs found only by driving the compiled release binary, not by unit tests:**

- **Filter-bar's own count could disagree with the table's.** `EntryIndex.matchingCount` answers
  "how many match the predicate", full stop — it doesn't know about the active time range, which
  lives in the shell. With a range selected, the filter bar showed `71 of 10000` while the table
  correctly showed `29 of 10000 ... in the selected range` for the *same* view — exactly the
  disagreement D2 exists to prevent, just moved one level up (table vs. timeline agreed; the
  filter bar's summary line didn't). Fixed by a shell-level `updateFilterCounts()` that reads
  `EntryIndex.countInRange(...)` instead of `matchingCount` whenever a range is active, called from
  both `applyPredicate()` and `setActiveRange()` so it can never go stale. Found running PRD Flow 3
  end to end (chips, then a range) against the real binary — no unit test exercises "two counts
  visible on screen for the same state," only an eyeballed page does.
- **`<mark>` itself, not the filtering logic, was most of one filter change's latency.** Isolating
  the 10k-entry latency measurement (below) down to individual calls found `EntryIndex.setPredicate`
  taking ~1.6ms and `LooqTimeline.refresh()` ~2ms — both trivial — while `LooqEntryTable`'s
  `setQuery`+`refreshFilters()` cost 30-50ms. Bisecting further: writing the exact same 36-row
  `innerHTML` content with `<mark>` wrapping matched text cost ~19ms average versus ~2.4ms for the
  identical text with no markup at all — an 8x cost from the semantic highlight element alone, in
  this environment. Switched `highlightHtml` (`looq-entry-table.ts`) to a plain `<span class="hl">`
  styled identically via CSS; the 10k regex-query sample average dropped from ~30ms to ~14ms and the
  worst sample from ~58ms to 37ms — see the latency numbers below. Whatever the exact browser
  internals responsible (candidate: `<mark>` hooking the UA's own find-in-page/selection styling
  machinery), a semantic tag choice was not worth an 8x cost difference for a purely visual effect.

**A third finding, not a bug but a real gap surfaced only by using the real field inventory:** a
10k-line fixture's `request_id` (one distinct value per line) never tripped the parser's
`highCardinality` flag — the field has *exactly* `DEFAULT_FIELD_VALUE_CAP` (10,000) distinct
values for 10,000 lines, so the cap is reached but never exceeded (`fields.rs`'s flag only sets on
the *next* value past the cap). The filter bar rendered 10,000 chip buttons for one field. The
parser's cap bounds inventory *memory*; it says nothing about what's usable as chip *UI*. Added a
UI-side `CHIP_LIST_MAX_VALUES = 50` in `looq-filter-bar.ts`, independent of the backend flag —
whichever cap trips first gets the typed-entry fallback. Re-verified end to end: typing
`req-000005` into the fallback input and clicking Add produced a working, removable chip narrowing
to the single matching entry.

**A third fix, found while root-causing the first two:** typing in the search box was re-running
the full predicate + table + timeline pipeline on every keystroke — compiling the query once per
change already happened (task 3.7), but *applying* it didn't wait for the user to stop typing.
Instrumented a real `pressSequentially` (per-character) type of a 14-character regex: before the
fix, that's 14 full re-renders; after adding a 150ms debounce on `emitChange()` in
`looq-filter-bar.ts` (error banner and `field=value` chip conversion stay instant — only the
expensive downstream apply is deferred, same debounce pattern D7 already uses for the hash
writer), the instrumented count dropped to exactly 1.

**Combination rule (D1), verified both directions against the real binary, not just unit tests**
(`crates` unaffected — this is TypeScript-only): on the 10k fixture, `level=ERROR` alone → 2500;
adding `level=WARN` → 1000 (**widened**, OR within the field); replacing it with `service=api`
→ 500 (**narrowed**, AND across fields) — matches the unit-level test in `predicate.test.ts`
exactly. `predicate.test.ts` additionally covers empty-value-set-means-no-constraint and a field
absent from an entry never matching a non-empty filter.

**Golden-path checkpoint (mvp-plan day 20, task group 7), against the real release binary,
`target/fixture-10k.jsonl` (10,000 JSON Lines entries, 4 levels × 2,500, 5 services × 2,000, jitter
timestamps, 20 timestampless entries), driven entirely through Playwright against
`http://127.0.0.1:7999`:**

- Flow 1 (open): file picker → `10000 entries, 0 blank lines, 10000 total lines in 142.9ms`.
- Flow 3 (filter): `[ERROR]` chip → 2500/10000; `[service=api]` chip → 500/10000 (AND); `[WARN]`
  chip → 1000/10000 (OR, confirms D1 both directions); drag-equivalent range dispatch combined with
  both chips + a query → table and filter-bar counts agree (29/10000, post-fix); URL hash
  (`#filter=level%3DERROR&filter=service%3Dapi&q=connection&range=...`) reproduced the identical
  view in a genuinely fresh page load (not a same-document hash navigation — `page.goto()` to a
  hash-only-different URL does *not* reload the document in a real browser, so the test had to
  navigate through `about:blank` first to get an honest "fresh tab").
- Flow 4 (search): substring search case-insensitive (`Cache MISS` → 72/500 while chips were
  active); highlighting confirmed present in the rendered row (`<span class="hl">cache miss</span>`
  after the `<mark>` fix); invalid regex (`re:[unclosed`) showed an inline error while the *previous*
  71-entry result stayed on screen — confirmed the table was not silently emptied; Escape cleared
  only the query, chips remained active; `service=db` typed into the search box became a chip and
  widened the service filter (OR) to 1000/10000.
- Malformed hash: `#...&bogus=1&range=not-a-range` on a fresh load applied the valid filter/query
  parts and surfaced `From the shared URL, could not be applied: unrecognised hash key(s): bogus;
  range "not-a-range" is not two comma-separated timestamps.` — nothing silently discarded.
- Copy-link: flushed the debounced hash write and copied `location.href`, showing "this link
  encodes your search text and filter values, which are fragments of the log itself" at the moment
  of copying (D8), not only in the README.
- Live data (a real `--stdin` process behind a FIFO-backed `tail -f`, so lines could be fed in
  controlled batches across separate steps rather than guessing at timing): 20 lines arrived, filter
  applied mid-stream (`level=ERROR` → 6/20, confirms task 6.3's "re-evaluates retained entries");
  20 more lines fed while the filter stayed active → 13/40 (`floor(40/3)`, confirms task 6.1 —
  entries evaluated against the active predicate on arrival, not just at the next full rescan) with
  the counter still reporting 40 received (task 6.2); timeline's dual series showed 39 (background,
  one entry outside the robust span) vs. 13 (foreground) for the same state.
- **Regex-on-arrival cost at stream rates (task 6.4)** — 2,000 single-entry `EntryIndex.append`
  calls with an active regex predicate over every field (the real per-arrival cost D9 describes,
  not a batch rescan): **4.5ms total, 2.25us/entry**, a theoretical ~444,000 lines/sec capacity —
  three to four orders of magnitude beyond any realistic producer rate this project targets
  (`live-tail`'s own test scenarios use ~100 lines/sec). No per-entry searched-text cap needed;
  design.md's "if needed" for that mitigation is answered no, measured rather than assumed.
- Fixed as part of this run, not deferred: the count-disagreement bug, the `<mark>` cost, the
  10,000-chip field, and the per-keystroke re-render — all four listed above, all four re-verified
  against the rebuilt binary afterward.

**Filter latency vs. TDR §11's <50ms target on 10,000 entries — measured post-fix, in the real
browser against the release binary, 20 samples per case, `requestAnimationFrame`-paced (not a tight
synchronous loop, which forces extra layout thrashing unrepresentative of one real interaction):**

- **Chip toggle (the common case — `applyPredicate()`, the same method a real click handler
  calls):** avg **8.44ms**, max **18.7ms** — under target by 2.7-6x.
- **Regex search (`re:cache.*miss`, matching 1429/10000, same `applyPredicate()` path):** avg
  **14.4ms**, max **37ms** — under target by 1.4-3.5x, and every one of 10 samples cleared 50ms
  (before the `<mark>`→`<span>` fix: avg ~30-37ms, max 56-58ms, with the max occasionally *missing*
  the target — this was the one real target-miss risk in this change, fixed, not downgraded).
- Isolated `EntryIndex.setPredicate` alone (the actual predicate-matching cost D9 calls "the
  expensive path"), with the real combined field-filter + regex-over-every-field-value predicate,
  on all 10,000 entries: avg **1.6ms**, max **4.5ms** — confirms the matching logic itself was never
  close to the target; the risk was entirely in rendering, and is now fixed.
- No target was downgraded; the one miss found was root-caused and fixed (task 7.4).

**Worst-case hash length (task 5.7)**, a realistic-not-pathological view — 7 fields × 8 UUID-shaped
selected values each (56 `filter=` entries), a ~90-character regex query, a range, format override
and tz — measured via a throwaway `vitest` script (`encodeHash`, deleted after running, not kept as
a regression gate, same convention as `timeline-and-table`'s index-cost measurement): **hash 3,779
chars, full URL 3,802 chars.** Exceeds the commonly-cited ~2,000-char cross-browser/legacy-proxy
safe threshold, but every browser this project targets (PRD §11: Chrome 110+, Firefox 110+, Safari
16+) supports URLs far longer (low tens of KB at minimum) — not a practical problem for this
project's stated browser support, but worth knowing if `#pattern=` or saved-view-style hash growth
is ever added later.

**Task 8.1 — design.md's three open questions, resolved (kept as found in design.md, per
`timeline-and-table`'s convention — resolution recorded here, not by editing an already-written
design doc):**

- **Chip negation (`level!=DEBUG`)?** Not added. This is exactly the query-language expansion
  design.md's own Non-Goals rule out for this change ("`field=value` and `re:` are the whole
  surface"); adding it means a hash grammar change (a new operator) and an unspecified UI affordance
  (right-click? a toggle?) with no design behind either yet. Left for a future change if usage shows
  it's wanted — excluding noise is real, but so is scope discipline.
- **Does search match the raw source line, or only parsed fields?** Parsed fields only (message +
  level + every field value) — no change. Matching the raw line would need `Entry`/the WASM DTO to
  carry the original line text, which they don't (`EntryDto` has no `raw` field) — that's a
  `log-parsing-core`/`wasm-bridge`-shaped change, not a filtering one, and design.md's own framing
  of the question already leans this way ("parsed matching is faster and more predictable"). Real
  gap this leaves: a JSON entry with no discoverable `message`/`msg` key has an empty `message`
  (log-parsing-core's own documented behavior, not new), so a raw-line-only fragment can't be found
  by search — narrow, and only affects JSON specifically, since logfmt's bare-token fallback and
  plain text's whole-line-as-message avoid it.
- **Should the hash encode the file name?** No. A hash-carried filename couldn't actually automate
  anything — ADR-0007's whole point is no browser API can auto-open a path a page merely names, so a
  recipient still has to pick a file by hand regardless of what the hash says. It would only ever be
  a cosmetic hint (duplicating the CLI-arg hint mechanism that already exists), and it would widen
  D8's already-stated leak surface — a path can carry a hostname, username or project codename — for
  a benefit that isn't real automation. Not worth it.

**Tests:** 95 Rust tests (unchanged, no Rust code touched by this change — confirmed via
`cargo test --workspace`), 55 TypeScript tests (`entry-index.test.ts` extended from 11 to 17 with 6
predicate/level-stats tests, `predicate.test.ts` 17, `search-query.test.ts` 10, `url-hash.test.ts` 8,
plus the pre-existing `timeline-bucket.test.ts`'s 3), all passing. `npm run typecheck` clean (both
`tsconfig.json` and `tsconfig.worker.json`), `cargo fmt --all -- --check` and
`cargo clippy --workspace --all-targets`
clean. `npm run build`: `index.js` 105.5KB / **38.42KB gzipped** (`index.css` 7.33KB / 2.18KB
gzipped) — up from `timeline-and-table`'s 35.90KB combined gzip by ~2.5KB for the filter bar, search
compilation and URL-hash modules combined; ~19% of TDR §5's 200KB budget used, 161KB headroom left.

No NEEDS HUMAN DECISION items — all three of design.md's open questions were explicitly assigned to
this change to resolve (task 8.1), and are recorded above with reasoning; the one measured
latency risk (regex search) was root-caused and fixed rather than downgraded.

## 2026-08-13 — `release-hardening`: security, error states, theme, size cap, freeze

Covers mvp-plan days 24–30, the seventh and last change of the MVP. Everything before
this made the happy path work; this is what happens on a bad day, plus feature freeze.

**Security (TDR §13) — closed:**

- `crates/looq/src/server.rs`: `Content-Security-Policy: default-src 'self';
  script-src 'self' 'wasm-unsafe-eval'; style-src 'self' 'unsafe-inline'` on every
  response via an `axum::middleware::from_fn` layer. Both additions beyond bare
  `default-src 'self'` were found necessary by actually loading the app under the
  bare policy first, not assumed: `'wasm-unsafe-eval'` for same-origin WASM
  compilation, and `'unsafe-inline'` on `style-src` because the virtual-scrolled
  table (`looq-entry-table.ts`) positions rows via `element.style.transform`/
  `.style.height` on every scroll — the bare policy produced 144 "Applying inline
  style violates ... default-src 'self'" console errors and a table that rendered
  zero rows, caught by opening a real file through Playwright, not by reading the
  code.
- `/ws` origin check (`ws_handler`/`origin_matches_host`): a WebSocket upgrade whose
  `Origin` header doesn't match the request's own `Host` header is refused with 403
  before `ws.on_upgrade` is even called. Deliberately compares against the request's
  own `Host`, not a value recomputed from `--host`/`--port`, so it stays correct when
  bound to `0.0.0.0` and reached via any interface address. Missing `Origin` is
  allowed through (a non-browser client, e.g. `wscat` or this crate's own integration
  tests, never sends one; a real browser always does, same-origin or not).
- Token handshake (`protocol.rs`'s `ClientMessage::Auth`, `server.rs`'s
  `authenticate`): a random 128-bit token (`rand::thread_rng().fill_bytes`, 32 hex
  chars) generated once per process in `main.rs`, embedded in the served page via a
  new `#token-template` (same convention as `#hint-template`/`#mode-template`), read
  by `web/src/token.ts`. The client's very first `/ws` message must be
  `{"type":"auth","token":"..."}`; the server subscribes to stdin / reads the
  snapshot only after that succeeds, closing the socket (code 4001) if 5 seconds pass
  without a valid token — verified the token never appears in the WebSocket URL/query
  string (D1's explicit concern), and never regenerated on reconnect within the same
  process, only on a fresh page load.
- Threat model documented in both READMEs (English and Russian, "Security" section):
  protects against another origin/tab, explicitly does **not** protect against
  another local process or a non-loopback `--host` bind — ADR-0003's warning already
  covers the latter and is unchanged.
- **Real cross-origin attempt against a running server**, not just a unit test:
  opened a second `looq` instance on a different port (different origin), and from
  that page's console tried `new WebSocket('ws://127.0.0.1:<other-port>/ws')`. The
  browser's own CSP (`connect-src` falling back to `default-src 'self'`) blocked the
  attempt before any network request went out — a real, if incidental, second layer.
  The origin-header-rejection path itself (what actually matters against an attacker
  page with no CSP of its own) is exercised directly in
  `cross_origin_websocket_upgrade_is_rejected` via a raw HTTP request that bypasses
  any browser policy entirely, same as a real unrestricted attacker page would.
- Tests (`crates/looq/tests/cli.rs`): `every_response_carries_the_csp_header`,
  `served_page_embeds_a_nonempty_per_process_token`,
  `cross_origin_websocket_upgrade_is_rejected`,
  `same_origin_websocket_upgrade_succeeds`,
  `websocket_connection_without_a_token_is_closed_without_data`,
  `websocket_connection_with_wrong_token_is_closed_without_data`. Every pre-existing
  `/ws` test's `connect_ws` helper now fetches the real token from the served page
  and performs the auth handshake before reading anything — `snapshot_then_lines_...
  _to_multiple_clients` covers "multiple tabs work" as a side effect of already
  opening two independent authenticated connections.
- `binding_to_all_interfaces_warns_before_the_banner`/`loopback_bind_stays_quiet`
  (pre-existing, `bootstrap-cli-and-wasm-skeleton`) re-run and still pass — the
  `--host` warning still fires and still tells the truth (task 2.7).

**File size limits (TDR §14) — resolved, no longer "TBD by benchmark":**

Measured via Chrome DevTools Protocol (`Performance.getMetrics` around a forced
`HeapProfiler.collectGarbage`, real Chromium via Playwright, real file picker) on
`bench-{50,100,200}mb.jsonl` (generated by concatenating the existing
`scripts/gen-bench-fixture.py` 1MB fixture N times): main-thread JS heap growth was a
strikingly consistent **~3.4x the raw file size at every size tried** (50MB: 3.40x —
170,195,864 bytes heap growth / 50,007,500 bytes file; 100MB: 3.40x —
340,482,588/100,015,000 after subtracting the ~2.3MB post-GC baseline; 200MB: 3.39x —
678,455,740/200,030,000). Parse time scaled linearly too, ~80ms/MB at these sizes
(50MB: 2,989.1ms; 100MB: 8,004.8ms; 200MB: 15,970.6ms) — comfortably under the
<200ms/MB TDR §11 target even at 200MB (~2.4x under). This is a **JS-heap**
measurement, not wasm32 linear memory — `looq-core`'s own retained state
(diagnostics, field inventory) is capped by `DEFAULT_DIAGNOSTIC_CAP`/
`DEFAULT_FIELD_VALUE_CAP` (`log-parsing-core`) and stays roughly constant regardless
of file size, so the "3-10x overhead" TDR §14 originally flagged as a wasm32-memory
risk turns out in this implementation to live entirely in the browser's main-thread
JS heap, not the WASM module's own address space.

`web/src/limits.ts`: `WARN_THRESHOLD_BYTES = 50 * 1024 * 1024` (above this, a
continue/cancel confirm banner before parsing — ~3s+ waits and hundreds of MB of heap
start here) and `HARD_CAP_BYTES = 400 * 1024 * 1024` (above this, outright refusal
with an explanation — at the measured ~3.4x ratio this extrapolates to ~1.36GB of JS
heap and ~32s of parse time; chosen to stay comfortably inside a single tab's
practical budget on an ordinary desktop browser running alongside other tabs, not the
multi-GB ceiling a 64-bit browser process could reach in isolation — 200MB completed
cleanly at 678MB heap with zero signs of degradation, so 400MB has real headroom
below it, not just below the point of catastrophic failure). TDR §14's table updated
in place with these numbers and the measurement they came from.

Verified end to end against the real release binary: a 95.4MB file triggers the
confirm banner (`Continue`/`Cancel`), continuing parses it successfully; a 430.2MB
file is refused outright with the hard-cap message, no parse attempted, previous
state untouched.

**Error states — `web/src/limits.ts`, `web/src/binary-detect.ts`,
`components/looq-app.ts`:**

- Empty file (`file.size === 0`): dedicated message, checked before any other
  preflight step.
- Binary file: `looksBinary()` scans the first 8KiB for a NUL byte; if found, the
  same confirm-banner mechanism as the size warning offers "proceed anyway" rather
  than refusing outright (design.md D4 — a log with one stray NUL byte is real).
  Also doubles as the unreadable-file check: `File.slice().arrayBuffer()` throwing
  (e.g. permission revoked after selection) reports "couldn't read ‹file›" the same
  way.
- WASM module load failure: reported as its own message
  (`the WASM parser failed to load: ...`), not a generic parse-failure string;
  recovery on the next attempt (worker disposal on `WorkerInitError`, from
  `browser-app-shell`) re-verified still working against the real binary.
- Format produced zero entries: `renderStatus()`'s `done` case now distinguishes
  `entriesEmitted === 0 && totalLines > 0` ("read N line(s), but none matched a known
  log format") from both an empty file (caught earlier, different message) and a
  filter matching nothing (`filtering-and-search`'s existing, unrelated "Filters and
  search exclude all N entries" in the table). First verified by direct property
  injection on the live component instance
  (`app.entriesEmitted = 0; app.totalLines = 5; app.renderStatus()`), then confirmed
  for real end to end: opening `plain-for-forced-json.log` (3 plain-text lines) with
  `#format=json` in the URL produced exactly this message
  (`"read 3 line(s), but none matched a known log format (0 entries) ... — try a
  different file, or override detection with #format="`) — see the correction below,
  an earlier attempt at this same real-file test looked like it hadn't worked, which
  turned out to be a test-harness artifact, not a product bug.
- **A real bug found by Playwright, not by reading the code**: the new
  `.error-banner`/`.confirm-banner { display: flex }` CSS rules had the same
  cascade specificity ((0,1,0), one class) as the browser's own
  `[hidden] { display: none }` UA rule, and being an *author* rule, won the tie —
  both banners rendered empty-but-visible on every page load regardless of the
  `hidden` attribute. A screenshot during routine verification caught it
  immediately; a code read would not have (the CSS looked correct in isolation).
  Fixed with an explicit `.error-banner[hidden], .confirm-banner[hidden] {
  display: none }` rule (specificity (0,2,0), wins outright).
- Failure preserves prior state (design.md, "Failure does not destroy existing
  state"): `openFile` no longer hides `#results` at the start of a new attempt —
  only a *successful* parse replaces what's shown. Errors go to a separate,
  independent `errorMessage`/`#error-banner`, decoupled from the `status`/`#status`
  line that reflects the last successfully loaded file. Verified: opened
  `bench-1mb.jsonl` (7987 entries), then `empty.log` — the empty-file error appeared
  in its own banner while the table, timeline and status line still showed
  `bench-1mb.jsonl`'s 7987 entries, untouched.
- Errors persist until dismissed (`#error-banner-dismiss`) or superseded by a new
  attempt — no timer, no fade.

**Theme — `web/src/theme.ts`, `web/src/style.css`:**

Full token-based rewrite: every color in `style.css` (previously a mix of hardcoded
hex/rgba values, several of which were only ever legible against a light background
— e.g. `.conn-indicator.conn-connecting`'s `rgba(217,119,6,0.2)` background with
`#92400e` text, unreadable on a dark background) now reads from CSS custom
properties (`--bg`, `--fg`, `--muted`, `--accent(-fg/-bg)`, `--error(-bg)`,
`--warn(-bg)`, `--success`, `--hl-bg`, `--border`, `--code-bg`), defined once for
light (`:root`, the default) and re-pointed under both `@media (prefers-color-scheme:
dark)` (guarded `:root:not([data-theme="light"])`, the "Auto" case) and
`:root[data-theme="dark"]` (the explicit override, which must win over the system
preference in both directions). `web/src/theme.ts` applies the stored override (or
falls through to system preference) before the toggle button even exists, then wires
a click handler cycling Auto → Light → Dark → Auto, persisting explicit choices to
`localStorage` (`looq-theme`) and clearing back to "follow the system" on the third
click.

Verified via real Chromium screenshots (not just computed-style checks): dark mode —
level badges, timeline bars, filter chips, table rows, and the theme toggle itself
all legible against `#121212`; light mode — same elements against `#ffffff`, table
and filters unaffected by the theme switch (state persists across toggling). Reload
after choosing Dark: `data-theme="dark"` still applied, toggle button still reads
"Theme: Dark" — override persists as designed. No external fonts/stylesheets/images
anywhere in `style.css` — confirmed by inspection (`grep`), not just assumed, since
that's also what makes the CSP's `default-src 'self'` viable in the first place.

**Performance pass (TDR §11) — all four targets re-measured, no regressions, no
target downgraded:**

| Target | Budget | Measured | Method |
|---|---|---|---|
| Filter latency, 10k entries | <50ms | avg 3.63ms, max 9.8ms | `app.applyPredicate()` called directly, 20 `requestAnimationFrame`-paced samples, `target/fixture-10k.jsonl` in the real release binary |
| Live-tail e2e latency | <100ms | avg 42.8ms, max 78ms (10 samples) | Marker lines with an embedded epoch-ms timestamp appended to a file behind `tail -f \| looq --stdin`; a `MutationObserver` recorded `Date.now()` the first time each marker's row appeared in the DOM — same OS clock on both ends, no RPC round trip in the measurement itself |
| Parse throughput | <200ms/MB | ~80ms/MB at 50-200MB (2.4-2.5x under); ~110-150ms for the 1MB fixture (well under the ≈190.7ms proportional budget) | Real file picker, `bench-{1,50,100,200}mb.jsonl`, status line's own reported elapsed time |
| Binary cold start | <100ms | 19.45-24.62ms over 5 runs (avg ~21ms) | `time.time()` around spawning `./target/release/looq --stdin --port N < /dev/null` and polling `curl` until `/` answers 200 |

All four comfortably inside target; none needed a fix or a downgrade. The filter and
parse-throughput numbers are consistent with (in some cases better than) the
per-change measurements recorded earlier in this file — the security/error-state/
theme changes in this session didn't touch the predicate, index or parser hot paths.

**Feature freeze sweep (task group 6) — P0 (F-1…F-7, F-9…F-13) and P1 (F-8, F-14,
F-15) against the golden path, this session, against the real release binary:**

- F-1 (open file), F-3 (auto-detect JSON/logfmt/plain), F-10 (WASM core), F-11 (TS
  UI), F-12 (single binary), F-13 (File API, zero backend reads) — all exercised
  repeatedly across every error-state/size-cap/theme check above; solid.
- F-2 (stdin pipe), F-9 (live tail WebSocket) — exercised via the `tail -f`-fed
  live-tail session used for the e2e latency measurement; `LIVE` indicator, entry
  count, and the new auth handshake all worked together without any behavior change
  visible to the user.
- F-4 (timeline + range), F-5 (virtual table), F-6 (field filters), F-7 (full-text
  search) — chip toggling, search-box typing (with URL hash updating to `q=cache`),
  and the 10k-entry filter-latency measurement all exercised this path; solid. Drag-
  select itself was not re-driven interactively this session (known headless-
  automation gap from `timeline-and-table`'s own devlog entry, not re-investigated
  here) — `setActiveRange`/the underlying predicate path was exercised via the range
  hash instead.
- F-8 (theme) — this change; see above.
- F-14 (auto-open) — not re-exercised interactively this session (opening a real
  default browser isn't meaningful to check the same way in this sandbox); CLI flag
  parsing/dispatch is unchanged code, covered by existing `cli.rs` tests, which all
  still pass.
- F-15 (URL hash) — chip/search/range changes all round-tripped through
  `location.hash` correctly during the checks above; unchanged code path, no
  regression found.

No new defects found in any P0/P1 feature. One real defect *was* found and fixed
during this sweep (the `[hidden]` CSS specificity bug above) — a release-hardening
addition, not a golden-path regression.

**Release build:**

```
cargo build --release -p looq            # after adding `strip = true` to [profile.release]
ls -la target/release/looq               # 2.3 MB (was 2.7 MB before strip=true)
ls -la crates/looq/assets/wasm/core.wasm # 190.6 KB (see the wasm-opt toolchain-fix note below)
gzip -9 -c web/dist/assets/index.js | wc -c   # 39,109 B
gzip -9 -c web/dist/assets/index.css | wc -c  # 2,651 B
ls -la web/dist/assets/worker.js              # 4.8 KB (ungzipped, as reported by prior changes)
```

Against TDR §5's budget: binary **2.3MB vs ~10MB** (77% headroom), `core.wasm`
**190.6KB vs ~300KB** (36% headroom — down slightly from `browser-app-shell`'s
194.0KB, no `looq-core`/`looq-wasm` source touched; see the `wasm-opt` toolchain-fix
note below for why the number moved at all), TS bundle **~41.7KB gzipped
(index.js+css) vs <200KB** (~79% headroom). **Platform note:** this
sandbox is macOS arm64 with no cross-linker toolchain available (`zig`,
`x86_64-unknown-linux-gnu-gcc`, `musl-gcc` all absent; no Docker; `cross` not
installed) — the binary above is the macOS arm64 build, not the Linux x86_64 TDR §5
commits to as the minimum release target. This is an environment limitation, not a
decision to skip the target; recorded plainly rather than silently substituted.
`[profile.release]` gained `strip = true` (workspace-wide) — smaller distributed
binary, no local path/symbol leakage; `core.wasm` is unaffected (built separately by
`wasm-pack`/`wasm-opt`, not this profile).

**Fresh-machine checklist — environment-limited, closest available proxy used:**
no container/VM without the dev toolchain was available in this sandbox (no Docker).
As the closest practical proxy, the compiled `target/release/looq` binary was run
directly (not via `cargo run`, not from within an IDE/dev-server context) against
fresh ports and fresh Chromium tabs for every check in this session — Flow 1 (open a
file), Flow 2 (live tail via `tail -f`), and Flow 3 (filter chips + search + URL hash,
confirmed the hash reflects state correctly) all completed successfully this way.
What this proxy does **not** rule out: a dependency on something present in this
specific macOS/Homebrew/Rust-toolchain environment that a genuinely clean Linux
container would lack (e.g. a dynamically-linked library assumed present). **NEEDS
HUMAN DECISION**: an actual container/VM run (e.g. a minimal `debian:bookworm`
image with nothing but the binary copied in) is still owed before the 0.1.0 release,
same caveat `bootstrap-cli-and-wasm-skeleton` recorded for its `dtruss` gap — this
environment cannot close it, only get as close as a real proxy allows.

**Bug found during the build/verify loop itself, not the application:**
`cargo build --release -p looq`, run shortly after `scripts/build-frontend.sh`
rewrote `crates/looq/assets/`, sometimes did not pick up the changed asset *content*
(same file paths) and kept serving the previous build's `include_bytes!`-embedded
bytes — caught by diffing the running server's `/assets/index.css` response against
the file on disk (different MD5s) after a CSS fix appeared to have no effect in the
browser. `scripts/build-frontend.sh` now ends with `touch
crates/looq/src/assets.rs`, forcing rustc to reprocess the file (and therefore its
`include_str!`/`include_bytes!` macros) on the next build regardless of the
fingerprinting question — cheap, and closes a real "the binary silently serves stale
assets" failure mode that fits this project's own "never silent" rule as much as any
in-product bug would.

**A second, unrelated build-environment problem found during the same final
verification pass**: a from-scratch `scripts/build-frontend.sh` run (after clearing
`target/frontend-build-tmp`, simulating a genuinely clean rebuild rather than an
incremental one) failed at the `wasm-opt` optimization step —
`rustc 1.95.0`'s `wasm32-unknown-unknown` codegen now emits bulk-memory
(`memory.copy`), sign-extension, mutable-globals and nontrapping-float-to-int
instructions by default, and `wasm-pack`'s cached `wasm-opt` (bundled version 117,
downloaded some time before this session) rejected the module outright, one missing
feature at a time, as `wasm-validator` hit each: first `memory.copy`, then
`i32.trunc_sat_f64_s` even after that first fix. Not a regression in this crate's own
code (`crates/looq-wasm`/`crates/looq-core` are untouched by `release-hardening`) —
purely a toolchain-vs-cached-tool version mismatch, caught only because this change's
own verification loop happened to include a truly clean rebuild rather than trusting
the incrementally-rebuilt artifacts already sitting in `crates/looq/assets/` (which
were themselves still correct throughout — this was never a risk to the shipped
binary, only to reproducing the build from scratch on this machine again later).
Fixed properly, not worked around: installed a current `binaryen` (`brew install
binaryen`, version 132) and added
`crates/looq-wasm/Cargo.toml`'s `[package.metadata.wasm-pack.profile.release]
wasm-opt = ["-O", "--enable-bulk-memory-opt", "--enable-sign-ext",
"--enable-mutable-globals", "--enable-nontrapping-float-to-int"]` so `wasm-opt` is
told to accept the exact feature set current `rustc` actually emits, rather than
disabling optimization (`wasm-opt = false`, the tool's own suggested escape hatch)
and giving up the size reduction it provides. Re-verified clean: `core.wasm` rebuilt
successfully at **190.6KB** (down slightly from 194.0KB, likely the newer `wasm-opt`
optimizing marginally better — still comfortably under the ~300KB budget), all 103
Rust tests still pass against the rebuilt binary, and a real browser parse of the
1MB fixture through the rebuilt `core.wasm` produced the expected 7,987 entries.
`docs/devlog.md`'s and `CLAUDE.md`'s own "if there's no ast-index... rebuild before
using" convention aside, this is the same class of lesson: verify the actual clean
build, don't trust that incremental state matches what a fresh checkout would
produce.

**Tests:** 103 Rust tests (19 unit in `looq` + 27 integration in `crates/looq/tests/
cli.rs`, up from 26 — 6 new security tests added, replacing/updating the prior 21;
35 + 22 unchanged in `looq-core`), all passing (`cargo test --workspace --release`;
one incidental flake on a full concurrent run, passed clean in isolation — same
known sandbox process-spawn-contention characteristic `live-tail`'s devlog already
recorded, not investigated further since it didn't reproduce). 55 TypeScript tests
unchanged and passing (`npm test`) — this change added no new `.test.ts` files
(the new modules — `theme.ts`, `limits.ts`, `binary-detect.ts`, `token.ts` — were
verified via the real-browser Playwright checks above, consistent with this
project's established pattern of verifying UI-facing, environment-dependent logic
end-to-end rather than only in isolation). `cargo fmt --all -- --check` and
`cargo clippy --workspace --all-targets -- -D warnings` clean. `npm run typecheck`
clean (both `tsconfig.json` and `tsconfig.worker.json`).

**Decisions made without a human available to ask, recorded per the task's own
instructions:**

- **Token generation via `rand::thread_rng()`**, adding `rand` as a new native-only
  dependency of `crates/looq` (not `looq-core`/`looq-wasm`, so no `core.wasm` budget
  impact) rather than the undocumented-but-common `std::collections::hash_map::
  RandomState` trick for OS randomness without a new dependency. Chosen for
  auditability: a security-relevant token generator built on a well-known crate's
  documented CSPRNG is more defensible than one relying on `RandomState`'s
  internal (not officially guaranteed) behavior.
- **CSP's `style-src 'unsafe-inline'`** (see Security above) — the alternative
  (reworking the virtual-scrolled table onto generated stylesheet rules via
  `CSSStyleSheet.insertRule` to avoid any inline style) was not attempted: a larger,
  riskier change to code this project has already measured and tuned (`timeline-
  and-table`'s 50k-row/~17ms-per-frame result) for a security property
  (`style-src` strictness) with a narrow, well-understood downside (no code
  execution via CSS alone; this app never renders attacker-controlled CSS).
- **Origin check allows a missing `Origin` header through** rather than rejecting
  it — the alternative (require `Origin` on every connection) would break every
  non-browser WS client (`wscat`, this crate's own integration tests) for a
  protection that specifically targets browser-tab cross-origin hijacking, which
  by definition always carries an `Origin` header.
- **Auth timeout of 5 seconds** — not measured/tuned, a judgement call consistent
  with the "short timeout" wording in both the proposal and the security spec; the
  real client (`web/src/live-tail.ts`) sends its auth message synchronously on the
  WebSocket's `open` event, so 5s is generous headroom, not a number anything in
  this change depends on being tight.
- **`WARN_THRESHOLD_BYTES`/`HARD_CAP_BYTES` exact values** (50MB / 400MB) — derived
  from the measured ~3.4x ratio and a target JS-heap ceiling of roughly 1.5GB for
  the hard cap, chosen as "comfortably inside a single ordinary tab's budget," not
  itself a measured failure point (200MB completed with no sign of trouble) — a
  reasoned choice from real data, not an arbitrary guess, but the specific ceiling
  (1.5GB, not e.g. 1GB or 2GB) is a judgement call.
- **A false alarm worth recording so it isn't rediscovered the same way**: a first
  attempt to build a real zero-entries fixture (`#format=json` over a plain-text
  file) appeared not to force JSON detection — the detection panel still read
  "Format fell back to plain text." Before concluding `#format=` was broken, tried
  `page.goto()` from a *hash-only-different* URL (matching a prior URL already
  loaded in the same tab) — which `filtering-and-search`'s own devlog entry already
  documented as a same-document navigation in real Chrome, not a reload, meaning
  `connectedCallback()` (and its one-time `location.hash` read into `pendingHash`)
  never re-ran. Navigating through `about:blank` first, exactly as that entry
  prescribes, gave an honest fresh tab: `pendingHash.formatOverride` was `"json"`
  as expected, and opening the plain-text fixture produced precisely the
  zero-entries message this change added. `#format=` was never broken; the first
  attempt's methodology was. No code changed as a result — recorded because it
  cost real time to work out and the fix (navigate through `about:blank`) is easy
  to forget a second time.

**NEEDS HUMAN DECISION:**

- **Fresh-machine verification is a proxy, not the real thing** (see above) — a
  genuine container/VM run with zero dev-environment assumptions is still owed
  before 0.1.0 ships.
- **Which additional platforms get 0.1.0 binaries** (task 7.5) — this change
  produced sizes for macOS arm64 only, given the sandbox's toolchain gap; Linux
  x86_64 (TDR §5's committed minimum) needs an actual Linux build environment,
  which this session didn't have.
- **The two `design.md` open questions not already resolved by design.md's own
  D1/D6 text** — "token vs. full URL for the SSH port-forward case" and "does the
  theme preference belong in localStorage or the URL hash": both were already
  answered in design.md's own D1 ("presented as the first message on the
  WebSocket... never a query string") and D6 ("stored in the browser [via
  localStorage]... preserves the zero-config principle") — implemented exactly as
  those decisions state, not re-litigated here.

## 2026-08-16 — `frontend-visual-redesign`: darker palette, six level colors, tighter density

Value/spacing-only retune of `web/src/style.css` + two component files, per the brainstorm doc
`docs/superpowers/specs/2026-08-14-frontend-visual-redesign.md` and this change's `design.md`
D1–D8. No markup, no layout, no new dependencies.

**Shipped:**

- D1: dark `--bg` `#121212`→`#0b0c0f`, `--border` opacity `0.22`→`0.13`, `--code-bg` opacity
  `0.1`→`0.06`, `--accent` `#7aa8ff`→`#6e8bff`, `--error` `#ff8a80`→`#f0525a`, `--warn`
  `#ffca7a`→`#f5a742`. `--fg`/`--muted` unchanged.
- D2: new `--bg-elevated` token (`#131417` dark, `#f7f7f8` light), applied to `.topbar` only.
- D3: six new `--level-*`/`--level-*-bg` token pairs (trace/debug/info/warn/error/fatal), light and
  dark. `.level-badge.level-*` repointed one rule per level — `fatal`/`critical` share
  `--level-fatal` (an alias, not a seventh level); every other level gets its own token. This is
  the fix for TRACE/DEBUG previously sharing `--muted` and INFO borrowing `--accent-fg`.
- D4: timeline background series fill `rgba(148,163,184,0.45)`→`rgba(148,163,184,0.25)`. Foreground
  (filtered) series switched from a hardcoded blue to `--accent`, read via
  `getComputedStyle(document.documentElement).getPropertyValue('--accent')` at render time and
  converted to rgba via a small `hexToRgba` helper (`web/src/components/looq-timeline.ts`) so a
  future token change doesn't need a code edit. **Resolved opacity: fill 0.6, stroke 0.9** — chosen
  by rendering a filtered timeline (level=ERROR chip) against the real `#0b0c0f` background in a
  Playwright-driven browser and comparing the two series side by side: at 0.6/0.9 the accent bars
  read clearly as "the interesting subset" without washing out, while the muted 0.25 background
  bars stayed legible as context rather than competing for attention.
- D5: filter-bar padding `0.5rem 0.75rem`→`0.4rem 0.6rem`, filter-chip padding `0.15em 0.6em`→
  `0.12em 0.5em`, error/confirm banner padding `0.6em 0.9em`→`0.5em 0.75em`, detail-panel padding
  `0.75rem`→`0.6rem`. Container radius `6px`→`5px` on theme-toggle, filter-bar, error-banner,
  confirm-banner, detail-panel; drop-zone (actually `8px` in the shipped code, not the `6px`
  `design.md` implied) also moved to `5px` since it's named in the same list. Table-viewport radius
  and badge/pill radii left untouched, as specified. **Held up as-is** — checked both a filtered and
  unfiltered dark-mode render plus a light-mode render with the detail panel open; nothing read as
  cramped next to the already-dense 24px rows, so none of the D5 percentages needed easing back.
- D6: `.conn-indicator.conn-live`/`.conn-disconnected` switched from solid one-off hex
  (`#16a34a`/`#b91c1c` with white text) to the same tinted-text-on-tinted-background pattern the
  other two connection states already used, pointing at `--level-info`/`--level-error` — keeps the
  four `.conn-indicator` states visually consistent with each other and with the level badges,
  and reads correctly in both themes (checked via forced `.conn-live`/`.conn-disconnected` class
  swaps in a live page, both themes).
- D7: **left open, not resolved.** No live human was reachable to answer the monospace-scope
  question a third time; per the change's own instruction, proceeded with the existing split
  (`ui-monospace` for data, `system-ui` for chrome) rather than assuming an answer. Still an open
  question for whoever picks this up next.

**Verification:**

- Playwright against the real compiled binary (`scripts/build-frontend.sh` + `cargo build --release
  -p looq`), a 12-line fixture covering all six levels twice each.
- File mode needed a real TTY on stdin to select (`mode_for` in `crates/looq/src/cli.rs` always
  picks stdin mode on a non-tty stdin, which every non-interactive shell has) — worked around with
  `script -q /dev/null looq …` to allocate a pty, confirmed `file mode` banner + drop-zone +
  file-upload flow all render correctly with the new tokens. Stream mode tested directly (its own
  shell, `looq-live-tail.ts`, non-tty stdin selects it by default).
- All six level badges confirmed pairwise distinct by close-up screenshot in both dark and light
  (gray → blue → green → amber → red → purple in both appearances) — satisfies the new `theming`
  scenario "Every level has its own color."
- Top bar vs. body separation (`--bg-elevated`) checked visually in both themes — subtle but present,
  as designed (a "small step up," not a strong border).
- Theme toggle mechanics (`theming`'s existing behavioral requirements, unaffected by this change in
  principle) re-verified against the new tokens: Auto→Light→Dark→Auto cycle, `localStorage`
  persistence across a real reload, explicit dark override surviving a light system preference —
  all still correct.
- Error banner and confirm banner visually checked (forced-visible via DOM, since neither's real
  trigger condition — empty file, large-file confirm — was convenient to reproduce with the small
  fixture) — legible, correctly padded/radiused, no regression.

**Tests:**

- `cargo test --workspace`: one flaky failure on the first run
  (`fast_producer_slow_consumer_never_blocks_and_reports_an_accurate_gap`, a `wait_until_serving`
  5s timeout under concurrent compile load) that passed cleanly both in isolation and on a full
  unconstrained re-run — pre-existing timing flakiness, not caused by this change (this change
  touched no Rust code at all). Full suite: 46+ tests, 0 failures on the clean re-run.
- `npm run test` (vitest): 55/55 passed, unmodified.
- `npm run typecheck`: clean after fixing a `noUncheckedIndexedAccess` complaint in the new
  `hexToRgba` helper (switched from regex capture groups to `hex.slice()` on the pre-validated
  string).
- `cargo fmt --check` and `cargo clippy --workspace --all-targets -- -D warnings`: clean.

**Deviation from the task list worth flagging:** `tasks.md` 2.4 and the proposal's Impact section
both describe the LIVE/DISCONNECTED color change as living in `looq-live-tail.ts` — in the actual
code those colors are CSS rules (`.conn-indicator.conn-live`/`.conn-disconnected` in `style.css`),
not TypeScript constants; `looq-live-tail.ts` only supplies the state labels. Implemented the color
change in `style.css` where the colors actually live; no `looq-live-tail.ts` edit was needed or
made.

## 2026-08-16 — `frontend-terminal-overhaul`: monospace everywhere, letter badges, topbar accent

Markup-level polish on top of `frontend-visual-redesign`'s palette/density pass, per this change's
`design.md` D1–D4. Resolves D7 from the prior change (typography scope), left open there after two
unanswered rounds — the user has now answered it directly ("шрифт хочется более айтишный"): monospace
everywhere, not just data surfaces.

**Shipped:**

- D1: new `--font-mono` token (`ui-monospace, "JetBrains Mono", "Cascadia Code", "SF Mono", Menlo,
  Consolas, "Liberation Mono", monospace`, one declaration, not duplicated per theme block — a font
  stack isn't a themeable color). `body`'s `font-family` switched from `system-ui, -apple-system,
  sans-serif` to `var(--font-mono)`. Deleted 10 duplicate `font-family: ui-monospace, SFMono-Regular,
  Menlo, monospace;` declarations across `style.css` (`.status`, `.diagnostics-counts`/
  `.diagnostics-examples`, `.conn-indicator`, `.lines-per-sec`, `.timeline-summary`,
  `.entry-table-summary`, `.entry-row`, `.detail-core dd`, `.detail-message`, `.detail-fields` —
  more than the "six-plus" `tasks.md` estimated).
- D1 caught a real regression during verification, not assumed away: `.detail-message` is a `<pre>`
  element, and `<pre>` carries its own UA-stylesheet `font-family: monospace` rule — a direct
  selector match, not inheritance, so deleting its explicit declaration exposed the browser's bare
  `monospace` default instead of `--font-mono`'s broadened stack. Confirmed via
  `getComputedStyle(...).fontFamily` before/after in a real Chromium tab (Playwright): every other
  one of the 10 elements inherits `--font-mono` correctly (none of them are `pre`/`code`/`kbd`/
  `samp`); `.detail-message` needed its declaration restored, explicitly pointed at
  `var(--font-mono)`, with a comment explaining why it's not just inheriting like its siblings.
- D2: `.filter-field-name` gained `text-transform: uppercase; letter-spacing: 0.04em;`. Scoped to
  that one class only — not applied anywhere else.
- D3: entry-table level badges are now single-letter circles (`display: inline-flex`, `1.6em` ×
  `1.6em`, `border-radius: 50%`) instead of padded text pills. Visible text is the level's first
  letter (T/D/I/W/E/F — confirmed unique against `crates/looq-core/src/level.rs`'s canonical
  `Level::as_str()` output, which is what actually reaches the frontend; `entry.level` is never
  `"WARNING"`/`"CRITICAL"`, those are alias inputs normalized before the DTO crosses the WASM
  boundary). `aria-label` and `title` both carry the full word. The six `.level-badge.level-*`
  color rules are untouched — shape/text-content change only, as scoped. Filter-bar chips
  (`looq-filter-bar.ts`) were not touched — verified by reading `chipHtml`/`renderChips`, which
  render `escapeHtml(value)` unmodified; level values reaching the chip bar come from
  `EntryIndex.levelStats` (`entry-index.ts`), keyed by the same full canonical `entry.level` string.
- D4: `.topbar` gained `padding: 0.5rem 0.75rem` (the density-scale value `design.md` proposed, kept
  as-is — it read correctly against the new border, no adjustment needed) and
  `border-bottom: 2px solid var(--accent)`. Added `.brand-mark` — a `0.6em` × `0.6em` filled square,
  `background: var(--accent)`, `display: inline-block` — via a `<span class="brand-mark"
  aria-hidden="true"></span>` before `<h1>looq</h1>` in `web/index.html`. `aria-hidden` keeps the
  `<h1>`'s accessible name as plain "looq" (confirmed by accessibility snapshot), so the mark is
  decoration only, not a11y noise.

**Verification:**

- Built the real frontend (`scripts/build-frontend.sh`, `wasm-pack` + `vite build`) and ran the
  actual compiled `looq` binary (`cargo build -p looq`), driven by Playwright against
  `http://127.0.0.1:PORT` — not `vite dev`, which can't serve `/wasm/core.wasm` as a dynamic import
  (`public/` assets are copied as-is, not transform-eligible; confirmed by hitting exactly that
  error before switching to the real binary). File mode needed a real TTY on stdin
  (`mode_for` in `crates/looq/src/cli.rs` always picks stdin mode on a non-tty stdin) — worked
  around with `script -q /dev/null looq …`, same as `frontend-visual-redesign`'s precedent.
- **Accessibility guarantee (the one part of this change tied to an actual spec requirement,**
  `entry-table` spec's new "Abbreviated level still exposes its full name" scenario): a 6-line
  fixture covering all six levels, Playwright `browser_snapshot` (reads the accessibility tree, not
  visual text) showed `generic "TRACE" [ref=…]: T` for every badge — accessible name is the full
  level word, visible text is the single letter. Confirmed `aria-label`/`title` both equal the full
  word via `getAttribute`. Filter-bar chips in the same snapshot still read `"TRACE (1)"`,
  `"DEBUG (1)"`, etc. — untouched, full text.
- **Font-family regression check (task 1.3):** `getComputedStyle(el).fontFamily === getComputedStyle(document.body).fontFamily`
  checked for all 10 previously-duplicated elements across three fixture scenarios (plain entries
  for the table/detail/summary elements, a live-tail stdin session for `.conn-indicator`/
  `.lines-per-sec`, a malformed-lines file for `.diagnostics-counts`/`.diagnostics-examples`) — all
  matched after the `.detail-message` fix above; none before it.
- **Click interaction (task 6.2):** clicked directly on the `.level-badge.level-info` circle
  (real Playwright pointer click, not a synthetic event dispatch) and confirmed the row it belongs
  to becomes `.selected` and its detail panel opens with the matching entry — the smaller circular
  target is still a real, correctly-delegated click target (`rowsEl`'s click listener uses
  `closest("[data-ordinal]")`, unaffected by the badge's shape).
- All six level circles checked for letter legibility against their background color, both themes,
  via screenshot (`docs/ui.png`-style close look, not just a pass/fail assertion) — gray/blue/green/
  amber/red/purple all read clearly in both appearances.
- Topbar checked in both themes: reads as a visible strip now (not the prior no-padding patch),
  accent border and brand-mark square both visible against `--bg`/`--bg-elevated`, page width/
  centering unchanged (no full-bleed).
- Hit one unrelated, pre-existing bug while constructing a malformed-lines fixture for the
  `.diagnostics-*` font check: `failed to parse …: TypeError: Cannot read properties of undefined
  (reading 'replace')` from `looq-diagnostics.ts`'s `escapeHtml`. Not caused by this change (no
  diagnostics/parsing code touched here) — not investigated further, noted here rather than fixed,
  since it's outside this change's scope.

**Tests:**

- `cargo test --workspace`: 103 passed (19+27+35+22 across the four crates with tests), 0 failed —
  entirely unaffected, no Rust touched, as expected.
- `npm run test` (vitest): 55/55 passed, unmodified.
- `npm run typecheck`: clean.
- `cargo fmt --check` and `cargo clippy --workspace --all-targets -- -D warnings`: clean.

**README check (task 7.2):** no changes needed — same as `frontend-visual-redesign`'s own precedent,
this is a visual/presentation-only change with no install-step, command, flag, or output difference.

**Five stale-Purpose specs noted, not fixed (task 5.1 scope boundary):** `entry-table`'s own
`## Purpose` ("TBD - created by archiving change timeline-and-table...") was rewritten as part of
archiving this change, since this change already touches `entry-table`'s delta. `entry-index`,
`filtering`, `search`, `timeline`, and `url-state` have the same stale-Purpose placeholder and were
deliberately left alone — fixing them here would be scope creep past what this change's `tasks.md`
asked for. Listed below under Ideas for later.

## 2026-08-17 — the `escapeHtml(undefined)` crash was the `Option::None` boundary bug

Committed the two frontend passes that were still sitting in the working tree (`58e7622` for
`web/`, `3df0425` for the `crates/looq/assets/` rebuild — the first commit alone would have failed
CI's `frontend-artifact-staleness` job, since the embedded copy was still the pre-restyle build),
then closed the one open bug on the Ideas list.

**The bug was already fixed, by `f8bd98e` ("serialize wasm Option fields as null, not undefined"),
committed a few hours after `frontend-terminal-overhaul`'s devlog entry was written.** The entry
attributed the `TypeError: Cannot read properties of undefined (reading 'replace')` to
`looq-diagnostics.ts`'s `escapeHtml` — that attribution was wrong. `DiagnosticsSummaryDto` has no
`Option` fields, so nothing it hands to `escapeHtml` can be `undefined`; the actual crash is
`entry.level` arriving as `undefined` instead of `null` and reaching `escapeHtml` in
`looq-filter-bar.ts`'s chip rendering (all four components define their own `escapeHtml`, which the
bundle renames to `escapeHtml`/`escapeHtml$1`/… — an easy misread off a production stack trace).
Everything after `setDiagnostics` in `openFile`'s `try` block is inside the same catch, so any of
them shows up as the same `failed to parse <file>: …` banner.

**Verified both directions against the real binary** (Playwright, `script -q /dev/null looq …` for
the TTY, not `vite dev`), with a fixture whose plain-text fallback produces entries that carry no
level (`.playwright-mcp/malformed.jsonl`, 8 lines: valid JSON, a truncated line, a bare text line,
a JSON array, a bad timestamp):

- Current `main`: file opens, 8 rows, level chips `INFO (3) / WARN (1) / ERROR (1) / DEBUG (1)`,
  no error banner. A mostly-valid JSON fixture (13 lines, 3 flagged) also renders the diagnostics
  panel correctly — `invalid_json: 1`, `non_object_json: 1`, `unparsable_timestamp: 1` plus their
  three examples.
- Same build with `.serialize_missing_as_null(true)` commented out: the exact banner text from the
  original report, `failed to parse malformed.jsonl: TypeError: Cannot read properties of undefined
  (reading 'replace')`.

**Build-cache trap worth remembering:** restoring the reverted line with `mv lib.rs.bak lib.rs`
carries the backup's *older* mtime, so `wasm-pack` skipped the rebuild and the "fixed" run still
served the broken wasm — the bug appeared to survive its own fix. `touch` on the source, then
rebuild, and `crates/looq/assets/wasm/core.wasm` came back byte-identical to the committed one
(so the frontend build is reproducible on this machine, which is what the staleness CI job relies
on).

No regression test added: catching this at its real layer needs `wasm-bindgen-test` over the DTO
shapes, which is still the open NEEDS HUMAN DECISION on the Ideas list. Testing it in the frontend
would mean asserting on `undefined` for a field `wasm-types.ts` types as `string | null` — codifying
the wrong contract.

**Tests:** `cargo test --workspace` 103 passed / 0 failed; `npm run test` 55/55; `npm run typecheck`
clean.

**Five stale spec Purposes rewritten** (`entry-index`, `filtering`, `search`, `timeline`,
`url-state`) — all five had read "TBD - created by archiving change … Update Purpose after archive."
since their capabilities were first archived. Each is now a paragraph in `entry-table`'s voice,
written from that spec's own requirements rather than from memory of the change that created it.
Writing `url-state`'s caught a real misstatement in my own first draft: I wrote that a shared link
"carries the view but never the log data", which is backwards — the spec's sharing-caveat
requirement exists precisely *because* the encoded search text and field values are fragments of the
log. `openspec validate --specs --strict`: 20 passed, 0 failed. No `TBD - created by archiving`
strings left anywhere under `openspec/specs/`.

## 2026-08-17 — `frontend-three-pane-layout`: the log fills the window, and filters survive a stream

The page stopped being one vertical stack. `looq-workspace` now owns a viewport-bound CSS grid —
topbar row, collapsible full-width timeline row, then rail / table / detail across the bottom — and
both shells (`looq-app.ts` for file mode, `looq-live-tail.ts` for stdin) mount that one element and
only fill its panes. The `.entry-table-viewport { height: 480px }` rule is gone; the viewport is a
flex child with `min-height: 0`, and the virtual scroller picks the new height up from
`clientHeight` with no code change, exactly as design.md D2 predicted.

**The live-stream click bug, before and after (measured, not argued).** The failure was never in
the filter code: `renderChips()` reassigned the chip container's `innerHTML` on every live batch, so
a control was detached between a human's `mousedown` and `mouseup`, and the browser then synthesises
no `click` at all. Reproduced on the pre-change build (`cargo build -p looq` against the committed
`crates/looq/assets/`, a ~25 lines/sec producer piped into `looq --port 7801`), by pressing a level
chip and releasing it 150ms later:

```
{ stillConnected: false, sameNode: false, clickWouldFire: false,
  filtersChangeEvents: 0, hashBefore: "", hashAfter: "",
  summaryBefore: "Showing all 735 of 735 entries.",
  summaryAfter:  "Showing all 753 of 753 entries." }
```

After the fix (same producer, same 150ms press, `looq --port 7804`):

```
{ stillConnected: true, sameNode: true, clickWouldFire: true,
  filtersChangeEvents: 1, ariaPressed: "true",
  hashBefore: "", hashAfter: "#filter=level%3DERROR",
  summaryBefore: "Showing all 713 of 713 entries.",
  summaryAfter:  "Showing 182 of 731 entries matching the active filters.",
  rate: "23 lines/sec" }
```

Playwright's own `browser_click` passed on *both* builds — it presses and releases in well under a
millisecond, inside the gap between two re-renders. That is why the check here dispatches
`mousedown`, waits 150ms, and then asks the question the browser asks (is the node still connected,
is it still under the pointer?) before deciding whether a `click` would fire at all.

The fix is D4's split: a structural pass keyed by `field` + `value` that only runs when the *set* of
fields or values changes, and a count pass that writes into the count text node of a control that
stays put. New values are inserted at their ordered position without relocating existing nodes, so
no control moves under a finger. Consequence, verified over ~25 live batches (2s at 80ms): the
level control and the high-cardinality field's `<input>` are the same DOM nodes before and after,
the half-typed `req-0001` and its caret at offset 4 are intact, focus never left the input, and the
ERROR count went 261 → 273 in place.

`looq-detection` and `looq-diagnostics` had the same latent problem — live mode pushes into them
every 80ms — so both now keep a persistent `<details>` skeleton and only rewrite content when a
content key actually changes. That is also what makes their collapsed state honest (D7): the
summary line carries the state, so collapsing never hides a warning. With `#format=json` forced onto
a half-broken fixture: summary reads `20 skipped` with warning styling and the section auto-opened
(`open: true`, `looq-diagnostics.className === "warning"`). On a fallback detection the summary
reads `fell back to plain text (50%)` and opens itself.

**Measurements (real numbers, exact method).**
- **50k-entry scroll smoothness, re-measured against `timeline-and-table`'s recorded result** —
  `./scripts/build-frontend.sh && cargo build -p looq`, then
  `python3 scripts/gen-timeline-fixture.py 50000 --jitter 5 > .playwright-mcp/fixture-50k.jsonl`
  opened through the file picker at 1280x800 against the real binary
  (`run_file_mode.py 7803 …`, a pty on stdin so `mode_for` picks file mode). Same recorder as
  before: `requestAnimationFrame` frame times around a 2-second programmatic `scrollTop` sweep from
  0 to `scrollHeight - clientHeight` (= 1,199,610px). **120 frames, avg 16.81ms/frame, max 34.17ms,
  1 frame over the 33.3ms threshold (0.8%).** The recorded baseline was 123 frames, avg 16.80ms,
  max 32.86ms, 0 over — i.e. no regression from moving the detail view out and letting the layout
  set the height. Jump-to-end still rendered ordinals 49998/49999/50000 with no stall (0.0ms for the
  synchronous `scrollTop` pair), DOM row count 25–33 rows for 50,000 entries.
- **No page scrollbar at 1280x800 with 50k entries** — `documentElement.scrollHeight === 800 ===
  clientHeight`, `pageHasScrollbar: false`, first row visible in the initial view, table viewport
  390px. Same numbers in stdin mode (`docScrollHeight 800`, table viewport 337px, 31 rows).
- **The table uses the height it is given** — resizing 1280x800 → 1280x1100 took the viewport from
  390px/16 fully-visible rows to 690px/28, still with no document scrollbar. This needed one new
  line of behavior: a `resize` listener on `looq-entry-table`, because in file mode nothing else
  would re-run `renderVisibleRows()` after a resize.

**Other things that had to be got right, and how they were checked.**
- **URL hash grammar unchanged** (task 7.4). A link produced *before* this change,
  `#range=1754668800000,1754669400000&filter=level%3DWARN&q=job&format=json`, opened on a fresh
  load and applied in full: WARN pressed, `q` back in the search box, both range inputs populated as
  `2025-08-08T16:00:00.000` / `...T16:10:00.000`, 14 of 400 shown, no hash notice. Writing works the
  other way too — the rail's own range inputs produce `range=1754668800000%2C1754669400000`.
- **Detail pane by ordinal, not position** (D6). Selecting a row moved zero rows
  (`rowsMoved: 0`, all bounding boxes identical before/after) — the reflow the old inline panel
  caused is gone. Under a `--max-lines 2000` stream, a selected ordinal that got evicted turned the
  pane into "Entry #1 is no longer retained (evicted at `--max-lines`) — its contents are gone, not
  hidden", rather than describing whatever entry now sits at that index.
- **Keyboard** (task 7.5). 35 focusable controls, all 10 rail sections focusable, `Enter` on the
  timeline `<summary>` collapsed it, `Enter` on a focused WARN row toggled the filter
  (`aria-pressed: true`, hash `#filter=level%3DWARN`, 500 of 2000 shown). Nothing is pointer-only:
  every value control is a `<button>` and every section header is a `<summary>`.
- **Narrow window** (D8). At 1000px the grid collapses to one column in rail → table → detail order
  (all three 1000px wide), every rail section starts closed, and this is the one mode where the
  document itself is allowed to scroll.
- **Privacy copy stays per-mode** (TDR §12). The file-mode paragraph moved out of
  `looq-drop-target` (a component only file mode mounts) into each shell's own rail section, so the
  stdin shell states the WebSocket guarantee and never the browser-only one.

**Decisions taken inside the implementation, not in design.md:**
- **The drop target is one element that moves**, from the center pane (nothing opened yet — the
  `app-shell` "prompts for a file" state) into the rail's collapsed "Open another file" section
  after the first successful parse. Moving the node keeps its listeners and picker state; a second
  instance would have been two sources of `file-selected`.
- **`showResults()` runs before `timelineEl.setIndex()`**, because `uPlot` sizes its canvas from
  `clientWidth` at draw time and the timeline row is still hidden before that call — the first build
  drew a 480px-wide chart in a 1256px row and stayed there. Nothing paints between the two calls, so
  this is not a flash of an unfiltered view.
- **Section `open` state is written exactly once, at creation.** Writing it on update would let a
  live batch reopen a drawer the user just closed. It is deliberately not in the URL hash: the hash
  describes the query, not furniture.

**Tests:** `cargo test --workspace` 103 passed / 0 failed, `npm run test` 55/55, `npm run typecheck`
clean, `cargo fmt --all -- --check` clean, `cargo clippy --workspace --all-targets -- -D warnings`
clean. One caveat worth recording: `fast_producer_slow_consumer_never_blocks_and_reports_an_accurate_gap`
failed once while three `looq` servers and two producers were saturating this machine, and passed
5/5 in isolation and in every run after the load was gone — a timing-sensitive backpressure test, no
Rust code was touched by this change.

**Bundle:** `index.js` 121.81KB / **42.45KB gzipped** (was 87.72KB / 34.01KB), `index.css` 14.83KB /
3.66KB gzipped, `worker.js` unchanged at 4.86KB. Main-bundle gzip total **46.11KB**, about 23% of
TDR §5's 200KB budget. `crates/looq/assets/` rebuilt via `./scripts/build-frontend.sh` and verified
byte-identical across two consecutive runs (same md5 for `index.js`/`index.css`), so CI's
`frontend-artifact-staleness` job has nothing to complain about.

**README:** no behavioral change — install steps, commands, flags, output and limitations are all
untouched — but one paragraph in each README described the filter controls as "a row of chips",
which is no longer what a user sees. `README.md` and `README.ru.md` were updated together in the
same edit ("its own collapsible section in the left rail" / "своя сворачиваемая секция" in the left
panel); nothing else in either file describes the page layout.

**Two defects found while reviewing the implementation, fixed inside the same change (tasks 9.1–9.6)
rather than deferred.** Both come from the entry table's column widths (`3.5rem 14rem 6rem 1fr`)
being inherited unchanged from a full-width table into a narrower centre pane:

- The timestamp column overflowed and ran under the level badge whenever timestamps carry
  milliseconds — visible in stdin mode, invisible in the file fixtures used during implementation.
  Measured before: `scrollWidth 237` against `clientWidth 224` for
  `2026-08-17T10:00:01.600+00:00`. Columns are now `3rem 15.5rem 2.5rem minmax(0, 1fr)`, and
  `.col-timestamp` gained the `overflow: hidden` / `text-overflow: ellipsis` treatment `.col-message`
  already had, so a nanosecond-precision timestamp truncates rather than colliding. Row grids are
  per-row, so content-based column sizing is not available here — rows would stop lining up with each
  other; fixed widths plus clipping is the only arrangement that holds.
- The message column — the actual log content — was the narrowest column at 212px, while the level
  column held 6rem for a 1.6em badge. After: message 252px, level 40px, nothing overflowing in either
  mode (worst-case timestamp overflow measured at 0px across all rendered rows in a live stream).

The timeline's vertical budget was trimmed in the same pass: its summary line, outlier note and range
controls now share one flex row instead of stacking, and the chart is 110px rather than 140px
(`CHART_HEIGHT` in `looq-timeline.ts` and the CSS `min-height` both — uPlot sizes itself from the
constant, so the CSS alone could not shrink it). First entry row at 1280×800 moved from y=401 to
y=334; the table viewport went from 390px to 457px, 33 rows to 36.

Then the timestamp column itself (tasks 9.7–9.8): rows now drop the `+00:00` suffix, because
`Entry::timestamp` is a `DateTime<Utc>` in `looq-core` — every value the parser emits carries the
same six redundant characters, and the column header already says UTC. The strip is deliberately
narrow (`/(\+00:00|Z)$/`): any other offset would stay on screen rather than being silently dropped
if the core's type ever changes, and the full RFC 3339 string stays reachable as the cell's `title`
and in the detail pane, so nothing is actually lost. The column went 15.5rem → 12.5rem and the
message column took the difference.

Final widths at 1280×800: timestamp 200px showing `2025-08-08T15:59:55`, message **300px** — up from
the 212px it started this change with, a 42% gain for the one column that holds the log. Live mode
with millisecond timestamps (`2026-08-17T10:05:27.200`, the longest form in practice) measured 0px
overflow across every rendered row.

Re-verified after the fix: no page scrollbar in either mode, `cargo test --workspace` 103/0,
`npm run test` 55/55, `npm run typecheck` clean, assets byte-identical across two rebuilds.

**Tooling note:** `openspec update` was run this session; it installed `opsx:ff`, `opsx:verify`,
`opsx:sync` and `opsx:bulk-archive` as real commands. `CLAUDE.md`'s flow section now names
`opsx:explore → opsx:ff → opsx:apply → opsx:archive` directly instead of describing "ff" as a manual
one-pass write, and records that a defect found while verifying an unarchived change is fixed inside
that change rather than spun out as a follow-up proposal.

## 2026-08-17 — the same detach-under-the-pointer bug, one layer down: entry rows

Ran the finished build against a live stream to look at it, and selecting a row did nothing — the
detail pane stayed on "Select an entry to inspect". Not a detail-pane bug: `renderVisibleRows()`
still did `this.rowsEl.innerHTML = html.join("")` on every render, exactly the pattern
`frontend-three-pane-layout` removed from the filter rail and did not think to look for in the table.
Under a stream the row you press is detached before you release, no `click` is synthesised, and the
selection never happens. Measured before the fix: `stillConnected: false` 150ms after `mousedown`.

**Fix:** rows are reused instead of rebuilt. `rowsEl` keeps a pool of row elements; each render
grows or shrinks the pool, then writes into a row **only when its content key changed** — the key
being `ordinal | selected | query`, i.e. everything the rendered output depends on. A row showing the
same entry is left completely untouched, so the node under the pointer survives. The row markup moved
out of the class into `createRowElement()` + `renderRowCellsHtml(entry, compiled)`, and
`compiledQueryKey()` turns the compiled query into the key's third component.

After: pressing a row inside a live stream and releasing 150ms later fires the click and fills the
pane (`ordinal 484`, `2026-08-17T10:06:27.200`), while `LIVE` keeps ticking.

**Two things worth writing down about the verification itself**, because both nearly produced a wrong
conclusion:

- The first probe reported `clickWouldFire: false` even after the fix. The cause was the probe, not
  the app: it grabbed `.entry-row[4]`, which is an *overscan* row sitting above the viewport's top
  edge, clipped out of view — `elementFromPoint` at those coordinates returns the toolbar overlaying
  that area. Picking a row whose rect is genuinely inside the viewport rect is what makes the
  measurement mean anything.
- Following the tail is a different case from paused, and only paused is really fixable: while
  autoscroll is on, rows move under the pointer by design. The fix targets the case a user actually
  clicks in — stopped, looking at something specific.

Tests after the fix: `npm run test` 55/55, `npm run typecheck` clean, `cargo test --workspace`
103/0, assets byte-identical across two rebuilds.

**Spec gap closed.** `filtering` carries "Filter controls stay operable while entries arrive" (added
by `frontend-three-pane-layout`); `entry-table` had no equivalent for row selection, which is exactly
why the same defect could ship in the same change that fixed it next door. `entry-table` now has
"Rows stay selectable while entries arrive", amended directly into the accepted spec rather than
routed through a change proposal — the code already behaves this way, so the spec was behind the
code, not ahead of it. It names the permitted rewrites (entry, selected state, active search) and
forbids the wholesale rebuild, so the next person to touch `renderVisibleRows` has the constraint in
front of them. `openspec validate --specs --strict`: 20 passed.

## Ideas for later

- Give `looq-detection`'s collapsed summary something better than "detecting…" when a `#format=`
  override skipped detection entirely (`crates/looq-wasm/src/lib.rs`: detection is `null` in that
  case). Pre-existing — the old panel said "Detecting format..." forever too — but a summary line
  that is now the *only* thing visible when collapsed makes it more noticeable.
- Resizable rail/detail panes, deliberately deferred by `frontend-three-pane-layout`'s Non-Goals
  rather than half-built; the widths are fixed at 18rem/22rem today.
- Re-render the timeline on window resize (its `uPlot` canvas width is chosen once, at draw time),
  now that the layout is width-responsive; out of scope for the change that introduced the layout.
- A disk-backed or larger in-page benchmark harness (median/stddev over many runs,
  automated regression check) belongs with `log-parsing-core`, once there's a real
  parser worth protecting from regressions.
- Named IANA timezone support (`chrono-tz` or an equivalent smaller data source) —
  see the NEEDS HUMAN DECISION entry above.
- Flattened nested-JSON fields (`http.status`) instead of D8's raw-JSON-text
  fields, once separators/array-index/depth-cap/dotted-key-collision decisions are
  made — explicitly deferred scope, not a bug.
- Anywhere-in-line timestamp matching for plain text (currently leading-only, per
  design.md's Open Questions) if the leading-only heuristic proves too strict in
  practice.
- `wasm-bindgen-test` in CI for `looq-wasm`/`dto.rs` — see the NEEDS HUMAN
  DECISION entry above; would catch a DTO shape regression before it reached the
  E2E/Playwright layer.
- Investigate *why* one large `feed()` call is slower than several small ones for
  the same total bytes (chunk-size measurement above) — likely
  `serde-wasm-bindgen` array serialization or GC behavior, not confirmed by
  profiling in this change.
- A dedicated "stream restarted" row/UI marker for the backend-process-restart
  case (`live-tail`'s NEEDS HUMAN DECISION) — currently silent-but-correct (no
  data lost, just no visual note), unlike every other data-loss path in this
  project.
- `wasm-bindgen-test`-style coverage for `web/src/live-tail.ts`'s
  `LiveTailSession`/`StreamParserSession` — this change's correctness was
  verified against a real running binary via Playwright (deliberately, since two
  of the three real bugs found only reproduced against real timing), but there is
  still no fast, deterministic unit-level regression test for the gap-detection/
  dedup/reconnect state machine; a future change touching this logic would only
  find a regression by re-running the same manual E2E flows.
- Binary WebSocket framing for `/ws`, per D1's stated revisit trigger — only
  worth it if envelope (de)serialization ever shows up in a profile at real
  throughput; not evaluated in this change beyond the measurement already
  recorded (81ms/~12.7MB at 100k lines in release).
- Chip negation (`level!=DEBUG`) — `filtering-and-search`'s task 8.1, deferred as a query-language
  expansion the change's own Non-Goals rule out; would need a hash grammar change and an
  undesigned UI affordance.
- Raw-source-line search (in addition to parsed fields) — `filtering-and-search`'s task 8.1,
  deferred because it needs `Entry`/the WASM DTO to carry the original line text, which they
  don't; would close the narrow gap where a JSON entry with no discoverable `message` key can't be
  found by search even though its raw line has content.
- Investigate *why* `<mark>` specifically costs ~8x a plain `<span>` for the same `innerHTML`
  content in this environment (`filtering-and-search`, `docs/devlog.md`'s 2026-08-13 entry) — fixed
  by switching elements, but the root browser-internal mechanism (candidate: `<mark>` hooking
  find-in-page/selection styling) was not confirmed, only worked around.
- A UI-level warning (not just a devlog note) when a field's distinct-value count approaches the
  parser's cardinality cap, so a user filtering on a near-cap field understands the value list may
  be incomplete before they hit it — `filtering-and-search` only fixed the chip-count DOM-size
  problem (`CHIP_LIST_MAX_VALUES`), not the underlying "cap reached silently" case for a field that
  still renders as a normal value list.
- Rework the virtual-scrolled table's row positioning off inline `element.style.transform`/
  `.style.height` and onto generated stylesheet rules (`CSSStyleSheet.insertRule` or similar), so
  the CSP's `style-src` could drop back to bare `'self'` without `'unsafe-inline'`
  (`release-hardening`) — not attempted in this change, a larger, riskier rework of code already
  measured and tuned (`timeline-and-table`'s 50k-row/~17ms-per-frame result) for a narrow security
  gain (no code execution is possible via CSS injection alone, and this app never renders
  attacker-controlled CSS).
- A CI/build check that would have caught this change's own "`cargo build` served stale
  `include_bytes!`-embedded assets after `scripts/build-frontend.sh` changed their content"
  bug automatically (e.g. hash the running server's `/assets/*` responses against the source
  files as part of the existing `frontend-artifact-staleness` CI job) — worked around by having
  `build-frontend.sh` touch `assets.rs`, which prevents recurrence, but doesn't add a check that
  would catch a *different* path to the same failure mode.
- An actual container/VM fresh-machine run with zero dev-environment assumptions, still owed
  before 0.1.0 ships (`release-hardening`'s own NEEDS HUMAN DECISION) — this sandbox had no
  Docker/VM available, so the closest achievable proxy (running the compiled release binary
  directly, not via `cargo run`) was used instead.
- Linux x86_64 release binary and its size against TDR §5 — this sandbox (macOS arm64, no
  cross-linker toolchain: no `zig`, `x86_64-unknown-linux-gnu-gcc`, `musl-gcc`, `cross`, or
  Docker) could only produce and measure a macOS arm64 build (`release-hardening`).
