## 1. Crates and manifests

- [x] 1.1 `git mv crates/looq crates/looqlog`, `crates/looq-core` → `crates/looqlog-core`, `crates/looq-wasm` → `crates/looqlog-wasm` (use `git mv` so history follows)
- [x] 1.2 Update workspace `members` in the root `Cargo.toml`
- [x] 1.3 Rename the three `package.name` values and the `[[bin]] name` to `looqlog`
- [x] 1.4 Update the `looqlog-core` path dependency in `looqlog`'s and `looqlog-wasm`'s manifests
- [x] 1.5 Update the workspace `repository` field to `https://github.com/yushman/looqlog`
- [x] 1.6 `cargo build --workspace` and `cargo test --workspace` pass before touching anything else — this is the checkpoint that the Rust half is coherent

## 2. Rust sources

- [x] 2.1 Rename the three template placeholders `__LOOQ_HINT__`, `__LOOQ_MODE__`, `__LOOQ_TOKEN__` to `__LOOQLOG_*__` in `crates/looqlog/src/assets.rs` **and** in `crates/looqlog/assets/index.html`, which is the source side of the same substitution
- [x] 2.2 Sweep remaining `looq` occurrences in `crates/**/src/**` and `crates/**/tests/**` — doc comments, `--help` text, log lines, test assertions on output
- [x] 2.3 Check `crates/looqlog/src/cli.rs` for the binary name in usage/version strings that `clap` renders
- [x] 2.4 `cargo clippy --all-targets` and `cargo fmt --check` clean

## 3. Frontend sources

- [x] 3.1 Rename the twelve custom element tags to `looqlog-*`: `looq-app`, `looq-core`, `looq-detection`, `looq-diagnostics`, `looq-drop-target`, `looq-entry-detail`, `looq-entry-table`, `looq-filter-bar`, `looq-live-tail`, `looq-table`, `looq-timeline`, `looq-workspace` — both the `customElements.define` call and every `querySelector`/`innerHTML` that names them
- [x] 3.2 Update `web/index.html` and any template markup carrying those tags
- [x] 3.3 Change the theme `localStorage` key in `web/src/theme.ts` from `looq-theme`
- [x] 3.4 Update CSS selectors in `web/src/style.css` that target the old tag names
- [x] 3.5 Sweep remaining `looq` in `web/src/**` — comments, type names, worker messages
- [x] 3.6 `npm test` and `npx tsc --noEmit` pass
- [x] 3.7 Rename the ten component class identifiers `LooqApp`, `LooqDetection`, `LooqDiagnostics`, `LooqDropTarget`, `LooqEntryDetail`, `LooqEntryTable`, `LooqFilterBar`, `LooqLiveTail`, `LooqTimeline`, `LooqWorkspace` to `Looqlog*`, with their imports and type annotations (66 occurrences across 10 files). Found while verifying: a `class LooqApp` inside `looqlog-app.ts` registering `<looqlog-app>` is a half-rename, and design D2 commits to one name everywhere. `tsc --noEmit` must stay clean

## 4. Vendored artifacts

- [x] 4.1 Run `./scripts/build-frontend.sh` — do **not** text-replace `crates/looqlog/assets/assets/index.js`; it is minified generated output and the element names are compiled into it (design D4)
- [x] 4.2 Confirm the rebuilt bundle contains `looqlog-` tags and no bare `looq-` tag remains in `crates/looqlog/assets/`
- [ ] 4.3 Commit `crates/looqlog/assets/` so the vendored-artifact-staleness CI job stays green (SKIPPED by explicit instruction: no committing in this session — files are rebuilt and staged/modified in the working tree, ready for the user to commit)

## 5. Scripts and CI

