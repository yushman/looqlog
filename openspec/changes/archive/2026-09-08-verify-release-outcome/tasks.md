## 1. Add the verification step

- [x] 1.1 In `.github/workflows/release.yml`, add a step to the `publish` job immediately after
  `Create GitHub Release`, named so the failure is identifiable from the job list alone (e.g.
  `Verify the release exists and carries the uploaded assets`).
  Evidence: step `Verify the release exists and carries the uploaded assets` added directly
  after the `Create GitHub Release` step in the `publish` job.
- [x] 1.2 Give it `env: GH_TOKEN: ${{ secrets.GITHUB_TOKEN }}` and `shell: bash`. Do not add a
  `permissions:` block — the workflow already declares `contents: write` at the top level
  (design D5, Risks).
  Evidence: step has `env: GH_TOKEN: ${{ secrets.GITHUB_TOKEN }}` and `shell: bash`; no
  `permissions:` block added.
- [x] 1.3 Query `gh api repos/${{ github.repository }}/releases/tags/${{ github.ref_name }}`.
  Treat only a `404` as retryable: poll at most 5 times with ~3s between attempts, and break
  out of the loop the moment a release is returned (design D2).
  Evidence: `for attempt in 1 2 3 4 5; do gh api "repos/${REPO}/releases/tags/${TAG}" ...`,
  breaks on exit code 0, retries only when the JSON body's `.status` is `"404"`, `sleep 3`
  between attempts, any non-404 failure exits immediately without retrying.
- [x] 1.4 On exhaustion, fail with an `::error::` line naming the tag that has no release.
  Evidence: `::error::no release exists for tag ${TAG} after 5 attempts — ...`.

## 2. Check the release is usable

- [x] 2.1 Fail when `.draft` is `true`, with an `::error::` line saying a draft is not reachable
  by anyone following the install instructions (design D3).
  Evidence: `::error::release for tag ${TAG} exists but is a draft — not reachable by anyone
  following the install instructions`.
- [x] 2.2 Build the expected set from `release-assets/` — the directory the release was created
  from — with `basename`, sorted. Do **not** reconstruct names from the target matrix
  (design D1).
  Evidence: `EXPECTED="$(find release-assets -type f -exec basename {} \; | sort)"` — no
  reference to `matrix.target` or a reconstructed asset-name expression anywhere in the step.
- [x] 2.3 Build the actual set from `.assets[].name` of the API response, sorted.
  Evidence: `ACTUAL="$(echo "$RELEASE" | jq -r '.assets[].name' | sort)"`.
- [x] 2.4 Compare the two sets in both directions. Missing and unexpected assets are both
  failures.
  Evidence: `comm -23`/`comm -13` against `EXPECTED`/`ACTUAL`; step exits 1 if either
  `MISSING` or `UNEXPECTED` is non-empty. Verified by the task 4.2 runs below.

## 3. Make the failure readable without job logs

- [x] 3.1 Emit one `::error::` line per missing asset and one per unexpected asset, each naming
  the asset. Follow the voice of the existing `::error::` output in
  `frontend-artifact-staleness` in `ci.yml` — state the consequence, not just the fact
  (design D4).
  Evidence: one `::error::missing from the release: <name> — uploaded by this run, absent
  from what the API reports` per missing name, one `::error::unexpected on the release:
  <name> — on the release but not among this run's uploads` per unexpected name.
- [x] 3.2 On success, print the confirmed release name and the asset count to the step summary
  or stdout, so a passing run records what it verified rather than being silent.
  Evidence: `echo "confirmed: release '...' for tag ${TAG} carries ${COUNT} asset(s), matching
  release-assets/ exactly" | tee -a "$GITHUB_STEP_SUMMARY"`.
- [x] 3.3 Add a comment above the step explaining why it exists, referencing the 2026-09-01
  devlog entry: two green runs, no release for twelve days, and a README documenting a 404.
  Match the explanatory-comment style the other jobs in this repo already use.
  Evidence: comment block above the step names the 2026-09-01 incident (two green runs,
  twelve days, `Not Found`, the README 404) in the same voice as the existing D1–D7 comments
  in this file and `frontend-artifact-staleness` in `ci.yml`.

## 4. Verify

- [x] 4.1 YAML validity: `python3 -c "import yaml,sys; [yaml.safe_load(open(f)) for f in sys.argv[1:]]" .github/workflows/*.yml`.
  Evidence: ran against `.github/workflows/*.yml`, no exception raised (output: `YAML OK`).
