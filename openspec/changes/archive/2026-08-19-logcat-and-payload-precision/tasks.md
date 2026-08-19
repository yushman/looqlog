## 1. Baseline and fixtures

- [x] 1.1 Record the current numbers before touching anything: `wc -c <
  crates/looq/assets/wasm/core.wasm` (expected 206,629 B) and `cargo bench -p looq-core`
  (expected json 76.0, logfmt 104.4, plain 121.3, plain_mixed_shapes 149.3 ms/MB)
  — `wc -c < crates/looq/assets/wasm/core.wasm` = **206,629 B**, exactly as expected.
  `cargo bench -p looq-core`: json **74.0** ms/MB, logfmt **100.2**, plain **121.5**,
  plain_mixed_shapes **150.0**. json/logfmt came in 2–4% faster than the recorded
  numbers (different machine load, same code); these measured values are the baseline
  task 7.1 compares against, not the ones written above.
- [x] 1.2 Confirm the suite is green (`cargo test --workspace`, `cargo fmt --all --check`,
  `cargo clippy --all-targets --all-features -- -D warnings`) so later failures are attributable
  — all three green before any edit: 31 integration + unit tests pass, fmt clean, clippy
  clean with `-D warnings`.
- [x] 1.3 Add `crates/looq-core/tests/fixtures/prefix-logcat.log` covering all four observed
  column layouts, plus negatives: a line with columns but no `Tag:`, and an `S` severity
  — 7 lines: `uid pid tid`, `name pid tid`, `pid tid`, `u0_aNN pid tid`, an `S` record, a
  record whose message is itself logfmt (for task 3.4), and the tagless negative. 6 of 7
  carry a recognised prefix, which keeps detection above the 80% threshold.

## 2. logcat scanner (`crates/looq-core/src/timestamp.rs`)

- [x] 2.1 Add `TimestampShape::Logcat` and the `MM-DD hh:mm:ss.mmm` byte scanner, reusing the
  existing shared primitives (`take_digits`, `match_time`, `match_fraction`)
  — `match_logcat`; the fraction is *required* (an unfractioned `MM-DD hh:mm:ss` is too weak
  a head to hang the rest of the shape on). `SHAPES` is now 7 entries and the no-overlap
  property still holds: logcat's `-` at index 2 disagrees with every other shape.
- [x] 2.2 Implement the column skipper: two or three tokens, each all-digits, a short lowercase
  word, or `u0_aNN`
  — `match_logcat_column`, capped at `LOGCAT_COLUMN_MAX = 16` bytes. Uppercase is excluded
  from every alternative, which is what makes the severity letter unambiguously not a
  column and lets the run terminate without a lookahead vocabulary. `_` is allowed in the
  lowercase-word alternative for real uid names like `webview_zygote`.
- [x] 2.3 Require the full shape through the tag's `:` before accepting — a partial match must
  consume nothing (design D1). This is the safety property; write the negative test first
  — `logcat_without_a_tag_is_not_consumed` written first and covers four ways to fail the
  anchor (nothing after the letter, a tagless word, trailing space, empty tag).
  `logcat_rejects_shapes_it_does_not_emit` covers 1 column, 4 columns, no fraction, an
  uppercase column, a non-severity letter and an impossible date.
- [x] 2.4 Return the severity letter, the columns and the tag alongside the parsed timestamp so
  the plain parser can turn them into level and fields (design D2)
  — `LeadingMatch.logcat: Option<LogcatRecord>` (uid/pid/tid/tag as `&str` into the line)
  plus the existing `level_letter`. `S` is accepted by the *shape* so the record is still
  recognised, and `level::from_letter` continues not to map it.
- [x] 2.5 Reuse `ParseContext`'s reference instant for the missing year and set
  `timestamp_year_inferred`, exactly as syslog 3164 and klog already do (design D3)
  — `parse_logcat` calls the same `resolve_inferred_year`; no clock read, no new field.
  `logcat_needs_a_reference_instant_like_every_other_year_less_shape` asserts the shape is
  not dated by invention when no reference was supplied.
