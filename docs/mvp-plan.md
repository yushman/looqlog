# MVP plan

**Build length:** 6 weeks / 30 working days (matches TDR §15 milestone estimate).
**Feature freeze:** day 25 (end of week 5) · **Release + buffer:** days 26–30.
**Golden path:** `looq app.log` → browser opens → file parsed in WASM (JSON /
logfmt / plain auto-detected) → timeline + virtual table appear (PRD Flow 1).
**Riskiest assumption:** WASM parsing throughput can hit TDR §11 targets (<200ms/MB
JSON parse, <50ms filter on 10k lines) — tested by day 4, not after a full parser is
built.

> Carried over from TDR §15: this 6-week estimate assumes full-time, dedicated work.
> WASM-core + JS-interop and UI (timeline / virtual scroll / filters) have each
> independently taken 2+ weeks on comparable projects — 6 weeks total is the
> aggressive case, not the conservative one. Treat slippage past day 30 as expected,
> not exceptional.

## Why the day order deviates from TDR's M1→M2→M3→M4 sequence

TDR §15 sequences work by layer: finish the whole backend skeleton (M1, one week)
before starting the WASM core (M2). That is the right grouping for scope, but the
riskiest assumption in the whole project — can WASM parsing actually hit the latency
targets that the entire "privacy-first, client-side parsing" architecture (ADR-0002)
depends on — would not get tested until week 2 under that ordering. This plan pulls a
minimal WASM parse-and-measure spike into days 3–4, inside what TDR would call M1/M2
overlap, and returns to finishing the rest of M1 afterward. Every task below is still
labeled with its TDR milestone so the M1–M4 breakdown stays traceable.

## Scope covered (from PRD §6)

**Must (P0):** F-1 open file, F-2 stdin pipe, F-3 auto-detect JSON/logfmt/plain, F-4
timeline + drag range, F-5 virtual-scrolled table, F-6 field filters, F-7 full-text
search, F-9 live tail (WebSocket), F-10 WASM parsing core, F-11 TS UI layer, F-12
single-binary distribution, F-13 File API upload.

**Should (P1):** F-8 light/dark theme, F-14 auto-open browser, F-15 URL hash state.

**Won't (this cycle):** F-16 MCP server mode and everything in PRD §6 P2/P3 — MCP is
architecturally prepared for (ADR-0005) but not implemented; see PRD §8 Out of Scope.

---

## Week 1 — Backend skeleton + risk spike (M1)

### Day 1 — Cargo workspace, axum hello world
- [ ] `cargo new` workspace: `looq` (bin) + `looq-core` (lib, empty placeholder) (~2h)
- [ ] axum server binds `127.0.0.1:7891`, one route returns `200 OK` (~2h)
**Done when:** `curl http://127.0.0.1:7891` returns 200.

### Day 2 — CLI args + static embed scaffold
- [ ] `clap`-based argv: `--port`, `--host`, `--open`, `--no-browser`, `--stdin`,
  `--max-lines`, `--version`, `--help` per TDR §6 (~3h)
- [ ] `include_bytes!` scaffold serving a placeholder `index.html` (~2h)
**Done when:** `looq --help` prints every flag from TDR §6; `looq anyfile.log`
serves the placeholder page.

### Day 3 — Walking skeleton ← checkpoint
- [ ] Minimal core-wasm: one hardcoded JSON-lines parse function, `wasm-bindgen`,
  built with `wasm-pack`, embedded into the binary (~4h)
- [ ] Inline JS: File API reads a local fixture, calls WASM `parse()`, logs entry
  count to console (~2h)
**Done when:** `looq tests/fixtures/sample.jsonl` starts the server and the page shows
`tests/fixtures/sample.jsonl` as a hint; after the user picks that file through the
page's file picker (per ADR-0007 — no browser API can auto-open a server-supplied path
without a user gesture), the console shows an entry count matching the fixture's line
count — with the backend process never reading the file (verify via empty Network tab,
US-6).
**If not met:** cut auto-detect entirely for now (hardcode `format=json`, already the
plan for this day) / if File API↔WASM plumbing itself is broken beyond a day of
debugging, stop and re-examine ADR-0002 (WASM-in-browser architecture) before building
anything further on top of it.

### Day 4 — Riskiest-assumption benchmark ← checkpoint
- [ ] Parse a realistic ~1MB JSON-lines fixture through the day-3 WASM core, measure
  wall-clock in browser devtools (~2h)
- [ ] Write the number down against the <200ms/MB target from TDR §11 (~1h)
**Done when:** a measured number exists, compared explicitly against target.
**If not met:** timebox one day (day 4.5, folds into day 6) for a columnar-layout /
SIMD pass; if still not met afterward, downgrade the MVP performance target explicitly
and document it — do not silently keep building against an unverified number.

