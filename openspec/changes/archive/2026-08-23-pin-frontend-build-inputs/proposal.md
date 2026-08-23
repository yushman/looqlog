## Why

CI's `frontend-artifact-staleness` job rebuilds `crates/looqlog/assets/` and diffs it against the
committed copy byte-for-byte. Of the inputs that decide those bytes, only binaryen is pinned (132).
`rustc` arrives via `dtolnay/rust-toolchain@stable` and `wasm-pack` via `npm install -g wasm-pack`
with no version, and the cargo build inside `wasm-pack` runs without `--locked`. Local and CI builds
match today only because both happen to sit on `rustc 1.97.1` and `wasm-pack 0.15.0`.

Rust ships every six weeks and 1.97.1 is dated 2026-07-14, so the next stable is due imminently. The
day it lands, this job turns red on whichever unrelated push happens to run first, and the failure
will read as that push's fault. The job also throws away the artifact it just rejected — which is
exactly how the 216,854-byte `core.wasm` anomaly recorded in `docs/devlog.md` lost its only
remaining evidence: it was rebuilt over instead of captured.

## What Changes

- Pin `rustc` to `1.97.1` in the `frontend-artifact-staleness` job only. `fmt`, `clippy`, `test`,
  `build-no-node` and `adr-0005-boundary` stay on `@stable` so new-stable lints and regressions keep
  surfacing.
- Pin `wasm-pack` to `0.15.0` in that job's install step.
- Add a `rustc --version` check to `scripts/build-frontend.sh`, mirroring the binaryen-132 check
  already there: warn loudly on a mismatch rather than let it surface as a confusing red run on
  someone else's push.
- Pass `--locked` to the cargo build that `wasm-pack` drives, so a silent `Cargo.lock` update cannot
  change the artifact.
- Upload the rebuilt `crates/looqlog/assets/` as a run artifact when the staleness diff fails, so the
  rejected bytes survive the job instead of depending on someone remembering the devlog note.
- Bump `actions/checkout@v4` → `@v5` and `actions/setup-node@v4` → `@v5` across `ci.yml`,
  `release.yml` and `pages.yml`, clearing the Node-20 deprecation warning every run currently carries.
- Update `docs/devlog.md`'s *Ideas for later*: strike the pinning item and the deprecated-actions
  item, keep the `core.wasm` anomaly item (still standing, now backed by automatic capture), and
  close the "nobody ran the Linux binary on Linux" item as won't-do with a corrected claim — the
  musl binary is built and smoke-tested on `ubuntu-latest`, which is a real Linux machine; what
  stays untested is a non-Ubuntu, non-glibc distribution, and that is a deliberate gap.

Not in scope: `free_port()`'s TOCTOU flake in `crates/looqlog/tests/cli.rs` (a test-only bugfix, no
spec behaviour) and running the musl binary inside an Alpine container (decided against).

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `packaging`: the "Vendored artifacts are verifiably current" requirement gains two obligations —
  every input that decides the artifact bytes must be pinned to an exact version, and a failing
  staleness check must preserve the rejected artifact rather than only describe it.

## Impact

- `.github/workflows/ci.yml` — the `frontend-artifact-staleness` job (toolchain pins, artifact
  upload) and every `checkout`/`setup-node` step across the file.
- `.github/workflows/release.yml`, `.github/workflows/pages.yml` — action version bumps only.
- `scripts/build-frontend.sh` — new `rustc` version check, `--locked` on the wasm build.
- `docs/devlog.md` — a new entry and four edits under *Ideas for later*.
- `openspec/specs/packaging/spec.md` — via this change's delta.
- No Rust or TypeScript source changes, no artifact rebuild, no user-facing behaviour change, so
  neither README needs updating.