- [x] 2.6 Unit-test each layout and each negative from 1.3
  — 8 new tests in `timestamp.rs`; `cargo test -p looq-core --lib timestamp` = 44 passed,
  0 failed.

## 3. logcat fields (`crates/looq-core/src/parsers/plain.rs`, `level.rs`)

- [x] 3.1 Map the severity letter through the existing letter table; do NOT map `S`
  — no change needed in `level.rs`: `LETTER_CODES` never contained `S`, so
  `from_letter(b'S')` already returns `None` and `plain.rs`'s existing
  `level_letter.and_then(level::from_letter)` does the right thing unchanged.
- [x] 3.2 Emit `tag`, `pid`, `tid`, and `uid` when a third column is present
  — `logcat_fields` in `plain.rs`. `tag` is a `String`; the numeric columns are
  `FieldValue::Number` and a named uid (`root`, `u0_a2`) stays a `String`, so the field
  inventory does not present `root` as a number.
- [x] 3.3 Make the message the text after the tag's colon — columns and tag must not remain in
  it
  — falls out of the scanner consuming through the colon; asserted on the `cmd: dumpsys
  cpuinfo` record, whose message keeps its *own* colon while the tag's is gone.
- [x] 3.4 Test that a logcat line's payload dispatch still runs, so a record whose message is
  itself structured contributes those fields too
  — `logcat_message_that_is_itself_structured_contributes_both_sets_of_fields`: the columns
  are built before the dispatch branch and the payload's fields are layered on top, so a
  payload key colliding with a column name wins the same way it wins the timestamp (D7).
  `cargo test -p looq-core --lib` = 103 passed, 0 failed.

## 4. Defect: braced logfmt values (`crates/looq-core/src/parsers/logfmt.rs`)

- [x] 4.1 In `tokenize`, consume a value beginning with `{` to its matching `}`, tracking depth
  and ignoring braces inside quoted spans (design D4)
  — `skip_braced`. Depth can never underflow: the caller only enters with `chars[start] ==
  '{'`, which is counted on the first iteration. Backslash escapes inside a quoted span
  advance two chars, clamped to the end so an escape at the last char cannot overrun.
- [x] 4.2 Test the measured case: `time=18ms ret=204 headers={null=[…], Alt-Svc=[…], Content-Length=[0]}`
  yields exactly three fields, and `Alt-Svc` is not among them
  — `braced_value_is_one_field_not_many` (exactly `headers`, `ret`, `time`) plus
  `a_dumped_java_map_behind_a_prefix_does_not_become_many_fields` in `plain.rs`, which runs
  the PROBE_HTTP line *verbatim* from the bugreport end to end: exactly `headers`,
  `request`, `ret`, `time`, and none of `Alt-Svc` / `Content-Length` / `User-Agent` /
  `Connection`.
- [x] 4.3 Test nested braces and an unterminated brace (the latter must not hang or panic — it
  consumes to end of line)
  — `nested_braces_are_balanced_to_the_outer_one` (`{a={b=1}, c=2}` is not truncated at the
  first `}`), `a_brace_inside_a_quoted_span_does_not_change_the_depth`, and
  `unterminated_brace_consumes_to_end_of_line` (including an unterminated quote *inside* an
  unterminated block). `cargo test -p looq-core --lib` = 109 passed, 0 failed.

## 5. Defect: keys are not levels (`crates/looq-core/src/level.rs`)

- [x] 5.1 Rewrite `scan_message` to track token end offsets instead of `split()`, so the
  following character is known
  — byte-index walk over runs of ASCII letters. Word-boundary semantics are unchanged:
  a non-ASCII byte is not `is_ascii_alphabetic`, so multibyte text separates tokens exactly
  as `split(|c| !c.is_ascii_alphabetic())` did, and both ends of a run land on a `str` char
  boundary.
