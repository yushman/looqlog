## 1. Metadata and license

- [x] 1.1 Point `repository` in the workspace `Cargo.toml` at
  `https://github.com/yushman/openlogviewer`, and confirm `cargo metadata` reports the new URL.
- [x] 1.2 Add a `LICENSE` file with the full MIT text, copyright holder `ivyush`, year 2026,
  matching the `license = "MIT"` field already declared.
- [x] 1.3 Replace the `looq-dev/looq` clone URL in `README.md` and `README.ru.md` with the real
  one, in the same commit, and grep both files for any other `looq-dev` occurrence.

## 2. Smoke-test script

- [x] 2.1 Add `scripts/smoke-release-binary.sh` taking the binary path and the expected version
  as arguments, written to run under `bash` on Linux, macOS and Git Bash on Windows.
- [x] 2.2 Assert `--version` prints exactly the expected version; fail loudly with both values
  on mismatch.
- [x] 2.3 Assert `--help` names every flag from TDR §6 (`--port`, `--host`, `--open`,
  `--no-browser`, `--stdin`, `--max-lines`, `--version`, `--help`), naming which flag is missing.
- [x] 2.4 Start the binary on a fixed free port, assert `/` returns 200 with a non-empty body and
  the embedded `core.wasm` returns 200 with the `application/wasm` content type, then stop it.
  Do not use `--port 0` — the smoke test needs to know the port without parsing stdout.
- [x] 2.5 Assert a line piped into stdin mode reaches a WebSocket client on `/ws`. Note in a
  comment why this is stdin mode and not file mode: with no TTY, `mode_for` selects stdin
  regardless of a path argument (design D3).
- [x] 2.6 Make every failure path exit non-zero and print what was expected against what was
  observed — a smoke test that fails quietly is worse than no smoke test.
- [x] 2.7 Run the script locally against `target/release/looq` and confirm it passes, then
  deliberately break one assertion and confirm it fails non-zero.

## 3. Release workflow

- [x] 3.1 Add `.github/workflows/release.yml` triggered on `push: tags: ['v*']`, with
  `permissions: contents: write` and no other permission.
- [x] 3.2 Add a first job asserting the tag version equals the workspace `version` in
  `Cargo.toml`, failing before any build when they disagree (design D1).
- [x] 3.3 Add the build matrix: `x86_64-unknown-linux-musl` on `ubuntu-latest` (with
  `musl-tools` and the rustup target), `aarch64-apple-darwin` and `x86_64-apple-darwin` on
  `macos-latest`, `x86_64-pc-windows-msvc` on `windows-latest`.
- [x] 3.4 Build each target in release mode without invoking `wasm-pack` or npm — the vendored
  artifacts are what ship (ADR-0008), and CI's existing staleness job already guards them.
- [x] 3.5 Run `scripts/smoke-release-binary.sh` against each freshly built binary in its own job,
  with `shell: bash` on all four runners.
- [x] 3.6 Measure each binary's size, write it to the job summary, and fail the Linux job if it
  exceeds TDR §5's ~10 MB budget (design D4).
- [x] 3.7 Name each uploaded asset `looq-<version>-<target>` so a downloaded file identifies its
  own platform and version.
- [x] 3.8 Add a final publish job depending on all four build jobs that creates the GitHub
  Release and attaches every asset, so a target that fails to build or smoke-test blocks the
  release entirely.
- [x] 3.9 Put the per-target sizes and the TDR §5 budget into the release body, so the recorded
  numbers outlive the build logs.
- [x] 3.10 Verify the workflow file parses — run `actionlint` if available, otherwise validate
  the YAML and check every `uses:` action reference resolves to a real published version.

## 4. Documentation

- [x] 4.1 Rewrite the install section of `README.md`: download-and-run as the primary path with
  a real Releases URL, build-from-source second, `cargo install looq` still marked as planned.
- [x] 4.2 Mirror the same section into `README.ru.md` in the same commit — content parity is a
  `packaging` requirement, not a nicety.
- [x] 4.3 State in both READMEs which platforms have been exercised by a human and which ship
  verified by automated smoke test only (Windows and Intel macOS — see design's risk list).
  Do not describe all four as equally tested.
- [x] 4.4 Remove the now-stale sentence about a prebuilt binary being published "once v0.1.0
  ships", and the pointer to `docs/devlog.md` for sizes, since the release body now carries them.

## 5. Verification and record

- [x] 5.1 Run the full local gate green before handing back: `cargo test --workspace`,
  `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `npm run typecheck`, `npm run test`.
- [x] 5.2 Run `openspec validate ship-0-1-0-release --strict` and confirm it passes.
- [x] 5.3 Append a `docs/devlog.md` entry recording what shipped, the measured sizes per target
  against the TDR §5 budget, and the mode-selection constraint found in design D3 — the fact
  that a CI runner cannot exercise file mode is the kind of thing the next person would
  otherwise rediscover by writing a test that silently asserts the wrong thing.
- [x] 5.4 Confirm `git status` is clean and report which platforms were verified how.

## 7. Defect found while verifying the implementation

- [x] 7.1 Build `x86_64-apple-darwin` on `macos-15-intel` rather than `macos-latest`: the
  latter is arm64 and its image does not list Rosetta 2, so the smoke test would have failed
  with `Bad CPU type in executable` on the tag push, after three other targets had built.
- [x] 7.2 Record the runner choice as design D7 so it is not "tidied up" back into a shared
  `macos-latest` label.
- [x] 7.3 Fix the stale `design D4/D9` reference in the publish job's step name — there is no D9.
- [x] 7.4 Re-check every runner label in the matrix resolves to a currently offered GitHub
  label, and confirm the devlog entry describes the runner split.

## 6. Handoff (not for the implementing agent)

- [x] 6.1 Push the `v0.1.0` tag. This is the maintainer's action: it is public, hard to
  reverse, and per this project's rules nothing is pushed without an explicit request.
  Done on the maintainer's explicit instruction; all six jobs passed and the release published.
- [ ] 6.2 After the workflow runs, download the Linux binary on a real machine and run it once,
  closing the "clean machine" requirement with a human rather than only a runner.
  **Partially done:** the published Linux asset was downloaded and confirmed `static-pie linked`
  (`file`), but not executed — there is still no Linux machine here. The published
  `aarch64-apple-darwin` asset was downloaded and run (`looq 0.1.0`). Running the Linux binary
  on a real Linux box remains genuinely owed; the runner's smoke test is not a substitute for it.
