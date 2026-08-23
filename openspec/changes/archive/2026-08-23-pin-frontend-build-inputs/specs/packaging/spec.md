## MODIFIED Requirements

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
- **WHEN** a new stable release of the Rust toolchain or of `wasm-pack` becomes available and someone
  pushes a change that does not touch the frontend
- **THEN** the check still runs against the pinned versions and passes, rather than reporting drift
  attributable to the push

#### Scenario: Local rebuild on the wrong toolchain says so
- **WHEN** the rebuild command runs on a machine whose Rust toolchain or `wasm-opt` differs from the
  pinned version
- **THEN** it warns, naming the version it found, the version expected, and that the resulting
  artifacts will fail the CI check

#### Scenario: Dependency resolution cannot change the artifact silently
- **WHEN** the rebuild command runs with a `Cargo.lock` that would need updating
- **THEN** the build fails rather than resolving to different dependency versions and producing a
  different artifact

#### Scenario: A failing check preserves the rejected bytes
- **WHEN** the CI check finds the committed artifacts and the rebuilt artifacts differ
- **THEN** the rebuilt artifacts are uploaded as an artifact of that run, and remain retrievable
  after the job ends
