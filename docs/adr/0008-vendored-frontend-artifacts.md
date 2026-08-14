# 0008. Vendored frontend build artifacts committed to the repository

- **Status:** Accepted
- **Date:** 2026-08-09

## Context

The backend embeds the frontend via `include_bytes!`, which needs the JS bundle and
`core.wasm` to already exist as files at `cargo build` time. TDR §5 promises
`cargo install looq` as a supported install path, and PRD §11 calls Node.js a dev-only
dependency, not a runtime one. Both promises only hold if the built frontend artifacts
are committed to the repository and shipped inside the published crate — otherwise
`cargo build`/`cargo install` would need to invoke a JS toolchain that most Rust-only
machines don't have.

## Decision

The compiled frontend artifacts (`index.html`, the JS glue, `core.wasm`) live in
`assets/` and are committed to the repository and included in the published crate.
`crates/looq` embeds them with `include_bytes!`/`include_str!` at compile time. A
single documented command (`scripts/build-frontend.sh`) regenerates `assets/` from
`web/` sources and `crates/looq-wasm`, and CI fails if the committed artifacts differ
from a fresh rebuild.

## Alternatives considered

### `build.rs` invoking `wasm-pack`/npm at build time

Always fresh, nothing to commit, no drift possible. Rejected: every `cargo build` or
`cargo install` would then require Node.js, `wasm-pack`, and a network fetch on a
machine that TDR §5/PRD §11 promise needs only the Rust toolchain — several minutes
and a new failure class (npm registry unreachable, Node version mismatch) added to
what should be a single, boring `cargo install`.

### Skip crates.io, ship prebuilt binaries only

Avoids the vendoring problem entirely by never asking `cargo build` to produce a
working binary from source. Rejected: contradicts TDR §5's explicit `cargo install
looq` install path and cuts off exactly the audience — Rust developers — most likely
to reach for `cargo install` first.

## Consequences

**Good:** `cargo build --release` and `cargo install looq` work with only a Rust
toolchain, no network access beyond crates.io, matching TDR §5 and PRD §11 exactly.
The published crate is self-contained.

**Bad / accepted cost:** the repository carries build output, which drifts from its
source unless something actively checks it — a stale bundle would otherwise ship
silently, which is precisely the failure class this project's testing rules single
out (CLAUDE.md Testing section). This is only safe with the CI check from D2/task 8.3;
without it, this decision would be a liability rather than a convenience.

**What would make us revisit:** if the CI staleness check proves unreliable (for
example, if the wasm-pack/rustc toolchain cannot produce byte-identical output across
runs even after pinning versions) to the point of chronic false failures — would
justify falling back to a normalized-hash comparison instead of raw bytes, recorded as
an amendment rather than abandoning vendoring itself.
