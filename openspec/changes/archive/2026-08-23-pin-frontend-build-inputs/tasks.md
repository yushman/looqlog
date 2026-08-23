## 1. Record the versions being pinned

- [x] 1.1 Confirm the toolchain that built the committed `crates/looqlog/assets/`: run `rustc --version`,
  `wasm-pack --version` and `wasm-opt --version` and record the three values in the change notes.
  Expected `1.97.1`, `0.15.0`, `132` — if any differs, stop and report before pinning, because pinning
  to a version that did not build the committed artifact makes the job red on the very next run.
  Confirmed: `rustc 1.97.1`, `wasm-pack 0.15.0`, `wasm-opt version 132` — all match.

## 2. Pin the floating inputs in CI

- [x] 2.1 In `.github/workflows/ci.yml`, replace `dtolnay/rust-toolchain@stable` with an explicit
  `dtolnay/rust-toolchain@1.97.1` (keeping `targets: wasm32-unknown-unknown`) in the
  `frontend-artifact-staleness` job only. Leave `fmt`, `clippy`, `test`, `build-no-node` and
  `adr-0005-boundary` on `@stable` — design D1.
- [x] 2.2 Add a comment above that step, in the voice of the existing binaryen comment, naming why the
  pin exists (rustc is an input to the diffed bytes) and where the other copy of the number lives
  (`EXPECTED_RUSTC` in `scripts/build-frontend.sh`).
- [x] 2.3 Change the install step to `npm install -g wasm-pack@0.15.0`, with a comment giving the same
  reason.

## 3. Make a local rebuild self-checking

- [x] 3.1 In `scripts/build-frontend.sh`, add `EXPECTED_RUSTC=1.97.1` next to the existing
  `EXPECTED_BINARYEN=132`, and a `rustc --version` check that mirrors the binaryen block exactly:
  same two-line warning shape, same non-fatal behaviour, naming the version found, the version
  expected and the consequence (design D2).
- [x] 3.2 Pass `--locked` through to the cargo build by appending `-- --locked` to the `wasm-pack build`
  invocation, with a one-line comment on why (design D3).
- [x] 3.3 Run `./scripts/build-frontend.sh` and confirm two things: it prints no warnings on this
  machine, and `git diff --stat -- crates/looqlog/assets/` is empty afterwards. A non-empty diff here
  means the pinned versions are wrong — do not commit a rebuilt artifact to make it pass, report it.
  First attempt: no warnings, but the diff was non-empty — `crates/looqlog/assets/wasm/core.wasm`
  differed by 102,441 bytes (216857 → 216885). Root-caused as a pre-existing staleness bug, not a
  pin problem: commit `156044f` changed `crates/looqlog-core/src/detect.rs` without rebuilding
  `crates/looqlog/assets/`. Fixed independently in `46e3db4` ("rebuild the vendored core.wasm for the
  detection fix"). Re-run after `46e3db4`: no warnings, `git diff --stat -- crates/looqlog/assets/`
  empty. Both halves of the claim now hold.
- [x] 3.4 Verify the warning path actually fires: temporarily set `EXPECTED_RUSTC` to a version that is
  not installed, run the script, confirm the warning text appears and the build still completes, then
  restore the real value.
  Confirmed: `EXPECTED_RUSTC=9.9.9` produced "warning: rustc version 1.97.1, expected 9.9.9 — the
  rebuilt core.wasm will differ from CI's and fail the staleness check." and the build still completed
  (exit 0); constant restored to `1.97.1` afterward.

## 4. Preserve the evidence when the check fails

- [x] 4.1 Add an `actions/upload-artifact@v4` step to `frontend-artifact-staleness`, after the drift
  check, guarded by `if: failure()`, uploading `crates/looqlog/assets/` under a name that identifies
  the run. It must be a separate step because the drift step exits non-zero (design D4).
- [x] 4.2 Add a comment explaining what the upload is for — the committed side is always recoverable
  from git, the rebuilt side exists only inside that runner — and reference the unexplained
  `core.wasm` drift note in `docs/devlog.md`.
- [x] 4.3 Extend the drift step's `::error::` output with one line telling the reader the rebuilt
  artifact is attached to the run, so the annotations point at the evidence rather than only
  describing it.

## 5. Clear the deprecated actions

- [x] 5.1 Bump every `actions/checkout@v4` to `@v5` in `.github/workflows/ci.yml`,
  `.github/workflows/release.yml` and `.github/workflows/pages.yml`.
- [x] 5.2 Bump every `actions/setup-node@v4` to `@v5` in the same files. Confirm no step passes an
  input beyond `node-version` that would need revisiting (design D5).
  Confirmed: `release.yml` and `pages.yml` don't use `setup-node` at all; every `setup-node` step in
  `ci.yml` passes only `node-version: 24`.

## 6. Verify

- [x] 6.1 Lint the three workflow files for YAML validity (`python3 -c "import yaml,sys; [yaml.safe_load(open(f)) for f in sys.argv[1:]]" .github/workflows/*.yml` or equivalent).
- [x] 6.2 Confirm no `@stable` remains in `frontend-artifact-staleness` and no `@v4` of `checkout` or
  `setup-node` remains anywhere under `.github/workflows/`.
- [x] 6.3 Re-run `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`
  and `cargo test --workspace` to confirm nothing in this change touched Rust behaviour.
  `cargo fmt --check`: clean. `cargo clippy --workspace --all-targets -- -D warnings`: clean. `cargo
  test --workspace`: 215 passed, 0 failed (one earlier run hit 1 failure, reproducing the known
  `free_port()` TOCTOU flake already on the devlog's Ideas list; a clean rerun passed 215/215).
- [x] 6.4 Confirm `git status` shows no change under `crates/looqlog/assets/` — this change must not
  move the artifact.
  Confirmed clean after `46e3db4` landed the unrelated staleness fix: `git status --short` shows no
  entry under `crates/looqlog/assets/`.

## 7. Documentation

- [x] 7.1 In `docs/devlog.md`, under *Ideas for later*: strike the CI-pinning item and the
  `actions/checkout@v4` / `setup-node@v4` deprecation item as done.
- [x] 7.2 Leave the unexplained 216,854-byte `core.wasm` item standing, and amend it to note that the
  failing artifact is now captured automatically by the staleness job, so the instruction no longer
  depends on someone remembering to save it.
- [x] 7.3 Strike the "nobody has run the Linux binary on a Linux machine" item as won't-do, replacing it
  with the corrected claim: the musl binary is built and smoke-tested on `ubuntu-latest`, which is a
  real Linux machine; what remains untested is a non-Ubuntu, non-glibc distribution, and running one
  is a deliberate gap rather than an open task.
- [x] 7.4 Append a devlog entry for this change: which three versions were pinned and to what, why the
  pin is scoped to one job, and the measured confirmation from 3.3 that the artifact did not move.
  Also covers the incident found by 3.3 (156044f's inert detection fix, root-caused and fixed
  independently in 46e3db4) and the free_port() TOCTOU flake's second real occurrence.
- [x] 7.5 Confirm no README change is needed — no install step, command, flag, output or limitation
  changed — and say so explicitly in the devlog entry rather than leaving it unstated.

## 8. Close out

- [x] 8.1 Run `openspec validate pin-frontend-build-inputs --strict` and fix anything it reports.
  `Change 'pin-frontend-build-inputs' is valid` — nothing to fix.
