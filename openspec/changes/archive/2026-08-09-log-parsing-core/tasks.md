## 1. Fixtures and harness

- [x] 1.1 Fixtures: one per format (JSON Lines, logfmt, plain), one with custom fields (`service=api`), one with a deliberately malformed line, one with a latin-1 line, one high-cardinality, one with out-of-order and missing timestamps
- [x] 1.2 Shared test helper that feeds a fixture whole and in adversarial chunk splits (mid-line, mid-multi-byte-character) and asserts identical output
- [x] 1.3 `criterion` bench harness in `looq-core` for the parse hot path

## 2. Parse API skeleton

- [x] 2.1 Stateful incremental parser: byte chunks in, completed entries out, incomplete trailing line held until the next chunk or an explicit finish
- [x] 2.2 UTF-8 decoding with per-line latin-1 fallback inside the core crate, correct across chunk boundaries, fallback recorded as a diagnostic
- [x] 2.3 Diagnostics type: line number, reason, per-reason counts, retained-example cap
- [x] 2.4 Verify `looq-core` still has no `wasm-bindgen`/`web-sys`/`std::fs` dependency (ADR-0005), enforced in CI

## 3. Format parsers

- [x] 3.1 JSON Lines parser: object lines become entries, valid-but-non-object lines are malformed
- [x] 3.2 logfmt parser: quoted values with spaces and escapes, bare tokens folded into the message
- [x] 3.3 Plain-text fallback: one entry per non-empty line, never malformed, blank lines skipped silently
- [x] 3.4 One entry per physical line for all three, with a stack-trace fixture asserting N entries and no diagnostics

## 4. Detection

- [x] 4.1 Sample the first 100 non-empty lines, evaluate JSON → logfmt → plain, select at the 80% threshold, fall back to plain otherwise
- [x] 4.2 Return the chosen format, the observed match fraction, and threshold-versus-fallback
- [x] 4.3 Explicit format override that skips detection, with skipped lines still reported so a bad override is visible
- [x] 4.4 Test: three fixtures auto-detect correctly; a plain-text fixture whose first line is JSON does not get classified as JSON

## 5. Entry and fields

- [x] 5.1 `Entry`: optional timestamp, optional level, message, input ordinal, fields; returned in input order with no sorting
- [x] 5.2 Timestamp extraction: field-name precedence `timestamp`/`ts`/`time`/`@timestamp`/`t`, RFC 3339, epoch s/ms/µs by magnitude, leading-timestamp pattern for plain text
- [x] 5.3 Timezone policy: explicit offsets respected, naive values in a caller-supplied zone defaulting to UTC, applied interpretation reported (named IANA zones not supported — fixed-offset only; see devlog "NEEDS HUMAN DECISION")
- [x] 5.4 Entries with missing or unparsable timestamps kept and counted, with a diagnostic for unparsable values
- [x] 5.5 Level extraction: `level`/`lvl`/`severity` field first, message scan otherwise, alias table `WARNING`→`WARN` / `ERR`→`ERROR` / `CRITICAL`→`FATAL`, absent when nothing matches
- [x] 5.6 Arbitrary fields: JSON top-level members, logfmt pairs, nested objects and arrays kept as JSON text
- [x] 5.7 Field inventory with per-field distinct-value cap, counts, and a high-cardinality flag that stops accumulation

## 6. Measurement and limits

- [x] 6.1 Decide the two caps (retained diagnostics, distinct values per field) from a measured memory number, not a guess; record the number and the command in `docs/devlog.md`
- [x] 6.2 Native `criterion` numbers for each format's parse path, recorded against TDR §11
- [x] 6.3 Re-measure browser throughput on the ~1MB fixture now that the real parser replaced the stub, and compare against the day-4 number
- [x] 6.4 Measure `core.wasm` size after `serde_json`, `regex` and `chrono` land, against the ~300KB budget in TDR §5; if it breaks, decide and record whether to hand-roll the scanners or move the budget (broke at ~1022KB with `regex`; hand-rolled the leading-timestamp scanner and dropped `regex`, landing at 154.3KB)
- [x] 6.5 Memory test: one million unparsable lines stays within the diagnostic cap; 50,000 distinct `request_id` values stays within the inventory cap

## 7. Documents

- [x] 7.1 Close TDR §16 on invalid-line behaviour and timezone handling, and PRD §14 Q2, writing the decisions back into those documents
- [x] 7.2 Record the nested-JSON-as-text limitation (D8) and the one-line-one-entry limitation where a reader will meet them, so neither gets half-implemented later
- [x] 7.3 Devlog entries per working day, including the measured numbers from group 6

## 8. Wrap-up

- [x] 8.1 `cargo test -p looq-core` green, including chunk-split, encoding and cap tests
- [x] 8.2 `openspec validate log-parsing-core --strict` passes
- [x] 8.3 Archive the change so `openspec/specs/` reflects the shipped parser behaviour
