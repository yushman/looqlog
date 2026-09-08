## Why

On 2026-08-20 the Release workflow ran twice against the `v0.2.0` tag and reported success
both times. No `v0.2.0` release existed. `releases/tags/v0.2.0` returned `Not Found` for
twelve days, `releases/latest` kept resolving to `v0.1.0`, and both READMEs meanwhile told
readers to `curl` an asset name that returned 404 while calling the one working install path
"not published yet". The gap was found on 2026-09-01 by someone writing a launch article, not
by CI and not by the workflow that was supposed to have shipped the thing.

The `packaging` spec says a `v*` tag publishes downloadable binaries. The workflow currently
proves only that its steps exited zero. A green job is not a shipped release, and nothing in
the pipeline asks the one cheap question that distinguishes them: does the release actually
exist, and does it carry the assets we just uploaded?

The specific failure shape matters for the fix. The 404 came from an asset *name* mismatch —
`latest` pointed at a release whose assets were named `looq-*` from before the rename, while
the README asked for `looqlog-*`. Counting assets would not have caught that. Comparing names
would.

## What Changes

- Add a verification step to the `publish` job in `.github/workflows/release.yml`, running
  after `softprops/action-gh-release`.
- The step queries `/repos/${{ github.repository }}/releases/tags/${{ github.ref_name }}` with
  the workflow's own `GITHUB_TOKEN` and fails the job when any of these hold:
  - the release does not exist for that tag;
  - the release exists but is a draft;
  - the set of asset names on the release differs from the set of file names in
    `release-assets/` — the directory the release was created from — in either direction.
- Tolerate GitHub's read-after-write lag with a bounded retry rather than a fixed sleep, so a
  slow API does not produce a false failure and a genuinely missing release still fails fast.
- Emit the mismatch as `::error::` annotations naming the missing and unexpected assets, so
  the cause is readable without job-log access (the same constraint that shaped
  `frontend-artifact-staleness`).

Not in scope: recreating or repairing a release the check finds broken, retrying the publish,
or verifying that each asset downloads. The step reports; a human decides.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `packaging`: the "A version tag publishes downloadable binaries" requirement gains the
  obligation that the workflow verify its own outcome against the API before reporting
  success, including that the published asset names match what was uploaded.

## Impact

- `.github/workflows/release.yml` — the `publish` job only. No change to `build`,
  `check-version`, or any other workflow.
- `openspec/specs/packaging/spec.md` — via this change's delta.
- Needs `contents: write` on `GITHUB_TOKEN`, which the workflow already declares at the top
  level for the release creation itself; read access to the release is covered by it.
- No Rust, TypeScript or vendored-artifact change. No user-facing behaviour change, so
  neither README needs updating — the workflow's *contract* is unchanged, only its
  willingness to confirm it was met.
