## 1. Core: entry shape and parser state

- [x] 1.1 Add `continuation_of: Option<usize>` to `Entry` in `crates/looq-core/src/entry.rs`, documenting that it holds the **chain root's** ordinal (design D2), not the predecessor's
- [x] 1.2 Add `DiagnosticReason::ChainTruncated` with its `label()` arm in `crates/looq-core/src/diagnostics.rs`
- [x] 1.3 Add a `ChainState` struct in `crates/looq-core/src/parsers/plain.rs` (or a new sibling module) holding root ordinal, owned logcat identity (`pid`/`tid`/`tag` strings + level), running brace depth and member count — owned, not borrowed, because `LogcatRecord` borrows from the line (design D10)
- [x] 1.4 Thread `Option<ChainState>` through `Parser`, clearing it on a blank line in `consume_line` (`parser.rs:147`, where blanks already return early) and passing it into the plain-text path
- [x] 1.5 Set `continuation_of` in `Parser::finish_entry` from whatever the recognizers decided, and confirm `entries_emitted`/`blank_lines`/`total_lines` accounting is untouched
- [x] 1.6 Define the chain cap constant (1,000 continuation lines, design D8) next to `DEFAULT_DIAGNOSTIC_CAP` with the HotSpot 1,024-frame rationale in its doc comment

## 2. Core: recognizers

- [x] 2.1 R1 — prefix-less frame markers (`at `, `Caused by:`, `... N more`, `Suppressed:`, `Traceback (most recent call last):`, `File "…", line N`) as byte-prefix scans over ASCII, preserving char boundaries; no `regex` crate
- [x] 2.2 R1 — exception-header marker: leading identifier whose last dotted segment ends in `Exception`, `Error` or `Throwable`, optional `: message`. Without it the chain in `tests/fixtures/stack-trace.log` breaks at the header (design D3)
- [x] 2.3 R2 — logcat identity: compare `(pid, tid, level, tag)` against the chain root, **excluding the timestamp** (design D4), plus a required continuation signal in the message (leading whitespace, an R1 marker, or root depth > 0)
- [x] 2.4 R3 — brace-depth tracking counting only `{` and `}`, never `[` or `]` (design D5); open a chain when a root's message ends at depth > 0, close it when depth returns to 0
- [x] 2.5 Enforce the shared guards: chain root must have a recognised prefix, and the whole mechanism is gated on `Format::Plain` only (design D6)
- [x] 2.6 Enforce the cap: close the chain, start a fresh unlinked entry, record `ChainTruncated` naming the root's line number — never silently
- [x] 2.7 Confirm `dispatch_payload` is left unmodified (design D7) and `payload_that_fails_to_parse_stays_message_text` still passes unchanged

## 3. Core: fixtures and tests

- [x] 3.1 Extend `crates/looq-core/tests/fixtures/stack-trace.log` coverage: `stack_trace_becomes_one_entry_per_line_no_diagnostics` must still see 5 entries and 0 diagnostics, now asserting lines 2–5 link to ordinal 1
- [x] 3.2 New fixture `continuation-python.log` — timestamped line, `Traceback (most recent call last):`, `  File "app.py", line 12, in handler`, frame body, `ValueError: …`
- [x] 3.3 New fixture `continuation-nested-cause.log` — root, frames, `Caused by:`, more frames, `... 14 more`; assert every member links to the **root**, not to the `Caused by:` line
- [x] 3.4 New fixture `continuation-logcat.log` drawn from the measured bugreport: the `JAZZ/WebSocketClientImpl` `SocketTimeoutException` block, plus the `W System.err` block whose frame drifts from `.983` to `.984` (design D4)
- [x] 3.5 New fixture `continuation-logcat-noise.log` — consecutive `NetworkSensitiveLogger: *` lines sharing `(pid, tid, level, tag)`; assert none are linked (the 946-line false-positive case)
- [x] 3.6 New fixture `continuation-json-payload.log` — the `BTB_UPDATER/ConfigRepositoryImpl: response body = {` block; assert grouping happens and that no nested key became a field (design D7)
- [x] 3.7 New fixture `continuation-dump-text.log` — a VM-TRACES-shaped run of unprefixed `at …`, `| held mutexes=` and `native: #NN pc …` lines with no timestamped line above; assert **zero** links
- [x] 3.8 Test: a blank line closes an open chain
- [x] 3.9 Test: a chain exceeding the cap is closed, the overflow entry is unlinked, and exactly one `ChainTruncated` diagnostic is recorded; a chain at exactly the cap records none
- [x] 3.10 Test: a truncated JSON Lines record is reported malformed and does not absorb the next record (design D6); the same input with `Format::Plain` forced does chain
- [x] 3.11 Test: feeding the continuation fixtures one line at a time produces byte-identical entries and links to whole-input feeding, and no entry is withheld pending a later line
- [x] 3.12 Test: an ANSI escape (`ESC[7m`) in a message opens no chain
- [x] 3.13 Run `cargo bench` on `looq-core`'s hot paths before and after; record the numbers and the command in `docs/devlog.md` (CLAUDE.md testing rules)

