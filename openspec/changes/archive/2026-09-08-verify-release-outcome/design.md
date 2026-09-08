## Context

`.github/workflows/release.yml` ends like this: the `build` matrix produces one binary per
target, stages it as `dist/<ASSET_NAME>` alongside a `<ASSET_NAME>.size` file, and uploads
both as a run artifact. The `publish` job downloads every artifact, copies the binaries (but
not the `.size` files) into `release-assets/`, assembles `release-body.md` from the sizes, and
hands `release-assets/*` to `softprops/action-gh-release@v2`.

That is the last thing the workflow does. Whatever the action reports is the workflow's final
word.

```
build (matrix ×4) ──> upload-artifact ──> publish ──> release-assets/ ──> action-gh-release
                                                              │                    │
                                                              │                    ▼
                                                              │            exit 0 = success
                                                              │
                                                              └── the set of names that
                                                                  SHOULD be on the release
```

On 2026-08-20 that final word was "success", twice, and no release existed. The left branch of
that diagram — the directory the release was built from — is sitting right there in the job,
and nothing compares it to what the API actually holds.

## Goals / Non-Goals

**Goals:**

- A release workflow that reports success only when a release for the tag exists, is
  published, and carries exactly the assets the job uploaded.
- A failure that names what is missing or unexpected, readable from annotations alone.
- No false failures from GitHub's read-after-write lag.

**Non-Goals:**

- Repairing anything. If the check fails, the run goes red and a human looks; the workflow
  does not retry the publish or recreate the release. The 2026-09-01 fix (re-running the
  workflow after the artifacts were confirmed retained) was a judgement call that depended on
  knowing *why* the release was missing, which this step cannot determine.
- Verifying that assets download, or that their bytes match. Asset presence by name is what the
  incident needed; byte verification would mean re-downloading four binaries on every release
  to guard a failure mode never observed.
- Guarding against a release deleted *after* the workflow finishes. That is what happened here,
  most likely, and no in-workflow check can prevent it — the step proves the release existed at
  the end of the run, which is the strongest claim the workflow is in a position to make.
  Ideas for later can carry a periodic check if that recurs.

## Decisions

### D1: Compare against `release-assets/`, not against the target matrix

The obvious source of truth for expected asset names is the matrix — reconstruct
`looqlog-<version>-<target>` for each entry and check the API for those. It was rejected:
that recomputes the name a second time, from the same inputs, in a second place. If the naming
expression in `Stage release asset` is ever wrong, the check would be wrong in exactly the same
way and would pass.

`release-assets/` is the directory the release was actually created from. Comparing the API's
asset list to `ls release-assets/` compares two *different* things — what we handed over and
what landed — which is the comparison that can fail usefully. It also needs no knowledge of the
matrix, so adding a target requires no change here.

The comparison is set-equality in both directions. A missing asset is the incident's shape; an
unexpected extra asset means the release was not created from this run, which is equally worth
failing on.

### D2: Bounded retry, not a sleep

The release is created moments before the check runs, and the REST API is read-after-write
eventually consistent. A fixed `sleep 10` would be both too long on the happy path and not
guaranteed on a slow one.

Instead: poll `releases/tags/<tag>` a small number of times with a short delay, treating only
`404` as retryable. A release that exists but is a draft, or exists with the wrong assets, is a
real failure and must not be retried into a timeout — those fail immediately. This keeps the
happy path at roughly one request and still fails fast when the release is genuinely absent.

### D3: Fail on `draft: true` explicitly

`release.yml` passes no `draft` input, and `action-gh-release` publishes by default, so a draft
should be unreachable. It is checked anyway because "should be unreachable" is precisely the
assumption that failed here: two successful runs and no release was also unreachable by the same
reasoning. A draft satisfies "the release exists" while satisfying none of what a user needs,
and it is one `jq` field to rule out.

### D4: `::error::` annotations carry the diff

Job logs on this repository need admin rights over the API; annotations are readable publicly.
`frontend-artifact-staleness` already dumps its byte-level diff into `::error::` lines for that
reason, and this step follows the same pattern: one line naming the tag and the failure mode,
then one line per missing asset and one per unexpected asset. A reader who can see only
annotations should be able to tell "the release is absent" from "the release is there but
`looqlog-0.3.0-x86_64-apple-darwin` never landed".

### D5: `gh` over `curl`

`gh` is preinstalled on GitHub-hosted runners and authenticates from `GH_TOKEN` without
constructing headers, which keeps the step short and keeps the token out of a command line.
`gh api` returns the same JSON as `curl`, and its non-zero exit on `404` is easy to branch on.
If `gh` ever stops being preinstalled, `curl` with `Authorization: Bearer` is a drop-in and the
step's shape does not change.

## Risks / Trade-offs

**The retry masks a slow-but-real failure** → Only `404` retries, and only a small bounded
number of times. A release that never appears still fails the run, a few seconds later than it
otherwise would.

**Set-equality is too strict if someone attaches an asset by hand mid-run** → That would mean a
human editing a release while its workflow is still running, which is not a workflow this
project has. Failing on it is the correct conservative answer, and the annotation says exactly
which asset was unexpected.

**The check passes and the release is deleted afterwards** → Out of scope by D-Non-Goals, and
explicitly the likeliest reading of the original incident. The step narrows the window from
"forever" to "after the run ended", which is the part the workflow can own.

**`GITHUB_TOKEN` permissions** → The workflow already declares `contents: write` at the top
level for release creation; reading a release needs no more than that. No new secret, no
new permission block.
