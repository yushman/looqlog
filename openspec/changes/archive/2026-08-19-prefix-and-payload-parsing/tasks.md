## 1. Baseline measurements before touching anything

- [x] 1.1 Record the current `core.wasm` size (`ls -la crates/looq/assets/wasm/core.wasm`,
  expected 194,350 B) — this change is measured against it, not against the TDR budget alone
  — measured 194,350 B (`wc -c < crates/looq/assets/wasm/core.wasm`)
- [x] 1.2 Run `cargo bench -p looq-core` and record the current per-format throughput,
  especially plain text, as the regression baseline — json 74.4 ms/MB (12.82 MiB/s),
  logfmt 100.4 ms/MB (9.50 MiB/s), plain 120.1 ms/MB (7.94 MiB/s)
- [x] 1.3 Confirm the existing suite is green (`cargo test -p looq-core`, `cargo fmt --check`,
  `cargo clippy --all-targets -- -D warnings`) so later failures are attributable — 22 tests
  pass, fmt and clippy clean

## 2. Timestamp scanners (`crates/looq-core/src/timestamp.rs`)

- [x] 2.1 Introduce a `TimestampShape` enum naming the recognised shapes (ISO, syslog 3164,
  klog, Apache/CLF, slash-date, epoch) so detection and the sticky choice can refer to one
- [x] 2.2 Hand-roll the syslog RFC 3164 scanner (`Aug  8 17:42:01`, note the double space for
  single-digit days) — byte-by-byte, no `regex`, every branch advancing only over matched ASCII
  so returned slices stay on `str` char boundaries
- [x] 2.3 Hand-roll the klog scanner (`0808 17:42:01.123456`, optionally preceded by the
  severity letter which task 4 consumes)
- [x] 2.4 Hand-roll the Apache/CLF scanner (`08/Aug/2026:17:42:01 +0000`), including the
  explicit offset
- [x] 2.5 Hand-roll the slash-date scanner (`2026/08/08 17:42:01`)
- [x] 2.6 Accept a leading integer epoch value, reusing the existing `parse_epoch` magnitude
  rules rather than duplicating them
- [x] 2.7 Add the head-window search: try token starts (offset 0, or preceded by whitespace,
  `[`, `(`, `<`) within the first 64 bytes; require a full pattern match; first match wins
  (design D2)
- [x] 2.8 Unit-test each shape, including the negatives: a date-like number past the window is
  not a timestamp, and a partial match is not accepted

## 3. Inferred year (`timestamp.rs`, `entry.rs`)

- [x] 3.1 Extend the parse API to take a caller-supplied reference instant; do NOT read the
  system clock inside `looq-core` (ADR-0005 — the crate must stay target-agnostic)
- [x] 3.2 Infer the year for year-less shapes: the reference year, stepped back one when that
  would date the entry in the future (design D4)
- [x] 3.3 Add `timestamp_year_inferred` to `Entry`, mirroring how `timestamp_used_default_tz`
  is already carried
- [x] 3.4 Test the December-reference/January-line boundary in both directions

## 4. Level by position (`crates/looq-core/src/level.rs`)

- [x] 4.1 Add a positional level matcher: bare, `[INFO]`, `<INFO>`, `INFO:`, and single letters
  `I`/`D`/`W`/`E`/`F`/`V`
- [x] 4.2 Parse a syslog priority (`<130>`) and map its severity onto the existing six-level
  table (`emerg`/`alert`/`crit`→FATAL, `err`→ERROR, `warning`→WARN, `notice`/`info`→INFO,
  `debug`→DEBUG). Do not widen the `Level` enum (design D5)
- [x] 4.3 Wire the precedence: dedicated field, then positional token, then the existing
  whole-message scan. Single letters must be reachable ONLY from the positional path
- [x] 4.4 Test the behavior change explicitly: `2026-08-08T17:42:01Z INFO retrying after ERROR
  response` must now yield INFO, and `app: ERROR something` (no prefix) must still yield ERROR

## 5. Prefix + payload in the plain parser (`crates/looq-core/src/parsers/plain.rs`)

- [x] 5.1 Split each line into (recognised prefix, remainder) and make the remainder the
  message, keeping today's whole-line behavior when no prefix is recognised
- [x] 5.2 Dispatch a remainder starting with `{` to `json::parse_line`, and one carrying at
  least two `key=value` pairs to `logfmt::parse_line`; one level of nesting only (design D6)
- [x] 5.3 A payload that fails to parse stays message text and emits NO diagnostic — a
  plain-text file must never produce malformed-line diagnostics
- [x] 5.4 Resolve conflicts payload-first, and keep a differing prefix timestamp as a field
  (design D7)
- [x] 5.5 Test the two-pair threshold from both sides: prose containing one `foo=bar` yields no
  fields; a real logfmt payload does

## 6. Docker wrapper (`crates/looq-core/src/parsers/json.rs`)

- [x] 6.1 Detect an object whose members are exactly `log`, `stream`, `time` — exact set, not
  "contains `log`" (design D8)
- [x] 6.2 Re-parse the `log` member as a line, run it through the prefix/payload path, fall
  back to the wrapper's `time` when the inner line has no timestamp, keep `stream` as a field
- [x] 6.3 Test that `{"log":"hi","level":"info","service":"api"}` is NOT unwrapped

