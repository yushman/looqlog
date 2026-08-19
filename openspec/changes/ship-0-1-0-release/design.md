## Context

The MVP is built and archived; what is missing is the act of shipping it. Two of
`packaging`'s existing requirements — "Release build with a recorded size" and
"Verification from a binary that never saw the dev machine" — were written expecting a
human with a spare Linux box. `release-hardening` could not satisfy either in a macOS-arm64
sandbox with no Docker, no VM and no cross-linker, and recorded both as owed. A CI runner
supplies both missing things at once: it cross-compiles, and it is by construction a machine
with no development environment.

Relevant current state:

- `.github/workflows/ci.yml` already runs `cargo build --release` on `ubuntu-latest` and
  throws the artifact away. It also has a `frontend-artifact-staleness` job that rebuilds the
  frontend and fails if `crates/looq/assets/` drifted, so a tag build can trust the vendored
  artifacts without running `wasm-pack` itself (ADR-0008).
- The dependency tree is pure Rust — `axum`, `tokio`, `clap`, `rand`, `serde`, `tracing`,
  `webbrowser`. No TLS, no `openssl`, no C libraries. musl needs a linker but no C shims.
- `crates/looq/src/` contains no `target_os`, no `Command::new` and no unix-only path;
  browser launching is delegated to `webbrowser` 1.2.4, which supports Windows.
- The local macOS arm64 release binary measures 2,392,608 bytes (~2.28 MiB) against TDR §5's
  ~10 MB budget — over 4× of headroom.

## Goals / Non-Goals

**Goals:**
- A `v*` tag produces a GitHub Release with downloadable binaries for four targets.
- Every published binary is smoke-tested on its own runner before it is published.
- Every published binary's size is recorded against the TDR §5 budget, automatically.
- `Cargo.toml` metadata and both READMEs point at a repository that exists.
- The repository carries the MIT license text it already claims.

**Non-Goals:**
- Publishing to crates.io. Irreversible, needs the maintainer's token, and the `looq` name
  being free today does not make claiming it this change's job.
- Package managers (Homebrew, AUR, winget), install scripts, signing, notarization.
- Any change to program behaviour. If a smoke test fails, the fix belongs to this change, but
  no feature is added on the way out.
- Reproducible/deterministic builds beyond what the vendored-artifact check already gives.

## Decisions

### D1. Tag-triggered workflow, not manual upload

The release workflow triggers on `push: tags: ['v*']`. The tag is the single source of truth
for what shipped, and the artifacts are traceable to a commit without anyone remembering to
upload them.

*Alternative rejected — `workflow_dispatch` with a version input:* lets the built version and
the tagged commit disagree, which is precisely the confusion a release should not start with.
A tag is a fact; a dispatch input is a claim.

The workflow verifies that the tag matches the workspace `version` in `Cargo.toml` and fails
otherwise. Tagging `v0.2.0` on a tree that says `0.1.0` currently produces binaries reporting
`looq 0.1.0` under a `v0.2.0` release — silently wrong in exactly the way a user would not
think to check.

### D2. musl for Linux, not gnu

`x86_64-unknown-linux-musl` links statically, so the binary runs on any distribution
regardless of its glibc version. The usual cost of musl — a slower allocator, and pain from C
dependencies — does not apply here: there are no C dependencies, and this process's hot path
is in the browser's WASM, not in the binary (ADR-0002). The binary is a static file server
and a stdin pump.

*Alternative rejected — `x86_64-unknown-linux-gnu`:* a binary built on `ubuntu-latest` carries
that image's glibc floor, and "GLIBC_2.39 not found" on an older distribution is the classic
first-download failure. It would also be a failure this project's own CI could never catch,
since CI runs on the same image that built it.

`musl-tools` on `ubuntu-latest` plus `rustup target add` is sufficient; `cross` and its Docker
requirement buy nothing here.

### D3. Each target is smoke-tested on its own runner, and a failure blocks publication

Building an artifact proves it links. It does not prove it runs. Each job, after building,
runs the binary it just produced and asserts:

1. `looq --version` prints the version matching the tag.
2. `looq --help` names every flag in TDR §6.
3. The server binds, serves `/` with a non-empty page, and serves the embedded `core.wasm`
   with a `200`.
4. A line piped into stdin mode reaches a WebSocket client.

Publication happens in a final job that depends on all four build jobs, so a target that
builds but does not run never reaches the Releases page.

**Constraint discovered while designing this, which shapes what the smoke test can be:** mode
selection is `mode_for(stdin_flag_passed, stdin_is_terminal)` in `crates/looq/src/cli.rs` —
with no TTY, the binary chooses **stdin mode regardless of a path argument**. On a CI runner
stdin is never a TTY, and `< /dev/null` does not make it one. So `looq app.log` on a runner
does not exercise file mode, and a smoke test written as if it did would silently assert
against the wrong mode. Allocating a pty (`script -q`) would work on Linux and macOS and has
no Windows equivalent, which would make the check inconsistent across the target set for the
one platform that most needs checking.

The smoke test therefore asserts what is genuinely reachable headlessly on all four targets —
the four checks above, which cover the binary's whole job of serving the embedded UI and
pumping stdin. File-mode behaviour is client-side by construction (ADR-0002/ADR-0007): the
backend's entire file-mode duty is printing a hint, and it never reads the file. What a pty
would add is coverage of the hint string, not of parsing.

