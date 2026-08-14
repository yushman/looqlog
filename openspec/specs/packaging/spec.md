## Purpose

The `packaging` capability covers how the frontend artifacts get into the binary and
into a published crate that builds without a JavaScript toolchain (ADR-0008).

## Requirements

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

### Requirement: Release build with a recorded size
The project SHALL produce a release binary for Linux x86_64 at minimum, and its size SHALL be
recorded against the budget in TDR §5 on every release. A size regression beyond the budget SHALL
be an explicit decision, not a discovery.

#### Scenario: Size is recorded
- **WHEN** a release build is produced
- **THEN** the binary size and the WASM and bundle sizes are recorded alongside the budget

#### Scenario: Budget miss is deliberate
- **WHEN** the binary exceeds the budget
- **THEN** the release either reduces it or documents the new number and why it is accepted

### Requirement: Verification from a binary that never saw the dev machine
Before release, the three PRD flows SHALL be run against a downloaded or freshly built binary on
a machine or container that has no development environment, following a documented checklist.

#### Scenario: Clean-machine run
- **WHEN** the release binary is run on a clean machine
- **THEN** Flow 1 (open a file), Flow 2 (live tail) and Flow 3 (filter and share the URL) all
  complete

#### Scenario: Environment-dependent failure is caught before release
- **WHEN** something works only because of a development-machine artefact
- **THEN** the clean-machine run fails and the release is blocked until it is fixed

### Requirement: Both READMEs ship complete and in sync
`README.md` and `README.ru.md` SHALL both describe installation, the three flows, every CLI flag,
the privacy asymmetry between file and stdin modes, and the known limitations, and SHALL be
updated in the same commit whenever user-facing behaviour changes.

#### Scenario: Content parity
- **WHEN** the two READMEs are compared
- **THEN** they describe the same behaviour, flags and limitations

#### Scenario: Privacy wording differs by mode
- **WHEN** the READMEs describe file mode and stdin mode
- **THEN** file mode is described as never leaving the browser and stdin mode as crossing a local
  process boundary, in different words
