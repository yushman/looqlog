## MODIFIED Requirements

### Requirement: A version tag publishes downloadable binaries
Pushing a tag matching `v*` SHALL build the binary for every target in the declared target
set and publish those binaries as assets on a GitHub Release for that tag. The declared
target set for 0.1.0 is `x86_64-unknown-linux-musl`, `aarch64-apple-darwin`,
`x86_64-apple-darwin` and `x86_64-pc-windows-msvc`. The Linux binary SHALL be statically
linked, so that it runs independently of the host distribution's C library version.

The release workflow SHALL NOT report success on the strength of its own steps alone. Before
it finishes, it SHALL confirm against the hosting platform that a release for the pushed tag
exists, is published rather than a draft, and carries exactly the assets the run uploaded —
compared by asset name, since a name that differs from what the documentation asks for is
indistinguishable from a missing release to anyone following the install instructions. A run
that cannot confirm this SHALL fail, and SHALL name what is missing or unexpected in output
readable without privileged access to job logs.

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

#### Scenario: A release that was never created fails the run
- **WHEN** every publishing step reports success but no release exists for the pushed tag
- **THEN** the workflow run fails rather than reporting success, and its output names the tag
  that has no release

#### Scenario: A draft is not a published release
- **WHEN** a release exists for the tag but is a draft
- **THEN** the workflow run fails, because a draft is not reachable by anyone following the
  install instructions

#### Scenario: Asset names are verified, not counted
- **WHEN** the release carries the expected number of assets but one of them is named
  differently from the file the run uploaded
- **THEN** the workflow run fails and names both the missing asset and the unexpected one

#### Scenario: Confirmation tolerates a lagging API
- **WHEN** the release has just been created and the platform has not yet made it readable
- **THEN** the confirmation retries for a short bounded period before failing, so that read
  latency does not fail a run whose release is present