- [x] 4.2 Exercise the set-comparison logic outside CI: extract it into a shell snippet and run
  it locally against three hand-made cases — sets equal, one name missing, one name
  unexpected — confirming the exit status and the `::error::` lines for each. Record the actual
  output; this is the only part of the step that can be tested without pushing a tag.
  Evidence: extracted to a standalone script and run against three cases with four hand-made
  `looqlog-0.2.0-*` names as the expected set (matching the real v0.2.0 release). Actual output:
  ```
  === Case 1: sets equal ===
  confirmed: release carries 4 asset(s), matching release-assets/ exactly
  exit=0

  === Case 2: one asset missing (windows uploaded but not on release) ===
  ::error::release does not carry exactly the assets this run uploaded
  ::error::missing from the release: looqlog-0.2.0-x86_64-pc-windows-msvc.exe — uploaded by this run, absent from the release the API reports
  exit=1

  === Case 3: one unexpected asset (stale looq-* name on release) ===
  ::error::release does not carry exactly the assets this run uploaded
  ::error::unexpected on the release: looq-0.1.0-x86_64-unknown-linux-musl — on the release but not among this run's uploads
  exit=1
  ```
  Additionally, `bash -n` against the actual step script extracted verbatim from
  `release.yml` (via a small `pyyaml` script pulling the `run:` block) passed clean.
- [x] 4.3 Confirm against the real API that the query shape and the `jq` paths are right, using
  the existing published release:
  `gh api repos/yushman/looqlog/releases/tags/v0.2.0 --jq '.draft, ([.assets[].name] | sort | join(","))'`
  should report `false` and the four `looqlog-0.2.0-*` names. Record the output.
  Evidence — `gh` (2.98.0, authenticated as `yushman`) was available on this machine, so the
  exact `gh`-specific invocation was run for real:
  ```
  false
  looqlog-0.2.0-aarch64-apple-darwin,looqlog-0.2.0-x86_64-apple-darwin,looqlog-0.2.0-x86_64-pc-windows-msvc.exe,looqlog-0.2.0-x86_64-unknown-linux-musl
  ```
  Matches the expected shape exactly.
- [x] 4.4 Confirm the 404 branch is reachable and shaped as expected:
  `gh api repos/yushman/looqlog/releases/tags/v9.9.9` returns non-zero. Record it.
  Evidence:
  ```
  $ gh api repos/yushman/looqlog/releases/tags/v9.9.9
  {"message":"Not Found","documentation_url":"https://docs.github.com/rest/releases/releases#get-a-release-by-tag-name","status":"404"}
  gh: Not Found (HTTP 404)
  exit=1
  ```
  stdout carries `"status":"404"`, which is exactly the field the step's retry loop reads via
  `jq -r '.status // empty'` to decide "retry" vs. "fail now".
- [x] 4.5 Confirm no other job or workflow was touched: `git diff --stat` shows only
  `.github/workflows/release.yml` plus this change's own files.
  Evidence: `git diff --stat` shows only `.github/workflows/release.yml`,
  `docs/devlog.md`, and `openspec/changes/verify-release-outcome/tasks.md` — see the
  implementation report.

## 5. Documentation

- [x] 5.1 Strike the "release workflow reports success without verifying its own outcome" item
  under `## Ideas for later` in `docs/devlog.md`, following the `~~...~~ **Done, <date>.**`
  form the neighbouring struck items use.
  Evidence: item struck with `**Done, 2026-09-08.**` in `docs/devlog.md`.
- [x] 5.2 Append a devlog entry: what the step checks, why it compares against
  `release-assets/` rather than the target matrix, and the recorded outputs from tasks 4.2–4.4.
  State plainly that the step cannot be end-to-end tested until the next `v*` tag is pushed —
  that is an honest limitation of the change, not an omission.
  Evidence: `## 2026-09-08 — verify-release-outcome` entry appended to `docs/devlog.md`.
- [x] 5.3 Confirm no README change is needed — the workflow's contract is unchanged, only its
  willingness to confirm it was met — and say so in the devlog entry rather than leaving it
  unstated.
  Evidence: stated explicitly in the devlog entry.

## 6. Close out

- [x] 6.1 Run `openspec validate verify-release-outcome --strict` and fix anything it reports.
  Evidence: see implementation report for the actual command output.
