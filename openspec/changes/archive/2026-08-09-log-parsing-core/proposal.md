## Why

After `bootstrap-cli-and-wasm-skeleton` the project has a walking skeleton with one
hardcoded JSON Lines counter. Everything the product actually sells — timeline, filters,
search — reads from structured entries that do not exist yet. This change builds them in
`looq-core`: the three MVP formats (TDR §8), auto-detection between them, timestamp / level
/ arbitrary field extraction, and the robustness behaviour that decides whether a bad line
is a visible warning or a silent hole in the data.

It is grouped this way because days 6, 7, 8 and 13 of `docs/mvp-plan.md` all write to the
same crate and the same spec surface. Day 13 (malformed lines, encoding) sits three days
later in the plan but belongs to the parser's contract, not to a separate hardening phase —
deciding what a parser does with garbage after it already has three formats means retrofitting
error paths into code written assuming clean input.

Two behaviours in scope here are on the project's silent-failure list (CLAUDE.md): a skipped
malformed line that nobody reports, and an auto-detect that quietly picks the wrong format.
Both are specified as loud, observable outcomes rather than left to implementation taste.

## What Changes

- Incremental parse API in `looq-core`: bytes in, entries plus diagnostics out, usable both
  for a file read in chunks and for stdin arriving line by line — one code path for both
  modes rather than two that can drift.
- JSON Lines parser, logfmt parser, plain-text fallback parser, each with unit tests.
- Format auto-detection over a sample of the first non-empty lines, in TDR §8 priority order
  (JSON → logfmt → plain), reporting which format was chosen and how confidently, plus an
  explicit override path for callers.
- `Entry`: timestamp, level, message, and arbitrary extracted fields.
- Timestamp parsing with a stated timezone policy for values that carry no offset, and a
  defined behaviour for entries whose timestamp cannot be parsed at all — kept, not dropped.
- Level detection with a fixed alias table, from a dedicated field where the format has one
  and from the message text otherwise.
- Field extraction with a per-field distinct-value cap, so a high-cardinality field cannot
  turn into thousands of filter chips downstream.
- Malformed-line handling: skip the line, keep parsing, and record a diagnostic that names
  the line number and the reason; diagnostics are bounded and aggregated so a broken file
  produces a usable summary rather than a million warnings.
- Encoding: UTF-8 by default with a latin-1 fallback, decoded inside the core crate so both
  modes share one implementation.
- Benchmarks for the parse hot path, and a re-measurement of the skeleton's throughput number
  now that a real parser has replaced the stub.

Not in this change: wiring any of it into the browser page (that is `browser-app-shell`,
mvp-plan days 9, 10, 14), typed `serde-wasm-bindgen` interop, `comlink`, any timestamp index
or sorting, any UI surface for the diagnostics, syslog / Apache / Docker formats (TDR §8 P1),
multi-line stack-trace aggregation (PRD §14 Q3, P2), and the `#format=` / `#pattern=` URL hash
plumbing (that is `filtering-and-search`; this change only exposes the override parameter the
hash will eventually set).

## Capabilities

### New Capabilities

- `log-parsing`: the incremental parse API, the three MVP format parsers, encoding handling,
  and what happens to lines that do not parse.
- `format-detection`: choosing a format from a sample, reporting the choice and its
  confidence, and honouring an explicit override.
- `field-extraction`: turning a parsed line into an `Entry` — timestamp, level, message,
  arbitrary fields — and the field inventory that filters will later be built from.

### Modified Capabilities

None. `bootstrap-cli-and-wasm-skeleton` is not archived yet, so `openspec/specs/` is still
empty; the provisional single-format entry point it defines under `browser-file-loading` gets
replaced by `browser-app-shell`, not here.

## Impact

- `crates/looq-core/`: parser modules, `Entry`, diagnostics, detection, benches. This is the
  crate ADR-0005 requires to stay target-agnostic — no `wasm-bindgen`, no `web-sys`, no
  `std::fs`, so every parser input arrives as bytes from a caller.
- Dependencies added to `looq-core`: `serde_json`, `regex`, `chrono`. Each lands in the WASM
  bundle, so the size effect on `core.wasm` is measured, not assumed.
- `tests/fixtures/`: one fixture per format, one with a deliberately malformed line, one
  non-UTF-8, one with custom fields, one high-cardinality.
- Constrains later changes: the parse API shape decided here is what `browser-app-shell` wraps
  and `live-tail` feeds; the field inventory shape is what `filtering-and-search` builds chips
  from; the timezone policy is what the timeline reads.
- Resolves TDR §16 open questions on invalid-line behaviour and timezone handling, and PRD
  §14 Q2; both get written back into those documents.
