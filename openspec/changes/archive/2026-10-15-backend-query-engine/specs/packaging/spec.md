## MODIFIED Requirements

### Requirement: Building the binary requires no JavaScript toolchain
`cargo build` and `cargo install looqlog` SHALL succeed on a machine with only a Rust
toolchain — no Node.js, no npm, no network access beyond crates.io. The compiled frontend
artifacts (JS bundle and stylesheet) SHALL therefore be committed to the repository and included
in the published crate (ADR-0008). No WebAssembly artifact SHALL be built or vendored.

#### Scenario: Node-less build
- **WHEN** `cargo build --release` runs in a container with a Rust toolchain and no Node.js
- **THEN** the build succeeds and the resulting binary serves a working page

#### Scenario: Published crate carries the artifacts
- **WHEN** `cargo package --list` is inspected
- **THEN** the JS bundle and the stylesheet appear in the file list and no `.wasm` file does

### Requirement: Vendored artifacts are verifiably current
The repository SHALL provide a single documented command that rebuilds the frontend
artifacts from source, and CI SHALL fail when the committed artifacts differ from what that
command produces.

Every tool whose version changes the bytes that command produces SHALL be pinned to an exact
version in CI, so that a new release of that tool cannot fail the check on a change that did not
touch the frontend. The rebuild command SHALL itself report when the tools available locally are not
the pinned ones, before it produces an artifact that CI would reject.

When the check does fail, the rebuilt artifacts SHALL be retained as a downloadable artifact of that
CI run, so that an unexplained drift can be inspected after the fact rather than only described in
the failure output.

#### Scenario: Stale artifact is caught
- **WHEN** a frontend source file is changed and committed without regenerating the
  artifacts
- **THEN** CI fails with a message naming the stale artifact and the command that rebuilds it

#### Scenario: Regeneration is deterministic enough to diff
- **WHEN** the rebuild command is run twice with no source change
- **THEN** the produced artifacts are byte-identical, so the CI check does not produce false
  failures

#### Scenario: A toolchain release does not fail an unrelated change
- **WHEN** a new release of Node.js or of a pinned frontend build tool becomes available and
  someone pushes a change that does not touch the frontend
- **THEN** the check still runs against the pinned versions and passes, rather than reporting drift
  attributable to the push

#### Scenario: Local rebuild on the wrong toolchain says so
- **WHEN** the rebuild command runs on a machine whose Node.js or pinned frontend build tool
  differs from the pinned version
- **THEN** it warns, naming the version it found, the version expected, and that the resulting
  artifacts will fail the CI check

#### Scenario: A failing check preserves the rejected bytes
- **WHEN** the CI check finds the committed artifacts and the rebuilt artifacts differ
- **THEN** the rebuilt artifacts are uploaded as an artifact of that run, and remain retrievable
  after the job ends

### Requirement: Release build with a recorded size
The project SHALL produce a release binary for every target in the declared target set, and each
binary's size SHALL be recorded against the budget in TDR §5 on every release, in a place that
outlives the build job. A size regression beyond the budget SHALL be an explicit decision, not a
discovery, and SHALL therefore fail the release rather than be published unnoticed.

#### Scenario: Size is recorded
- **WHEN** a release build is produced
- **THEN** the binary size for each target, and the bundle size, are recorded alongside the budget
  in the published release, not only in a transient build log

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
  and serves the page, and a line supplied on stdin becomes queryable by an authenticated
  WebSocket client

#### Scenario: Environment-dependent failure is caught before release
- **WHEN** something works only because of a development-machine artefact
- **THEN** the clean-machine checks fail and no binary for that target is published

#### Scenario: Every target is held to the same checks
- **WHEN** the checks for two different targets are compared
- **THEN** they assert the same things, so that "smoke-tested" means one thing across the release
