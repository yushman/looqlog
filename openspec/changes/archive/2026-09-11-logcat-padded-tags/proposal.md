## Why

`adb logcat` pads its tag with spaces to a width of 8 in `threadtime`, the default
format since Android 7 — `01-01 03:00:01.182   135   135 I vold    : Vold 3.0 firing
up`. The logcat recognizer requires the tag's colon to sit immediately after the tag,
so every padded record fails to match, and because the shape is all-or-nothing the
whole prefix is left unconsumed: no timestamp, no level, columns and tag stranded in
the message text.

Measured on a live Android 10 emulator (37,427 lines): 5,791 of them — 15.5% — parse
as unprefixed plain text. The UI reports it in three places at once. The timeline says
"5805 of 37427 entries have no timestamp". `tag=vold` matches nothing while the
substring `vold` matches 67 entries. And detection, sampling the first 100 lines of a
boot log where short tags like `vold`, `init` and `ueventd` dominate, matches 68 of
100, misses the 80% threshold, and tells the user "fell back to plain text — no format
matched at least 80%" on input that is nothing but logcat.

The recognizer was built against a bugreport, whose logcat sections carry long tags
(`ActivityManager:`, `ProcessCpuTracker:`) that need no padding. Piping `adb logcat`
straight into looqlog — the single most obvious way to use the tool on Android — was
never on the measured corpus.

## What Changes

- The logcat recognizer accepts whitespace between the tag and its terminating colon,
  so `vold    :` is recognised exactly as `ActivityManager:` is.
- The `tag` field carries the tag without its padding: `vold`, not `vold    `.
- The continuation chain's logcat identity compares trimmed tags, so a stack trace
  under a padded tag still collapses into one entry.
- Detection gains a regression test on padded input. Its existing logcat test is built
  from unpadded tags and passes vacuously — it cannot fail on the bug it was written
  to guard.
- Both READMEs state that the default `adb logcat` output is understood, padding
  included.

No behavior is removed and nothing is renamed. A line that did not match before and
does now was previously mis-parsed, not correctly parsed.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `log-parsing`: the "logcat records are recognised as a whole shape" requirement
  specifies a tag "terminated by `:`" and is silent on padding — every one of its
  scenarios came from a bugreport. It gains the padding rule and a scenario for it.
- `field-extraction`: the "logcat columns become fields" requirement gains the rule
  that the `tag` field's value is the tag alone, with the padding removed.
- `format-detection`: gains a requirement that a logcat-majority sample is a threshold
  match whatever its tag widths. The capability's existing rules already imply it; what
  they do not do is make the vacuous test fail, and a boot log is where this breaks.

## Impact

- `crates/looqlog-core/src/timestamp.rs` — `match_logcat_tag`, and the `LogcatRecord`
  the match produces.
- `crates/looqlog-core/src/parsers/plain.rs` — `logcat_fields` and `LogcatIdentity`,
  which compares tags as strings to decide whether a line continues the one above it.
- `crates/looqlog-core/src/detect.rs` — tests only; the detection logic is untouched,
  since a recognised prefix is already a threshold match.
- Test fixtures — a slice of real `adb logcat` output, which the corpus has never had.
- `README.md`, `README.ru.md`, `docs/devlog.md`.