### Day 5 — M1 completion: stdin + WebSocket
- [ ] `/ws` WebSocket echo endpoint, stdin read in its own tokio task (~3h)
- [ ] Graceful shutdown on Ctrl+C, `--stdin` vs file-path detection wired end to end (~2h)
**Done when:** `echo hi | looq --stdin` opens a socket that echoes `hi` to a
`wscat` client; M1 backend skeleton is complete.

**Week 1 done when:** M1 complete + riskiest-assumption number recorded (day 4).

---

## Week 2 — Core parsers (M2)

### Day 6 — JSON Lines + logfmt parsers
- [ ] JSON Lines parser in `looq-core`, unit tests (~3h)
- [ ] logfmt (`key=value`) parser, unit tests (~2h)
**Done when:** `cargo test -p looq-core` and `wasm-pack test` both green for both
formats.

### Day 7 — Plain-text fallback + auto-detect
- [ ] Plain-text fallback parser (regex/heuristic) (~2h)
- [ ] Auto-detect: check first 100 non-empty lines in TDR §8 MVP priority order
  (JSON → logfmt → plain) (~3h)
**Done when:** three fixture files (one per format, no `#format=` override) each
auto-detect correctly.

### Day 8 — Entry/Index structures + field extraction
- [ ] `Entry` struct: `timestamp` (chrono), `level` (regex over
  ERROR/WARN/INFO/DEBUG/TRACE/FATAL), `message` (~3h)
- [ ] Arbitrary field extraction from JSON root / logfmt pairs, becomes filter list (~2h)
**Done when:** parsing a fixture with custom fields (e.g. `service=api`) exposes
`service` as an extractable field in a unit test.

### Day 9 — JS interop
- [ ] `serde-wasm-bindgen` for typed JS↔WASM exchange, replace ad hoc console-log
  plumbing from day 3 (~3h)
- [ ] `comlink` wrapper around the WASM worker boundary (~2h)
**Done when:** the day-3 skeleton page now calls the real multi-format parser (not
the hardcoded JSON-only stub) and gets typed entries back.

### Day 10 — First demoable version ← checkpoint
- [ ] Wire day-7's auto-detect into the page so any of the three fixture formats
  works without a hardcoded format (~2h)
- [ ] Rough unstyled table dump of parsed entries in the page, for visual proof (~2h)
**Done when:** three different fixture files (JSON, logfmt, plain), opened one at a
time, each parse correctly end-to-end with zero backend file reads.

---

## Week 3 — Core hardening + UI shell start (M2 wrap, M3 start)

### Day 11 — Live tail data path
- [ ] stdin line → `/ws` → WASM parse path wired for real (not just echo) (~3h)
- [ ] Bounded ring buffer on backend (`--max-lines`, default 100k) + snapshot sent
  on connect, per ADR-0004 (~3h)
**Done when:** `myapp | looq` then opening the browser a few seconds later shows
the lines emitted before connection, then continues live.

### Day 12 — Backpressure
- [ ] Bounded channel backend→client, drop-oldest under load (~2h)
- [ ] "gap" event emitted when messages are dropped (data only, no UI yet) (~2h)
**Done when:** a synthetic fast-producer/slow-consumer test shows the stdin reader
never blocks and a gap event fires.

