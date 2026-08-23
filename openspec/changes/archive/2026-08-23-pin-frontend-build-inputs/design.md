## Context

`frontend-artifact-staleness` (`.github/workflows/ci.yml`) is the only job in the repository that
asserts a byte-for-byte equality. It runs `./scripts/build-frontend.sh` and fails if
`crates/looqlog/assets/` moved. That makes it uniquely sensitive to its own environment: every input
that reaches the compiler reaches the diff.

The inputs, and how each is currently controlled:

```
                     committed crates/looqlog/assets/
                                   ▲  byte-for-byte diff
                                   │
   ┌───────────────────────────────┴──────────────────────────────┐
   │                     scripts/build-frontend.sh                │
   ├──────────────────────────────┬───────────────────────────────┤
   │  wasm-pack → core.wasm       │  vite → index.js/css/worker.js│
   ├──────────────────────────────┼───────────────────────────────┤
   │  rustc       @stable    FLOAT│  node         24.x    major   │
   │  wasm-pack   npm, no ver FLOAT│ vite/rollup   package-lock ✔ │
   │  binaryen    132        ✔    │  web/src      committed    ✔  │
   │  Cargo.lock  no --locked soft│                               │
   │  RUSTFLAGS   remap-path ✔    │                               │
   └──────────────────────────────┴───────────────────────────────┘
```

The JS half is already pinned by `npm ci` against `web/package-lock.json`. The wasm half is not. Both
`rustc` and `wasm-pack` float, and both are inputs to the same `core.wasm` the job diffs.

Two prior incidents recorded in `docs/devlog.md` and in the job's own comments show what a floating
input costs here. An arm64-macOS build and an x86_64-Linux build of identical source differed by 12
bytes, which is why this job runs on `macos-latest` while everything else runs on Ubuntu. Absolute
source paths embedded by rustc differed by the length of `/Users/<name>` versus `/home/runner`, which
is why `build-frontend.sh` exports `--remap-path-prefix`. Each was diagnosed only after a red run
that looked like somebody's fault.

## Goals / Non-Goals

**Goals:**

- Every input that decides the bytes of `crates/looqlog/assets/` is pinned to an exact version, in
  one place per input, so a toolchain release cannot turn the job red on an unrelated push.
- A toolchain bump becomes a deliberate commit that rebuilds the artifact alongside it, rather than
  an ambient event.
- A local run of `build-frontend.sh` on the wrong toolchain says so before the developer commits,
  in the same voice the binaryen check already uses.
- A failing staleness diff preserves the rejected artifact as a downloadable run artifact.
- CI stops emitting the Node-20 deprecation annotation on every run.

**Non-Goals:**

- Pinning `rustc` for `fmt`, `clippy`, `test`, `build-no-node` or `adr-0005-boundary`. Those jobs
  exist to catch drift, and freezing them would hide new-stable lints and regressions until a manual
  bump.
- Explaining the 216,854-byte `core.wasm` anomaly. The evidence is gone; this change only makes the
  next occurrence self-preserving.
- Reproducibility across host operating systems. The job stays on `macos-latest` for the reason its
  existing comment gives.
- Any rebuild of `crates/looqlog/assets/`. If the pinned versions are the ones the committed artifact
  was built with — they are, `rustc 1.97.1` and `wasm-pack 0.15.0` — the artifact does not move.

## Decisions

### D1: Pin `rustc` in the staleness job's `with:`, not in a root `rust-toolchain.toml`

A `rust-toolchain.toml` at the repository root is the more usual mechanism and would automatically
give local builds the same compiler as CI. It was rejected because `rustup` honours it for *every*
cargo invocation in the workspace, which would silently freeze `clippy`, `test` and `fmt` on 1.97.1
as well. Those five jobs are the project's early-warning system for a new stable; a pin there trades
a loud, correct signal for silence.

The alternative — `dtolnay/rust-toolchain@stable` replaced by an explicit `toolchain: 1.97.1` in the
one job that needs it — keeps the pin exactly as wide as the problem. The cost is that a local
`build-frontend.sh` no longer inherits the pin, which D2 covers.

