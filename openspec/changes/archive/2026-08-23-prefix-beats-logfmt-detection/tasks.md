## 1. Detection

- [x] 1.1 In `crates/looqlog-core/src/detect.rs`, move the `prefix_evidence(sample, ctx)` call above the logfmt branch so the logfmt decision can depend on it
- [x] 1.2 Make the logfmt branch conditional: return `Format::Logfmt` only when `logfmt_fraction >= THRESHOLD` **and** `prefix_fraction < THRESHOLD` (design D1)
- [x] 1.3 When logfmt yields to plain, report `match_fraction` as the prefix fraction and carry the modal `timestamp_shape`/`timestamp_offset`, exactly as any other prefixed plain-text win (design D4)
- [x] 1.4 Leave the JSON branch untouched and above everything — nothing about JSON Lines detection changes
- [x] 1.5 Add a comment at the tie-break explaining *why* plain wins here (it subsumes logfmt), so the next reader does not "restore" priority order as a cleanup

## 2. Core tests

- [x] 2.1 `detect.rs` unit test: 90 lines of `<ISO> <LEVEL> key=value key=value` select `Plain` with `Threshold`, `timestamp_shape: Some(Iso)`, `timestamp_offset: Some(0)`
- [x] 2.2 `detect.rs` unit test: 90 lines of `ts=<ISO> level=info msg="x" service=api` still select `Logfmt` with `Threshold` — the regression guard for D2
- [x] 2.3 `detect.rs` unit test: the same for `time=<ISO> …`, and for logfmt lines carrying no timestamp at all
- [x] 2.4 `detect.rs` unit test: JSON still wins over a sample that would also satisfy both logfmt and the prefix scanner
- [x] 2.5 Confirm every existing test in `detect.rs` still passes unchanged — particularly `ambiguous_input_falls_back_to_plain` and `result_carries_its_evidence`, which pin the branches this change reorders

## 3. End-to-end fixture

- [x] 3.1 New fixture `crates/looqlog-core/tests/fixtures/prefix-then-logfmt.log` — the shape from the proposal: ISO timestamp, positional level, a service token and two or three `key=value` pairs, across several levels
- [x] 3.2 Integration test: parse it with detection (no format override) and assert `Format::Plain`, then assert on a sample entry that timestamp, level **and** the payload fields are all present on the same entry
- [x] 3.3 Integration test: the same fixture with `Format::Logfmt` forced still parses as logfmt with no timestamp and no level — the override path is unchanged (design D4's escape hatch)
- [x] 3.4 Confirm `entries + skipped + blank == total lines` still holds on the new fixture

## 4. Verification

- [x] 4.1 `cargo test --workspace` — 208 passing before this change, so expect 208 plus the new ones, none failing
- [x] 4.2 `cargo clippy --all-targets` and `cargo fmt --check` clean
- [x] 4.3 `cargo bench -p looqlog-core` — `prefix_evidence` now also runs on logfmt-shaped input; confirm no measurable regression and record the numbers
- [x] 4.4 Run the real binary against a generated log of this shape and confirm in the browser that the timeline draws, the level chips populate, and the detection panel reports `plain` — this is the failure the change exists to fix, so seeing it fixed is the acceptance test
- [x] 4.5 No frontend or wasm change is expected; confirm `crates/looqlog/assets/` is untouched and does not need a rebuild
- [x] 4.6 Append a `docs/devlog.md` entry: how the defect was found, the measured discriminator table, and why no offset rule was needed
- [x] 4.7 `openspec validate prefix-beats-logfmt-detection --strict` passes, then run `/opsx:archive` — validate passes; archive is deliberately not run, that's the user's call