- [x] 5.1 Update `scripts/build-frontend.sh` — output paths now under `crates/looqlog/assets/`
- [x] 5.2 Update `scripts/smoke-release-binary.sh` and the two `scripts/gen-*-fixture.py` (the two fixture scripts had no `looq` references — nothing to change)
- [x] 5.3 Update `.github/workflows/release.yml`: `cargo build -p looqlog`, the binary path, the asset name `looqlog-${VERSION}-${target}`, the release title and body headings
- [x] 5.4 Update `.github/workflows/ci.yml`: `cargo package --list -p looqlog`, and the ADR-0005 guard steps that run `cargo tree -p looq-core -i wasm-bindgen` / `-i web-sys` and the `std::fs` source check
- [x] 5.5 **Prove the ADR-0005 guard still fires** (design D5): temporarily add a `web-sys` dependency to `looqlog-core`, confirm the job fails, then revert. A guard naming a crate that no longer exists can pass vacuously — green is not evidence

## 6. Specs and current docs

- [ ] 6.1 Apply the seven delta specs to `openspec/specs/` (handled by `/opsx:archive`, not by hand) — NOT RUN: `/opsx:archive` was explicitly withheld from this session per instructions; left for the user to run
- [x] 6.2 Directly edit the Purpose paragraphs of `openspec/specs/cli/spec.md`, `log-parsing/spec.md` and `format-detection/spec.md` — Purpose is not a requirement, so no delta covers it
- [x] 6.3 Update `README.md` and `README.ru.md` in sync: install command, every example invocation, the pipe form, the repository URL
- [x] 6.4 Note in both READMEs that a v0.1.0 `looq` binary has no in-place upgrade path — the command is renamed, reinstall
- [x] 6.5 Update `docs/PRD.md`, `docs/TDR.md`, `docs/mvp-plan.md` — current documents

## 7. ADRs

- [x] 7.1 Set ADR-0006's status line to `Superseded by ADR-0009`. Do **not** edit its body — the reasoning it recorded was correct on the evidence available in 2026-08
- [x] 7.2 Write `docs/adr/0009-project-renamed-to-looqlog.md`: Context is the in-category collision with Looq for macOS (a Quick Look extension that reads log files and markets "Zero Data Collection, runs entirely locally") plus Looq AI holding `looq.ai`; Decision is `looqlog` everywhere; Alternatives covers why `lq` is unavailable and why `loglens` is worse than the status quo; Consequences names the pipe-length cost and the v0.1.0 upgrade break
- [x] 7.3 Update `CLAUDE.md`'s ADR list line for ADR-0006 and add ADR-0009

## 8. Deliberately not touched

- [x] 8.1 Confirm `docs/devlog.md`, the bodies of ADR-0001…0008, and `openspec/changes/archive/` (56 files) still say `looq` — they are records of what the project was called when they were written (design D3)
- [x] 8.2 Confirm the `v0.1.0` tag and its published release binaries are untouched

## 9. Verification

- [x] 9.1 `cargo test --workspace` (208 expected), `npm test` (111 expected), `tsc --noEmit`, `cargo clippy --all-targets`, `cargo fmt --check` — all pass with the same counts as before the rename (design D5: the suite is the evidence, not a zero grep count)
- [x] 9.2 Run the built binary in file mode against a fixture; confirm the page loads, the timeline and table render, and no custom element is left unupgraded in the browser console
- [x] 9.3 Run live mode: pipe a log into `looqlog --stdin` and confirm the WebSocket connects and lines arrive — a stale `__LOOQ_TOKEN__` placeholder breaks auth silently while the page still loads (design D1 risk)
- [x] 9.4 `cargo publish --dry-run -p looqlog-core` packages cleanly; `-p looqlog` still reports only the known missing-version error, which the crates.io change fixes
- [ ] 9.5 Rename the GitHub repository to `looqlog` and update the local remote; confirm the old URL redirects and the `v0.1.0` release page still resolves — SKIPPED by explicit instruction: this is an account-level, outward-facing action the user must decide and perform themselves; left undone
- [x] 9.6 Append a `docs/devlog.md` entry: the collision that triggered it, why the window closes at first publish, and the theme-preference reset as a user-visible change
- [x] 9.7 `openspec validate rename-to-looqlog --strict` passes, then run `/opsx:archive` — validate passes; `/opsx:archive` intentionally NOT run per explicit instruction (archiving is the user's decision), left for the user to run