## 7. Detection (`crates/looq-core/src/detect.rs`)

- [x] 7.1 Measure the fraction of sampled lines whose prefix yields a timestamp, and report
  plain text as a threshold match when it clears 80%, as fallback otherwise
- [x] 7.2 Record the winning `TimestampShape` and head offset in the detection result
- [x] 7.3 Implement the sticky choice in the parser: try the recorded shape/offset first, fall
  back to the full sweep on a miss, re-select after repeated misses (design D3) — the recorded
  *shape* is tried first at every candidate offset; offsets are still swept in ascending order,
  because jumping straight to the recorded offset would change which match wins on a line that
  also carries an earlier one, and D3/the spec require result-neutrality
- [x] 7.4 Test that a mixed-shape input produces identical entries with and without the sticky
  path — the optimisation must not change results

## 8. Boundary and UI

- [x] 8.1 Wire the reference instant across the boundary — `ParserHandle::new` takes it and
  `worker.ts` passes `Date.now()`, reaching `Parser::with_context(..., ParseContext::new(tz)
  .with_reference(now))`. Found while verifying groups 1–7: without it `ParseContext` defaults
  to `None`, year-less shapes are not recognised at all, and syslog/klog files still render an
  empty timeline in the browser — the change would ship inert for two of the formats it exists
  to support. `looq-core` must still never read the clock itself (ADR-0005)
- [x] 8.2 Carry `timestamp_year_inferred` and detection's new `timestamp_shape` /
  `timestamp_offset` fields through `crates/looq-wasm/src/dto.rs` and `web/src/wasm-types.ts`
  (note: `DetectionOutcome` gained no new variant — see design D9)
- [x] 8.3 Update `web/src/components/looq-detection.ts`: stop showing the "fell back to plain
  text" warning for input that matched via recognised prefixes
- [x] 8.4 Surface the inferred-year flag in the UI wherever the existing timezone-assumption
  signal is shown, so a guessed year is visible rather than silent

## 9. Verification against the budgets

- [x] 9.1 Re-run `cargo bench -p looq-core` against task 1.2's numbers, including a
  pathological fixture that alternates timestamp shapes to defeat the sticky choice; the
  <200 ms/MB TDR §11 target is the gate — `cargo bench -p looq-core`: json 76.0 ms/MB
  (was 74.4), logfmt 104.4 ms/MB (was 100.4), plain 121.3 ms/MB (was 120.1), and the new
  `plain_mixed_shapes` fixture (six shapes rotating line to line, so the sticky choice
  misses on ~5 lines in 6) 149.3 ms/MB. Worst case is 75% of the 200 ms/MB gate
- [x] 9.2 Rebuild via `scripts/build-frontend.sh` and record the new `core.wasm` size against
  task 1.1's 194,350 B and the ~300KB TDR §5 budget — `bash scripts/build-frontend.sh` then
  `wc -c < crates/looq/assets/wasm/core.wasm`: **206,629 B**, +12,279 B (+6.3%) over the
  194,350 B baseline and 67% of the ~300KB (307,200 B) TDR §5 budget, ~100KB of headroom
  left. Design.md predicted "tens of KB against ~113KB of headroom" — it landed at the low
  end of that
- [x] 9.3 Add fixtures under `tests/` for each newly recognised shape and run the full suite
  plus `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` — added
  `prefix-{syslog3164,klog,clf,slash-date,epoch,payload}.log` and `docker-wrapper.jsonl`,
  each driven through auto-detection with a fixed reference instant. `cargo test --workspace`
  165 passed / 0 failed (88 looq-core unit + 31 integration + 27 + 19 elsewhere);
  `cargo fmt --all --check` clean; `cargo clippy --all-targets --all-features -- -D warnings`
  clean
- [x] 9.4 Load a real multi-format file in the browser and confirm the timeline is populated
  where it previously was empty — 300 lines rotating syslog 3164 / syslog+logfmt payload / klog /
  CLF / ISO+JSON payload, served via `./target/release/looq --stdin --port 7899`, driven with
  Playwright. 300/300 entries timestamped, timeline 17:42:40 → 17:47:00, format panel `plain
  (100%)` (no "fell back to plain text" warning), chips for status/path/service/attempt/trace off
  plain-text lines, CLF client address preserved in the message, detail pane shows "year inferred
  — this timestamp shape carries none". Screenshot: `.playwright-mcp/mixed-formats-timeline.png`

## 10. Documentation

- [x] 10.1 Update `README.md` and `README.ru.md` together — supported input, the level
  precedence change, and the fact that a guessed year is flagged. A lagging Russian README is
  worse than none — added a "Supported log formats" / "Поддерживаемые форматы логов" section to
  both (shape table, level precedence with the behavior-change callout, payload dispatch, Docker
  unwrap, inferred year), updated both status blurbs, and added two new Known limitations entries
  to each (access-log fields not broken out, no user-defined patterns)
- [x] 10.2 Append a `docs/devlog.md` entry with the measured numbers and the command that
  produced each — entry dated 2026-08-18, including the baseline-vs-after table, the
  design.md ~80 vs actual 120.1 ms/MB machine discrepancy, the two design decisions that
  changed during implementation, and the reference-instant hole found during verification
- [x] 10.3 Run `openspec validate prefix-and-payload-parsing --strict` before archiving