## 4. WASM bridge

- [x] 4.1 Add `continuation_of` → `continuationOf` to `EntryDto` in `crates/looq-wasm/src/dto.rs`, verifying it serializes as `number | null` and never `undefined` under the existing `serialize_missing_as_null(true)` serializer
- [x] 4.2 Mirror the field in `web/src/wasm-types.ts` with a doc comment matching the Rust one
- [x] 4.3 Confirm `cargo build -p looq-core --target wasm32-unknown-unknown` still passes the target-agnostic CI job (no `wasm-bindgen`, `web-sys` or `std::fs` reached `looq-core`)

## 5. Frontend: index, filtering, search

- [x] 5.1 Teach `web/src/entry-index.ts` to track chain membership (root → members, member → root) alongside the existing ordinal map
- [x] 5.2 Exclude continuation entries from `bucketCounts` and `bucketCountsUnfiltered` (design D9) while leaving `robustSpan`/`fullSpan` semantics alone
- [x] 5.3 Evaluate the active predicate against the chain root and show/hide the chain as a unit; a member match pulls in its root
- [x] 5.4 Handle the orphaned-member case: after `evictFront` removes a root, remaining members render and filter as ordinary standalone entries — no failed lookup, no dropped rows
- [x] 5.5 Update `web/src/predicate.ts` for chain-aware evaluation and extend `web/src/predicate.test.ts` and `web/src/entry-index.test.ts` to cover 5.2–5.4
- [x] 5.6 Make search surface the chain root with the matching member highlighted, for both substring and regex queries, and once per chain when several members match

## 6. Frontend: table

- [x] 6.1 Render a chain root with an expand/collapse control and its line count in `web/src/components/looq-entry-table.ts`; chains start collapsed
- [x] 6.2 Render members indented beneath an expanded root, in input order
- [x] 6.3 Keep virtual-scroll row accounting correct across expand/collapse — no skipped or duplicated rows, positions still driven by CSS custom properties rather than inline style attributes (existing `entry-table` requirement)
- [x] 6.4 Preserve collapse state across row recycling when a chain scrolls out of view and back
- [x] 6.5 Auto-expand a collapsed chain whose member a search matched
- [x] 6.6 Style the group control and indentation in `web/src/style.css` against `docs/ui.png`; extend `web/src/entry-table-styles.test.ts`

## 7. Verification and artifacts

- [x] 7.1 Run `./scripts/build-frontend.sh` and commit the rebuilt `crates/looq/assets/` — the macOS CI job fails on vendored-artifact drift otherwise
- [x] 7.2 Record the new `core.wasm` size against the 210,165-byte baseline and the ~300,000-byte TDR §5 budget in `docs/devlog.md`; if the budget is at risk, stop and report rather than trimming behavior silently
- [x] 7.3 Load `.playwright-mcp/bugreport.txt` in a running binary and confirm the `JAZZ/WebSocketClientImpl` trace collapses to one row, the timeline peak at `12:56:57` drops from ~60 to 1, and the VM TRACES section is unchanged from today
- [x] 7.4 Verify live mode: pipe a log containing a stack trace into stdin and confirm each line appears as it arrives, with no entry delayed waiting for the next line
- [x] 7.5 Update `README.md` and `README.ru.md` in the same commit — multi-line events are now collapsible, and the timeline counts events rather than lines
- [x] 7.6 Mark PRD §14 Open Question #3 resolved, in the style used for #2
- [x] 7.7 Append a `docs/devlog.md` entry: what shipped, the measured wasm size and bench numbers with the commands that produced them, and the D7 limitation (payloads grouped, not field-extracted)
- [x] 7.8 `openspec validate multiline-entry-continuations --strict` passes, then run `/opsx:archive`
