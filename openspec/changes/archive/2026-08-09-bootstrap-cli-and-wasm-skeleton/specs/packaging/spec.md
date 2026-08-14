## ADDED Requirements

### Requirement: Building the binary requires no JavaScript toolchain
`cargo build` and `cargo install looq` SHALL succeed on a machine with only a Rust
toolchain — no Node.js, no npm, no `wasm-pack`, no network access beyond crates.io. The
compiled frontend artifacts SHALL therefore be committed to the repository and included in
the published crate (ADR-0008).

#### Scenario: Node-less build
- **WHEN** `cargo build --release` runs in a container with a Rust toolchain and no Node.js
- **THEN** the build succeeds and the resulting binary serves a working page

#### Scenario: Published crate carries the artifacts
- **WHEN** `cargo package --list` is inspected
- **THEN** the JS bundle and `core.wasm` appear in the file list

### Requirement: Vendored artifacts are verifiably current
The repository SHALL provide a single documented command that rebuilds the frontend
artifacts from source, and CI SHALL fail when the committed artifacts differ from what that
command produces.

#### Scenario: Stale artifact is caught
- **WHEN** a frontend source file is changed and committed without regenerating the
  artifacts
- **THEN** CI fails with a message naming the stale artifact and the command that rebuilds it

#### Scenario: Regeneration is deterministic enough to diff
- **WHEN** the rebuild command is run twice with no source change
- **THEN** the produced artifacts are byte-identical, so the CI check does not produce false
  failures

### Requirement: Frontend sources are not shipped to end users
The published crate SHALL contain the built artifacts and the Rust sources, and SHALL NOT
require `node_modules` or frontend sources to be present at build time.

#### Scenario: Install from crates.io
- **WHEN** `cargo install looq` runs against the published crate on a clean machine
- **THEN** the install succeeds and `looq --version` runs
