## ADDED Requirements

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

## MODIFIED Requirements

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