### D2: `build-frontend.sh` warns on a `rustc` mismatch, and does not fail

The script already does exactly this for binaryen: compare `wasm-opt --version` against
`EXPECTED_BINARYEN=132`, print a two-line warning naming the consequence ("the rebuilt core.wasm will
differ from CI's and fail the staleness check"), and continue. The `rustc` check mirrors it —
`EXPECTED_RUSTC=1.97.1`, the same warning shape, the same non-fatal behaviour.

Non-fatal is deliberate and consistent: the script's job is to rebuild the artifacts, and a developer
on a different toolchain who wants to inspect the output should get it plus a warning, not a refusal.
The gate is CI, not the script.

Keeping the expected versions as two shell constants in `build-frontend.sh`, duplicated in the
workflow, was weighed against a shared file both read. Duplication won: a shared env file is a third
thing to keep in sync for two integers, and the script's warning text already exists to catch the
drift that duplication risks. The workflow gains a comment naming `build-frontend.sh`'s constant as
the other copy.

### D3: `--locked` reaches cargo through `wasm-pack build -- --locked`

`wasm-pack` forwards arguments after `--` to the underlying `cargo build`. Adding it makes a stale
or updatable `Cargo.lock` a hard failure at build time instead of a silent resolution that changes
the artifact. The workspace lock is committed, so on a clean tree this changes nothing; it only
removes the path where it could.

### D4: Capture the rejected artifact with `upload-artifact`, guarded by `if: failure()`

The failure branch already emits the useful *description* of a drift — sizes, differing byte count,
the first eight differing offsets, a `strings` diff — into `::error::` annotations, because job logs
on this repository need admin rights over the API while annotations are readable publicly. What it
does not do is keep the bytes.

`actions/upload-artifact@v4` with `if: failure()` on the whole `crates/looqlog/assets/` directory
fixes that at the cost of one step. It must be a separate step rather than part of the diff step,
because the diff step exits non-zero, and it must upload the *rebuilt* tree — the committed side is
recoverable from git at any time, the rebuilt side exists only inside that runner.

The retention default (90 days) is left alone: the anomaly this guards against is diagnosed within
hours of the red run or not at all.

### D5: Bump the deprecated actions in the same change

`actions/checkout@v4` and `actions/setup-node@v4` run on Node 20, which GitHub has deprecated, and
every run carries the annotation. This is unrelated to byte-exactness, and was deliberately kept out
of the earlier staleness work for that reason. It belongs here because the change is already about
"a red or noisy CI run should mean something", the edits are in the same three files, and leaving a
permanent warning annotation next to a job whose failure output *is* annotations makes the real
signal harder to find.

`@v5` of both actions is a runtime bump (Node 24) with no input changes for the way this repository
uses them: `checkout` is used bare, `setup-node` only with `node-version`.

## Risks / Trade-offs

**The pinned `rustc` goes stale and nobody notices** → The unpinned jobs (`test`, `clippy`) still run
on `@stable`, so a genuine incompatibility with a newer compiler still turns CI red — just not this
job. Bumping the pin becomes part of the next release's work, and the bump commit is the natural
place to rebuild the artifact.

**Local `build-frontend.sh` and CI diverge without failing** → The warning is printed but ignorable,
so a developer can still commit an artifact built on 1.98 and get a red staleness job. That is the
same exposure the binaryen check has today, and the failure is at least immediate, attributed to
their own push, and explained by a warning they saw during the build.

**`--locked` fails a build that previously succeeded** → Only if `Cargo.lock` is out of date relative
to a `Cargo.toml` change, which is a real defect the staleness job should surface rather than absorb.
The fix is to commit the updated lock.

**`@v5` actions behave differently** → Both are widely used major bumps with no input surface change
for this repository. The failure mode is loud and immediate on the first CI run, and the revert is
one line per file.

**The artifact upload leaks something** → It uploads `crates/looqlog/assets/`, which is committed to
a public repository and shipped inside the published crate. There is nothing in it that is not
already public.
