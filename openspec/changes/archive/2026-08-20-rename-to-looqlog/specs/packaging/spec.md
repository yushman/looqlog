## MODIFIED Requirements

### Requirement: Building the binary requires no JavaScript toolchain
`cargo build` and `cargo install looqlog` SHALL succeed on a machine with only a Rust
toolchain — no Node.js, no npm, no `wasm-pack`, no network access beyond crates.io. The
compiled frontend artifacts SHALL therefore be committed to the repository and included in
the published crate (ADR-0008).

#### Scenario: Node-less build
- **WHEN** `cargo build --release` runs in a container with a Rust toolchain and no Node.js
- **THEN** the build succeeds and the resulting binary serves a working page

#### Scenario: Published crate carries the artifacts
- **WHEN** `cargo package --list` is inspected
- **THEN** the JS bundle and `core.wasm` appear in the file list

### Requirement: Frontend sources are not shipped to end users
The published crate SHALL contain the built artifacts and the Rust sources, and SHALL NOT
require `node_modules` or frontend sources to be present at build time.

#### Scenario: Install from crates.io
- **WHEN** `cargo install looqlog` runs against the published crate on a clean machine
- **THEN** the install succeeds and `looqlog --version` runs
