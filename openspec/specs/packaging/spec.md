## Purpose

The `packaging` capability covers how the frontend artifacts get into the binary and
into a published crate that builds without a JavaScript toolchain (ADR-0008).
## Requirements
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
- **WHEN** `cargo install looqlog` runs against the published crate on a clean machine
- **THEN** the install succeeds and `looqlog --version` runs

### Requirement: Release build with a recorded size
The project SHALL produce a release binary for every target in the declared target set, and each
binary's size SHALL be recorded against the budget in TDR §5 on every release, in a place that
outlives the build job. A size regression beyond the budget SHALL be an explicit decision, not a
discovery, and SHALL therefore fail the release rather than be published unnoticed.

#### Scenario: Size is recorded
- **WHEN** a release build is produced
- **THEN** the binary size for each target, and the WASM and bundle sizes, are recorded alongside
  the budget in the published release, not only in a transient build log

#### Scenario: Budget miss is deliberate
- **WHEN** a binary exceeds the budget
- **THEN** the release fails, and shipping that size requires either reducing it or explicitly
  recording the new number and why it is accepted

### Requirement: Verification from a binary that never saw the dev machine
Before any binary is published, the binary itself SHALL be executed on a machine with no
development environment for this project, and SHALL pass a documented set of checks. A binary that
builds but does not run SHALL NOT reach users. Because the mode a binary selects depends on whether
stdin is a terminal, these checks SHALL assert against the mode the checking environment actually
produces, and SHALL be the same set of checks for every target.

#### Scenario: Clean-machine run
- **WHEN** the freshly built binary is run on a machine that has never built this project
- **THEN** it reports the expected version, its help output names every CLI flag, its server binds
  and serves both the page and the embedded WASM, and a line supplied on stdin reaches a connected
  WebSocket client

#### Scenario: Environment-dependent failure is caught before release
- **WHEN** something works only because of a development-machine artefact
- **THEN** the clean-machine checks fail and no binary for that target is published

#### Scenario: Every target is held to the same checks
- **WHEN** the checks for two different targets are compared
- **THEN** they assert the same things, so that "smoke-tested" means one thing across the release

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

### Requirement: A version tag publishes downloadable binaries
Pushing a tag matching `v*` SHALL build the binary for every target in the declared target
set and publish those binaries as assets on a GitHub Release for that tag. The declared
target set for 0.1.0 is `x86_64-unknown-linux-musl`, `aarch64-apple-darwin`,
`x86_64-apple-darwin` and `x86_64-pc-windows-msvc`. The Linux binary SHALL be statically
linked, so that it runs independently of the host distribution's C library version.

#### Scenario: Tag produces a release
- **WHEN** a `v*` tag is pushed
- **THEN** a GitHub Release for that tag exists carrying one binary asset per declared target

#### Scenario: Linux binary does not depend on the builder's libc
- **WHEN** the published Linux binary is inspected
- **THEN** it is statically linked and names no dynamic libc version requirement

#### Scenario: Tag and package version must agree
- **WHEN** the pushed tag's version differs from the workspace `version` in `Cargo.toml`
- **THEN** the release fails before publishing anything, rather than publishing binaries whose
  `--version` output contradicts the release they are attached to

### Requirement: Package metadata resolves to the real repository
Every repository URL the project publishes SHALL point at the repository that actually hosts
it — the `repository` field in `Cargo.toml`, and every clone command or Releases link in
`README.md` and `README.ru.md`. A URL printed in user-facing documentation SHALL resolve.

#### Scenario: Clone instruction works
- **WHEN** the clone command from either README is run
- **THEN** it clones this project, rather than failing on a repository that does not exist

#### Scenario: Metadata agrees with the remote
- **WHEN** `Cargo.toml`'s `repository` is compared with the repository's own remote URL
- **THEN** they name the same repository

### Requirement: The declared license is present as a file
The repository SHALL contain a `LICENSE` file whose contents are the license named by the
`license` field in `Cargo.toml`. A declared license with no license text is not a licensed
project.

#### Scenario: License file matches the declared field
- **WHEN** `Cargo.toml`'s `license` field is compared with the `LICENSE` file
- **THEN** the file contains the full text of that license