### Day 13 — Parser robustness
- [ ] Malformed-line handling: skip + warning (per PRD Open Question #2 resolution) (~2h)
- [ ] Encoding fallback: UTF-8 default, latin-1 fallback (~2h)
**Done when:** a fixture with one deliberately malformed line and one non-UTF-8 line
parses the rest correctly and reports both issues instead of crashing.

### Day 14 — TS app shell
- [ ] Vite project scaffold, TypeScript strict mode (~2h)
- [ ] Web Components skeleton replaces the day-9 ad hoc HTML/JS shell (no styling) (~3h)
**Done when:** the real app shell loads a fixture through the same parse path as
day 10, rendered via Web Components instead of inline script.

### Day 15 — Bare timeline
- [ ] `uPlot` integrated, count-per-time-bucket histogram from parsed entries (~3h)
- [ ] No interaction yet — static render only (~1h)
**Done when:** opening a fixture shows a histogram whose bucket counts match a
manual count from the fixture file.

**Week 3 done when:** M2 fully complete (parsers, WASM core, tests green) and M3 has
an app shell with a non-interactive timeline.

---

## Week 4 — Interactive UI (M3)

### Day 16 — Virtual-scrolled table
- [ ] Table component: timestamp / level / message columns, virtual scroll (~4h)
**Done when:** a 50k-line fixture scrolls smoothly (no full-list DOM render).

### Day 17 — Timeline drag → range filter
- [ ] Drag-select on the day-15 timeline sets a time range (~2h)
- [ ] Range filters the table (~2h)
**Done when:** dragging a region on the timeline visibly narrows the table to that
time window.

### Day 18 — Field filter chips
- [ ] Chips for `level` and extracted fields (e.g. `service`) (~2h)
- [ ] AND-combination of active chips + range filter (~2h)
**Done when:** PRD Flow 3 steps 1–3 reproduce manually against a real fixture.

### Day 19 — Full-text search
- [ ] Search input, case-insensitive substring match, highlighting (~2h)
- [ ] `re:` prefix triggers regex search (~2h)
**Done when:** PRD Flow 4 (search, regex, Esc-to-clear) reproduces manually.

### Day 20 — Golden path checkpoint
- [ ] Run PRD Flow 1 + Flow 3 + Flow 4 back to back on one real multi-thousand-line
  fixture (~3h)
- [ ] Fix anything broken found in the combined run (~1h)
**Done when:** timeline + table + filters + search all work together without
console errors on a real fixture, hands-off of any hardcoded fixture-specific logic.

---

## Week 5 — Live UI, state, freeze (M3 wrap)

### Day 21 — Live tail UI
- [ ] `LIVE` indicator + lines/sec counter in top bar (~2h)
- [ ] Autoscroll with throttling, gap indicator surfaced from day-12 backend event (~2h)
**Done when:** PRD Flow 2 (`myapp | looq --open`) shows the live indicator,
counter, and a visible gap marker under a synthetic backpressure test.

### Day 22 — URL hash state (F-15)
- [ ] `#range=`, `#filter=` written on filter change (~2h)
- [ ] Loading a URL with a hash pre-applies filters (~2h)
**Done when:** copying the current URL into a fresh tab reproduces the same filtered
view (PRD Flow 3 step 4).

### Day 23 — CLI polish (F-14)
- [ ] `--open` auto-opens the default browser, `--no-browser` suppresses it (~2h)
- [ ] `--port 0` allocates a random free port (~1h)
**Done when:** `looq --open file.log` opens a browser tab without manual action;
`--port 0` prints a different port each run.

### Day 24 — Performance pass
- [ ] Measure filter latency at 10k lines, live-tail end-to-end latency against TDR
  §11 targets (~2h)
- [ ] Fix the worst regression found (~2h)
**Done when:** both numbers are written down; if either misses target, the fix (or
the decision to accept and document the miss) is made today, not deferred.

### Day 25 — Feature freeze ← checkpoint
- [ ] Full pass over every P0 item (F-1…F-7, F-9…F-13) against the golden path (~3h)
- [ ] Full pass over every P1 item (F-8 theme, F-14 auto-open, F-15 URL hash) —
  functional even if unpolished (~1h)
**Done when:** every Must from PRD §6 works on the golden path; nothing new gets
added to scope after this point.

---

## Week 6 — Polish, release, buffer (M4)

### Day 26 — Theme + error states
- [ ] Light/Dark theme toggle (F-8) (~2h)
- [ ] User-facing error states: bad file, empty file, unsupported format — loud,
  not silent (~2h)
**Done when:** opening a corrupt/empty/binary file shows a specific message instead
of a blank screen or console-only error.

### Day 27 — Security pass
- [ ] CSP header `default-src 'self'` on all responses (~1h)
- [ ] Origin check + one-time token for `/ws` (TDR §13) (~2h)
- [ ] Mandatory warning when `--host` ≠ `127.0.0.1` (ADR-0003) (~1h)
**Done when:** a manual cross-origin WebSocket connection attempt from another
origin is rejected; `--host 0.0.0.0` prints the warning.

### Day 28 — Docs + release build
- [ ] `README.md` + `README.ru.md`: install steps, the three PRD flows as examples (~3h)
- [ ] Release build for Linux x86_64 (at minimum); binary size check against TDR §5
  budget (~1h)
**Done when:** both READMEs exist and are in sync; a release binary exists and its
size is recorded.

### Day 29 — Fresh-clone / fresh-binary check
- [ ] Download the built binary on a clean machine or VM, run Flow 1, Flow 2, Flow 3
  end to end (~3h)
- [ ] Fix anything that only breaks outside the dev machine (~1h)
**Done when:** all three flows work from a binary that was never touched by the dev
environment.

### Day 30 — Buffer
- [ ] Absorb whatever slipped from days 1–29.
- [ ] If nothing slipped: record the demo video referenced in PRD US-7 (< 2 minutes).
**Done when:** either the slip is closed, or (if none) a demo video exists.

---

## Feasibility note

This plan does not compress TDR §15's own 6-week estimate — it keeps the same total
and the same caveat (aggressive, not conservative). What it changes is order: the
riskiest assumption (WASM parse throughput, ADR-0002) is measured on day 4 instead of
implicitly assumed until M2 is "done" in week 2–3. If day 4's number misses target by a
wide margin, that is the point to cut scope (e.g. drop plain-text auto-detect
heuristics to P1, keep JSON/logfmt only) rather than discovering it in week 5 during
the performance pass, with four weeks of UI already built on top of an unverified
core.