- [x] 5.2 Skip any token immediately followed by `=` (design D5)
- [x] 5.3 Test both sides of the rule: `err=Success` yields no level; `level=err` still yields
  ERROR
  — `a_key_is_not_a_level`, `a_level_on_the_value_side_still_resolves` and
  `the_key_rule_does_not_disturb_ordinary_prose` (which also pins the multibyte case).
  `cargo test -p looq-core --lib` = 112 passed, 0 failed.

## 6. Detection (`crates/looq-core/src/detect.rs`)

- [x] 6.1 Include `Logcat` in the detection sample and the sticky shape hint
  — no code change in `detect.rs` was needed: `prefix_evidence` goes through
  `timestamp::prefix_shape`, which sweeps `SHAPES`, so adding the variant there was
  sufficient. That is design D6's claim ("absorb the new shape without structural change")
  holding in practice rather than by assertion.
- [x] 6.2 Test that a logcat-majority input reports plain text as a threshold match rather than
  a fallback
  — `logcat_majority_input_is_a_threshold_match_not_a_fallback` (90 logcat + 10 DUMPSYS
  banners → Plain/Threshold/0.90, shape `Logcat`, offset 0), plus
  `the_sticky_choice_records_logcat_like_any_other_shape` pinning that the modal rule still
  applies. Also end to end on the fixture:
  `logcat_fixture_covers_every_observed_column_layout` asserts the detection outcome, all
  four column layouts, the `S` record, the structured-message record, the tagless negative,
  and that `tag` reaches the field inventory with 6 values.
- [x] 6.3 Test that a bugreport-shaped input (unstructured preamble, logcat later) still parses
  every logcat line correctly with no sticky hint available (design D6)
  — `bugreport_shaped_input_parses_its_logcat_lines_with_no_sticky_hint`: 120 banner lines
  then 40 records. Detection reports `Fallback` and records **no** shape (the sample never
  reaches a record), and all 40 records are still dated, levelled and tagged off the full
  sweep. `cargo test --workspace` = 19 + 27 + 114 + 33 passed, 0 failed.

## 7. Verification against the budgets

- [x] 7.1 `cargo bench -p looq-core` against task 1.1's numbers; the <200 ms/MB TDR §11 target
  is the gate, including `plain_mixed_shapes`. First add logcat lines to
  `gen_mixed_prefix_fixture` — found during group 1–6 verification: that fixture contains none,
  so today it measures only the cost of a seventh shape in the sweep and never exercises the new
  scanner or a logcat-heavy sticky-hint miss, which is the case this change actually risks
  — fixture widened to `i % 7` with a logcat arm cycling all four column layouts on `i % 4`
  (`gcd(7,4)=1`, so every layout occurs); the line before a logcat line is never logcat, so
  the sticky hint misses on every one of them. The generated lines were checked to actually
  match `TimestampShape::Logcat` (throwaway integration test, deleted after) before
  benchmarking — a fixture that silently fails to match would have measured nothing again.
  `cargo bench -p looq-core`: json **74.0** ms/MB (baseline 74.0), logfmt **100.3** (100.2),
  plain **121.5** (121.5), `plain_mixed_shapes` **146.4** on the *new* fixture against 150.0
  on the old logcat-free one. That mixed run reported 157.6 ms/MB first (16% outliers, machine
  noise) and 146.4 on a re-run with a [146.2, 146.7] CI; both are recorded, both are under
  three quarters of the <200 ms/MB gate.
- [x] 7.2 `bash scripts/build-frontend.sh` and record the new `core.wasm` size against 206,629 B
  and the ~300KB TDR §5 budget
  — `wc -c < crates/looq/assets/wasm/core.wasm` = **210,379 B**, +3,750 B (+1.8%) over the
  206,629 B baseline, 68.5% of the ~307,200 B budget with ~96.8KB of headroom left.
  wasm-pack + npm both present; the build ran clean.
- [x] 7.3 Full suite plus `cargo fmt --all --check` and
  `cargo clippy --all-targets --all-features -- -D warnings`
  — `cargo test --workspace` **193 passed, 0 failed** across 7 binaries (19 + 27 + 114 + 33 +
  3 empty), fmt clean, clippy clean with `-D warnings`. No new dependencies.
