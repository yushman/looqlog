## Why

Every P0 and P1 feature in PRD §6 is implemented and archived, the workspace version is
already `0.1.0`, and `main` is pushed — but there is nothing a user can download. The
`packaging` spec already requires a Linux x86_64 release binary with a recorded size and a
verification run on a machine with no development environment; both were left owed by
`release-hardening` because that work happened in a sandbox with no Docker, no VM and no
cross-linker toolchain. A GitHub Actions runner is exactly the clean machine and the
cross-compilation environment that were missing, so the two open debts close together.

Three defects block a release that anyone could actually use, and all three are invisible
until someone tries:

- `Cargo.toml` declares `repository = "https://github.com/looq-dev/looq"` and `README.md`
  tells the reader to `git clone https://github.com/looq-dev/looq`. That repository does not
  exist. The README's own "A prebuilt release binary will be published once v0.1.0 ships
  (see Releases)" therefore points at a 404.
- There is no `LICENSE` file, though `Cargo.toml` claims `license = "MIT"`. GitHub shows the
  repository as unlicensed, which for many readers means "not usable".
- No tag, no release workflow, and CI's `cargo build --release` artifact is discarded at the
  end of the job.

## What Changes

- **Add a release workflow** triggered by a `v*` tag: cross-compile the four agreed targets,
  attach the binaries to a GitHub Release, and record each binary's size against TDR §5.
  Targets: `x86_64-unknown-linux-musl`, `aarch64-apple-darwin`, `x86_64-apple-darwin`,
  `x86_64-pc-windows-msvc`.
- **Smoke-test each built artifact on its own runner** before it is published — the runner is
  a machine that never saw this project's development environment, which is what the existing
  "Verification from a binary that never saw the dev machine" requirement asks for.
- **Fix the repository metadata** to `https://github.com/yushman/openlogviewer` in
  `Cargo.toml` and in both READMEs, so clone instructions and the Releases link resolve.
- **Add the `LICENSE` file** (MIT) matching the already-declared `license` field.
- **Record the release sizes** in `docs/devlog.md` against the TDR §5 budget, including the
  first real Linux x86_64 number this project has ever had.
- Both READMEs gain a real download-and-run install path alongside the build-from-source one.

Explicitly **not** in this change: publishing to crates.io. The name `looq` is free today,
but publishing is irreversible (a yank does not free the version) and needs the maintainer's
own token. The `cargo install looq` line in both READMEs stays marked as planned.

## Capabilities

### New Capabilities

None. Release building, size recording and clean-machine verification are already
`packaging`'s subject matter; this change tightens requirements that exist rather than
introducing a new capability.

### Modified Capabilities

- `packaging`: the release-build requirement broadens from "Linux x86_64 at minimum" to a
  declared target set whose binaries are published as downloadable artifacts; clean-machine
  verification becomes an automated gate that blocks publication rather than a manual
  pre-release checklist; and two new requirements cover package metadata resolving to the
  real repository and the license file matching the declared field.

## Impact

- **New:** `.github/workflows/release.yml`, `LICENSE`.
- **Modified:** `Cargo.toml` (`repository`), `README.md` and `README.ru.md` (clone URL,
  install section), `docs/devlog.md` (recorded sizes).
- **Unchanged:** all crate source. This change ships what is already built; if a smoke test
  fails on a target, that is a finding to fix inside this change, not a reason to alter
  behaviour on the way out the door.
- **Risk carried knowingly:** Windows is in the target set but has never been run by a human
  on this project. `webbrowser` 1.2.4 supports it and `crates/looq/src/` contains no
  `target_os`, `Command::new` or unix-only path, so it should build and pass tests — but the
  Windows artifact will ship verified only by the automated smoke test, not by a person
  performing the three PRD flows. See design.md.
