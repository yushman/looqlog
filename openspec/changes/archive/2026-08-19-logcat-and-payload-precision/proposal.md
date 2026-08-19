## Why

`prefix-and-payload-parsing` widened the prefix scanner to six timestamp shapes, and an
Android bugreport measured immediately afterwards still came out at **161,177 of 164,275
entries with no timestamp (98.1%)**, detection reporting `fell back to plain text (12%)`.
The file parsed without a single skipped line, so nothing was broken — it just could not
see what it was looking at.

The gap is one shape: **logcat**. 32,712 lines of that 13MB file are logcat records whose
`04-21 13:07:53.198` timestamp matches none of the six shapes — klog is `I0421 13:07:53`
(letter, then `MMDD` with no dash), syslog 3164 is `Apr 21 13:07:53` (month name). Those
32,712 lines are the only real event log in a bugreport; the remaining ~74% is `dumpsys`
output that legitimately has no timestamps at all.

The same file surfaced two precision defects in code this project already shipped, both
visible in its UI and both cheap to fix while the parser is open.

## What Changes

- The prefix scanner learns the logcat shape: `MM-DD hh:mm:ss.mmm`, followed by two or
  three uid/pid/tid columns, a single severity letter, and a `Tag:` anchor. All four
  column layouts observed in a real bugreport are accepted (`uid pid tid`, `uidname pid
  tid`, `pid tid`, `u0_aNN pid tid`).
- A logcat line contributes `tag`, `pid`, `tid` and `uid` as fields. The tag is the
  valuable one: 334 distinct tags across 32,712 lines in the measured file — low
  cardinality, high selectivity, exactly what a filter chip is for.
- logcat's year-less timestamp reuses the inferred-year machinery
  (`timestamp_year_inferred`) that `prefix-and-payload-parsing` already built and flags in
  the UI.
- **Defect fix — the logfmt tokenizer no longer descends into brace-delimited values.** A
  value starting with `{` is consumed as one balanced, opaque block. Today a line carrying
  a Java map dump (`headers={null=[…], Alt-Svc=[…], Content-Length=[0], …}`) contributes
  every member of that dump as its own top-level field, so the measured file produced
  filter chips named `Alt-Svc`, `Content-Length`, `X-Android-Sent-Millis` and
  `Cross-Origin-Resource-Policy`. Genuine sibling pairs on the same line (`time=18ms
  ret=204`) keep working.
- **Defect fix — the whole-message level scan skips a token immediately followed by `=`.**
  Such a token is a key, not a level. Today `… I incidentd: Done taking incident report
  err=Success` is reported as **ERROR**, because the scan tokenises `err` and resolves it
  through the `ERR` → `ERROR` alias. 51 lines in the measured file hit this. Field *values*
  are unaffected: `level=err` still means ERROR.

## Capabilities

### New Capabilities

None. This adds one shape and two precision fixes to capabilities that already exist.

### Modified Capabilities

- `log-parsing`: the prefix scanner gains the logcat shape; the logfmt tokenizer stops
  splitting brace-delimited values.
- `field-extraction`: logcat contributes `tag`/`pid`/`tid`/`uid`; the whole-message level
  scan ignores tokens that are keys.

## Impact

- `crates/looq-core/src/timestamp.rs` — a `Logcat` variant on `TimestampShape` plus its
  scanner and the column-skipping rule.
- `crates/looq-core/src/level.rs` — the `=` guard in `scan_message`.
- `crates/looq-core/src/parsers/logfmt.rs` — balanced-brace value consumption in
  `tokenize`.
- `crates/looq-core/src/parsers/plain.rs` — logcat's fields flow into the entry.
- `crates/looq-core/src/detect.rs` — the new shape joins the detection sample and the
  sticky choice; no new `Format` variant.
- No new dependencies. `core.wasm` is 206,629 B against the ~300KB TDR §5 budget, and this
  is again hand-rolled scanning.
- Both READMEs and `docs/devlog.md`.

Out of scope, and staying in Known Limitations: **dmesg/kernel lines** (`[ 1538269.814760]`,
4,897 lines in the measured file) — monotonic time since boot is a different class of value
that cannot become an instant without a boot anchor read out of the file's preamble; and
**bugreport section awareness** (`------ DUMPSYS … ------` as a container with a per-section
format) — a product feature, not a parser fix. This change is expected to put roughly 20% of
that file's lines on the timeline, which is all of its actual event log.