*Alternative rejected — pty on unix, skip on Windows:* an uneven gate is worse than an honest
one, because the summary would read "smoke-tested" for all four while meaning two different
things.

The checks live in one script run with `shell: bash` on every runner, including Windows, where
GitHub provides Git Bash. One script is what makes the "same checks for every target"
requirement enforceable — a bash script plus a PowerShell translation would be two things
drifting apart, and the drift would show up as a target that is less tested than the summary
claims.

### D4. The size check fails the build on a budget miss

Each job measures its binary and writes the number into the job summary and into the release
body. The Linux x86_64 job additionally fails if the binary exceeds TDR §5's ~10 MB, so a
regression is an explicit decision rather than a discovery — which is what `packaging`'s
"Budget miss is deliberate" scenario already requires. At 2.28 MiB measured locally there is
over 4× of headroom, so this gate will not fire spuriously.

*Alternative rejected — record without enforcing:* a recorded number nobody is forced to look
at is how a budget quietly stops being a budget.

### D5. Repository metadata points at the real remote

`Cargo.toml`'s `repository` and both READMEs move from `looq-dev/looq` (does not exist) to
`yushman/openlogviewer` (the actual remote, confirmed via `git ls-remote`). This keeps the
crate name `looq` per ADR-0006 while the repository keeps its current name — ADR-0006 governs
the project, binary and crate names, and says nothing about the hosting path, so no ADR is
being contradicted.

*Alternative considered — renaming the GitHub repository to `looq`:* tidier, and GitHub
redirects the old path, but it needs an action outside this repository and blocks nothing.
Worth doing before any public announcement; not worth blocking the release on.

### D6. The devlog entry is written by hand, the release body by the workflow

The workflow does not commit to `docs/devlog.md`. A workflow that pushes to `main` on tag
needs write permissions and creates a commit nobody reviewed, for a file whose whole value is
that a human wrote down what they saw. The workflow publishes the sizes into the release body
where they are permanent and public; the devlog entry for the first release is written as a
task in this change, with the numbers the workflow produced.

### D7. The Intel macOS binary is built on a native Intel runner, not on macos-latest

Found while reviewing the implemented workflow, not while writing this design. `macos-latest`
maps to macOS 15 on arm64. Building `x86_64-apple-darwin` there works — the Apple toolchain
cross-compiles happily — but D3 requires the binary to be *run* on the machine that built it,
and an x86_64 binary on an arm64 host needs Rosetta 2. The `macos-15-arm64` runner image does
not list Rosetta among its installed software, so that run would have failed with `Bad CPU type
in executable` — on the tag push, after the other three targets had already built. The Intel
target therefore builds on `macos-15-intel`, one of the native Intel runner labels GitHub still
offers.

*Alternative rejected — install Rosetta in the job* (`softwareupdate --install-rosetta
--agree-to-license`): it would probably work, but then the smoke test measures a translated
binary rather than the artifact a user runs natively — a weaker claim than the one both READMEs
make ("smoke-tested on its own platform"). A native runner gives the stronger claim for the same
effort.

*Alternative rejected — drop the Intel macOS target:* it was explicitly chosen for this release,
and a native runner exists, so there is nothing to trade away.

This is why the two macOS targets deliberately do not share a runner label, which would
otherwise look like an oversight worth "tidying up".

## Risks / Trade-offs

- **Windows ships verified only by machine.** No human has ever run this project on Windows.
  The evidence that it works is indirect (`webbrowser` supports it, no unix-only code, the
  test suite and smoke test pass on the runner). → The smoke test is the same four assertions
  as every other target, so it is not held to a lower standard; and the README states which
  platforms have been exercised by hand. If Windows fails to build or smoke-test, the honest
  response is to drop it from the target set for 0.1.0 rather than publish it untested — a
  decision to bring back to the user, not to make silently.
- **The runner is a clean machine, but always the same clean machine.** It satisfies "no
  development environment" while not covering "an old distribution", "a locale that is not
  UTF-8", or "a user without a browser installed". → musl (D2) removes the largest of these.
  The rest are not closed by this change and should not be described as closed.
- **A tag is easy to push and hard to unpublish.** A bad `v0.1.0` cannot be re-pointed without
  deleting a public tag and release. → The version/tag consistency check (D1) plus the smoke
  gate (D3) catch the likely mistakes before anything is published; the publish job runs last.
- **First `--help` assertion in the smoke test duplicates what CLI unit tests already cover.**
  Mild redundancy. → Deliberate: the unit test proves the code parses flags, the smoke test
  proves *this artifact* does. That is the distinction the whole job exists to make.

## Migration Plan

No migration — nothing is deployed today. Rollback for a bad release is deleting the GitHub
Release and its tag, then tagging a corrected commit; nothing depends on 0.1.0 yet, and
nothing is published to a registry that would make a version permanent (which is a further
argument for D-non-goal on crates.io in this change).

## Open Questions

- Whether to rename the GitHub repository to `looq` before announcing publicly (D5). Does not
  block this change; the metadata is correct either way at the moment it is written.
- Whether 0.1.0 should be marked as a pre-release on GitHub. Leaning no — every P0 and P1 in
  PRD §6 is implemented and the number itself communicates maturity — but it is the
  maintainer's call and is one checkbox to change afterwards.