- [x] 7.4 Re-run the Android bugreport end to end in the browser and record the new
  "N of M entries have no timestamp" figure against the measured baseline of 161,177 of
  164,275 (98.1%). Confirm `tag` appears as a filter chip and that `Alt-Svc` /
  `Content-Length` / `X-Android-Sent-Millis` no longer do
  — `cargo build --release -p looq`, then `(cat <bugreport>.txt; sleep 600) | ./target/release/looq
  --stdin --port 7896 --no-browser --max-lines 200000`, driven at `http://127.0.0.1:7896/`.
  Timeline caption: **"128778 of 164275 entries have no timestamp"** — 78.4%, against the
  161,177/164,275 (98.1%) baseline. Entries on the timeline went 3,098 → **35,497** (+32,399);
  `grep -cE '^[0-9]{2}-[0-9]{2} [0-9]{2}:[0-9]{2}:[0-9]{2}\.[0-9]{3} '` counts 33,300 lines
  opening with the shape, so ~900 correctly fail the rest of it (no columns / no `Tag:`).
  Entry count and skipped count unchanged (164,275 / 0 skipped). Filter rail: 382 fields,
  including `tag`, `pid`, `tid` (all three high-cardinality → typed input) and `uid` (34 values,
  numeric *and* named — `1000`, `drm`, `media`); `level` now resolves on 33,760 entries
  (TRACE 686, DEBUG 12,533, INFO 15,251, WARN 3,365, ERROR 1,891, FATAL 34). None of `Alt-Svc`,
  `Content-Length`, `X-Android-Sent-Millis` or `Cross-Origin-Resource-Policy` appear; the
  PROBE_HTTP line contributes exactly `time`, `ret`, `request`, `headers`. Diagnostics: "0
  skipped". **Detection still reads `fell back to plain text (12%)`** — unchanged, and the
  outcome design.md D6 predicted: the sample is the first 100 non-empty lines and a bugreport
  opens with ~1,370 lines of `dumpstate` preamble, so no prefix shape is selected from that
  head. Not a defect; recorded because the proposal quotes that 12% as motivation. Server
  stopped after the run.

## 8. Documentation

- [x] 8.1 Update `README.md` and `README.ru.md` together — logcat in the shape table, `tag`/
  `pid`/`tid`/`uid` as fields, and both behavior changes (braced values are one field; a key is
  no longer read as a level). Keep the two files in sync in the same pass
  — same pass, section-for-section: an `Android logcat` row in the existing shape table, a new
  **Android logcat** paragraph inside "Supported log formats" (whole-shape match through the
  `Tag:` colon, `S` supplies no level, columns become `tag`/`pid`/`tid`/`uid` and leave the
  message), the brace rule folded into **Payloads** with its own behavior-change callout, the
  key-is-not-a-level rule as a second callout beside the existing one under **Levels**, and
  **Inferred years** widened to "syslog, klog and logcat". No second section was bolted on.
- [x] 8.2 Add the dmesg/monotonic-time and bugreport-section limitations to the Known
  Limitations list in both READMEs, so what this change deliberately does not cover is stated
  — two bullets added to both lists, after "No user-defined format patterns": dmesg/kernel
  monotonic time (why the boot anchor is out of reach, and that those lines still become
  entries) and bugreport section awareness (why 74% of a bugreport stays off the timeline).
- [x] 8.3 Append a `docs/devlog.md` entry with the before/after bugreport figures and the
  command that produced each
  — `## 2026-08-19 — logcat-and-payload-precision: 98.1% of a bugreport had no timestamp, now
  78.4%`. Carries the before/after table with the exact commands, the bench table with both
  mixed-shape runs, the wasm size, and — stated plainly rather than buried — that the
  mixed-shape fixture contained no logcat lines until task 7.1, so any "no regression" claim
  made before that would have been measuring the wrong thing.
- [x] 8.4 Run `openspec validate logcat-and-payload-precision --strict` before archiving
  — passes.
