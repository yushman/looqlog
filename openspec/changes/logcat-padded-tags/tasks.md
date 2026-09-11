## 1. Fixture from real output

- [x] 1.1 Add `crates/looqlog-core/tests/fixtures/logcat-adb-threadtime.log`: a 100–200
      line slice of real `adb logcat -v threadtime` output. It MUST carry padded tags
      (`vold`, `libc`, `netd`, `Binder`, `System`), unpadded ones (`ServiceManagement`,
      `wificond`, `SystemServerTiming`), the two-column `pid tid` layout, at least one
      padded-tag message whose own text contains a colon, and one continuation chain
      (repeated prefix with identical pid/tid/level/tag). Do NOT invent a three-column
      `uid` record — the measured emulator emits none, and inventing shapes is how the
      original rule got written (design D5). Source dump — 37,427 lines captured from an
      Android 10 emulator with `adb -s emulator-5554 logcat -d`:
      `/private/tmp/claude-501/-Users-ivyush-proj-openlogviewer/5d2c8a48-4d15-40c1-b4af-5bbfd99ffbf2/scratchpad/full.txt`
- [x] 1.2 Note in the fixture's first line, as a comment-free header line or in the test
      that loads it, where it came from and which `adb` invocation produced it, so the
      next person can regenerate it.

## 2. Recognizer

- [x] 2.1 In `crates/looqlog-core/src/timestamp.rs`, change `match_logcat_tag` to scan
      the tag, then skip any run of spaces and tabs, then require the colon. The
      returned `tag_end` MUST stay at the end of the tag token, so the reported range
      never includes the padding (design D3). Do not bound the length of the run
      (design D2) — record that choice in the function's doc comment, including the
      false-positive it accepts.
- [x] 2.2 Update the doc comment on `match_logcat` / `match_logcat_tag` that currently
      explains the tag "carries no whitespace" — it is about to be half wrong, and the
      reason the colon anchor still holds is the thing worth writing down.
- [x] 2.3 Verify no other caller depends on `tag_end` being the byte before the colon
      (`after_colon` is what the record's `end` uses); the message must still start
      after the colon, not after the tag.

## 3. Fields and continuation identity

- [x] 3.1 Confirm `logcat_fields` in `crates/looqlog-core/src/parsers/plain.rs` produces
      the trimmed tag with no change of its own — it should follow from 2.1. If it does
      not, the trim is in the wrong place; fix 2.1 rather than trimming here (design D3).
- [x] 3.2 Confirm `LogcatIdentity::of` stores the same trimmed text, so the
      `entry-continuations` rule "same pid, tid, level and tag" compares tags and not
      padding.

## 4. Tests

- [x] 4.1 `timestamp.rs`: a padded tag matches, with the tag range excluding the padding.
- [x] 4.2 `timestamp.rs`: a padded tag whose message contains its own colon stops at the
      tag's colon (`vold    : Detected support for: ext4 f2fs vfat`).
- [x] 4.3 `timestamp.rs`: a padded tag with no colon at all (`I vold    started`) does
      NOT match and does not consume the head — the anchor that design D2 leans on.
- [x] 4.4 `timestamp.rs`: the existing two-column and three-column scenarios still pass
      with padding applied to their tags.
- [x] 4.5 `detect.rs`: a test paired with `logcat_majority_input_is_a_threshold_match_not_a_fallback`
      (detect.rs:255) built from *padded* tags, asserting `Threshold` and
      `TimestampShape::Logcat`. Verify it fails against the unfixed recognizer before
      accepting it — a test that cannot fail is the defect being fixed here.
- [x] 4.6 `detect.rs`: a sample mixing padded and unpadded records resolves to the
      logcat modal shape.
- [x] 4.7 `plain.rs` or `parser_integration.rs`: a stack trace under a padded tag
      collapses into one entry, proving the trim did not break `LogcatIdentity`.
- [x] 4.8 `parser_integration.rs`: parse `logcat-adb-threadtime.log` end to end and
      assert every line yields a timestamp and a level, and that `tag` values carry no
      trailing spaces.
- [x] 4.9 `cargo test -p looqlog-core` green; run `cargo bench` on the parse benchmark and
      confirm no regression in the hot path (CLAUDE.md requires this before merging
      changes to `looqlog-core`'s hot paths).

## 5. End-to-end verification

- [x] 5.0 Rebuild the vendored frontend artifacts with `scripts/build-frontend.sh` before
      measuring anything. `core.wasm` under `crates/looqlog/assets/wasm/` is what the
      browser parses with; a `cargo build --release` after a `looqlog-core` change serves
      the old parser to the page and the measurement silently reads pre-fix behavior.
      (Added during verification — the list did not have it, and the run that found this
      was nearly invalid because of it.)
- [x] 5.1 Re-run the measurement that found the bug: pipe the 37,427-line dump through
      the built binary and confirm the timeline's "N entries have no timestamp" count
      drops from 5,805 to approximately zero, the format banner reports a threshold
      match rather than "fell back to plain text", and `tag=vold ` (trailing space —
      that is how `field=value` becomes a chip) matches the entries whose substring
      count is 67.
- [x] 5.2 Record the before/after numbers; they go in the devlog entry.

## 6. Specs and docs

- [ ] 6.1 Apply the delta specs: `log-parsing` (padding rule plus two scenarios),
      `field-extraction` (trimmed `tag` value), `format-detection` (padded-sample
      threshold requirement). Handled by `/opsx:archive`, not by hand — archiving is
      blocked until group 5 runs, so this stays open (precedent:
      `archive/2026-08-20-rename-to-looqlog` task 6.1).
- [x] 6.2 `openspec validate logcat-padded-tags --strict` passes.
- [x] 6.3 `README.md`: the Android logcat section (around line 220) states that the
      default `adb logcat` output is understood, padded tags included, and that `tag`
      filters on the tag without padding.
- [x] 6.4 `README.ru.md`: the same change, in the same commit. A Russian README that
      lags the English one is worse than none (CLAUDE.md).
- [x] 6.5 `docs/devlog.md`: an entry with the measured numbers and the commands that
      produced them, plus a line under `## Ideas for later` for the field-inventory
      noise this change deliberately leaves alone — `logfmt::looks_like_payload` firing
      on prose because base64 `==` in APK paths reads as `key=value`, giving 154 field
      names on a 37k-line corpus.
