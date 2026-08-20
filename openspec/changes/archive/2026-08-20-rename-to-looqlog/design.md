## Context

ADR-0006 (2026-08-08) named the project `looq` before a line of code existed, precisely
because naming is irreversible once a crate is published. It closed with an unusually
explicit exit condition:

> **What would make us revisit:** nothing short of a trademark conflict. Once the crate is
> published the name is permanent, so the practical window for reversal closes at first
> publish — which is also why this decision was made before writing code rather than after.

Two facts have since changed the picture.

**The conflict exists, in category.** [Looq for
macOS](https://www.producthunt.com/products/looq-3) is a developer tool that previews
files from Quick Look, explicitly handles **log files** (finding timestamps inside zipped
log archives), and sells itself on **"Zero Data Collection", running entirely locally**.
Same audience, same domain, same privacy pitch. Separately, [Looq AI](https://looq.ai/) is
a funded company holding `looq.ai` — different industry, and the weaker of the two
problems.

**The window is still open.** All three crate names are unregistered:

```
crates.io  looq         404  free
crates.io  looq-core    404  free
crates.io  looq-wasm    404  free
crates.io  looqlog      404  free
```

v0.1.0 is tagged and four binaries are on the Releases page, so there is some public
exposure — but nothing that crates.io will freeze forever. After the first
`cargo publish`, this change becomes impossible.

## Goals / Non-Goals

**Goals:**
- One word for the crate, the binary, the product and the repository, preserving what
  ADR-0006 counted as its main win: `cargo install looqlog` needs no explanation and no
  `git-delta`-style split between package name and command name.
- Land before crates.io publication, which is the only thing making this reversible.
- Leave the historical record intact and readable, rather than rewriting the past to
  match the present.
- Zero behavior change. Every test that passed before passes after, with the same
  assertions about the same behavior under a different name.

**Non-Goals:**
- Publishing to crates.io. That is the next change; this one only makes it safe to do.
- Bumping the version or cutting a release.
- Rewriting `docs/devlog.md`, the bodies of accepted ADRs, or archived changes.
- Providing an upgrade path for someone holding a v0.1.0 `looq` binary. There is none;
  they reinstall.

## Decisions

### D1 — `looqlog`, not the alternatives

The shortlist was `looqlog`, `loglens`, `lq`. Two failed on facts rather than taste.

**`lq` is unavailable.** Not a judgement call:

```
lq  0.16.0   created 2024-10-28   updated 2026-04-22   6,391 downloads
    github.com/clux/lq — "low overhead yq/tq/jq cli"
```

ADR-0006 already rejected it in 2026-08 for the same reason; the crate has only grown
since.

**`loglens` is free on crates.io and the worst possible choice in the wild.** Six
products carry the name, and they are all in this exact category:

| What | Overlap |
|---|---|
| [getloglens.com](https://getloglens.com/) | "Blazing Fast Structured Log Analysis CLI Tool", JSON, interactive TUI — this project's pitch |
| [`loglens-core`](https://users.rust-lang.org/t/loglens-core-a-zero-config-structured-log-parsing-engine-json-logfmt/136406) | a **Rust** crate, "zero-config structured log parsing engine (JSON/Logfmt)" — `looq-core`'s description, same `-core` suffix |
| [LogLens](https://www.linuxlinks.com/loglens-log-viewer/) | Python TUI log viewer |
| [VS Code extension](https://marketplace.visualstudio.com/items?itemName=DenizYesilirmak.loglens) | **Android logcat** viewer |
| [Chrome extension](https://chromewebstore.google.com/detail/loglens-simple-android-lo/inlfidjabddanmepfheklibnbhmjkibp) | "Simple Android Logcat Viewer" |
| [Academic paper](https://www.researchgate.net/publication/326445987_LogLens_A_Real-time_Log_Analysis_System) | "LogLens: A Real-time Log Analysis System" |

Choosing it would reproduce, worse, the exact failure ADR-0006 rejected the original "Open
Log Viewer" name for: launching into an established brand's search results.

**`looqlog` costs what ADR-0006 already priced.** It keeps the deliberate misspelling the
ADR booked as a known cost, and at 7 characters it is close to the 8 that got `logscope`
rejected for the pipe position (`myapp | looqlog`). What it buys: it says what the tool
is, it returns no competing product, and it stays recognisably continuous with the
already-released v0.1.0 rather than arriving as an unrelated third name.

### D2 — One name everywhere, not a crate/binary split

A `git-delta`-style split (crate `looqlog`, command `looq`) was available and roughly a
third of the work: only the manifests and READMEs would change, leaving 1,186 occurrences
in code untouched.

Rejected because it keeps the part that actually collides. The conflict is with a macOS
developer tool that reads logs; the string a user types and speaks is the string that
collides. A split would put the searchable name on crates.io and leave the ambiguous one
in the terminal — solving the registry problem while preserving the confusion, and giving
up what ADR-0006's Consequences section counted as the arrangement's main benefit.

### D3 — The past is not rewritten

`docs/devlog.md` (2,600 lines), the bodies of ADR-0001…0008, and the 15 archived changes
under `openspec/changes/archive/` — 56 files — keep saying `looq`. They are records of
what was decided and built at the time, and a devlog entry that claims the tool was always
called `looqlog` is simply false.

The dividing line is tense, not file type: **`openspec/specs/` describes what the code
does now**, so its eight affected specs are updated; `openspec/changes/archive/` describes
what was proposed then, so it is not. Same for `docs/PRD.md` and `docs/TDR.md` (current)
versus `docs/devlog.md` (historical).

ADR-0006 gets a status line only — `Superseded by ADR-0009` — never an edited body, per
this project's ADR rule. ADR-0009 carries the reasoning, and its Context is the one thing
ADR-0006 could not have known.

### D4 — The vendored bundle is rebuilt, never text-replaced

`crates/looqlog/assets/assets/index.js` is a minified Vite bundle with the twelve custom
element names compiled into it, alongside mangled identifiers. A repo-wide find-and-replace
would either corrupt it or patch it inconsistently with its `.css` and `.wasm` siblings,
and the result would still be a hand-edited artifact claiming to be generated output
(ADR-0008).

The correct sequence is: rename the sources, then run `./scripts/build-frontend.sh`, then
commit `crates/looqlog/assets/`. CI's macOS vendored-artifact-staleness job is the backstop
— if the bundle is edited rather than regenerated, that job fails.

### D5 — Renaming is verified by the test suite, not by grepping for leftovers

`grep -c looq` reaching zero proves nothing: it is equally satisfied by a rename that
broke the build. The change is verified by the existing suite passing unchanged — 208
Rust tests, 111 frontend tests, `tsc --noEmit`, `cargo clippy`, plus the two CI guard jobs
that name the crates explicitly (the ADR-0005 target-agnostic check runs
`cargo tree -p looq-core`, which must become `looqlog-core` or it silently stops checking
anything).

That last one is the trap worth naming: a guard job that references a crate by a name
nothing has anymore does not fail — `cargo tree -p <missing>` errors, but a step written
to tolerate it would pass vacuously. The ADR-0005 job must be confirmed to still *fire*,
not merely to still be green.

### D6 — The GitHub repository is renamed too

`openlogviewer` is the name ADR-0006 moved away from on 2026-08-08 — it collides with an
established automotive-ECU log tool — and it is still sitting in the `repository` field
that every crate inherits and that crates.io renders on the package page. Leaving it would
put a third name in a chain that is supposed to have one.

GitHub keeps a permanent redirect from the old path, so existing clones, the v0.1.0 tag
and the published release binaries all keep resolving.

## Risks / Trade-offs

- **A half-finished rename that still compiles.** Rust catches renamed crates at build
  time, but a stale custom element name only shows as a component that never upgrades, and
  a stale `__LOOQ_TOKEN__` placeholder means the WebSocket auth token is never substituted
  — the page loads and live tail silently fails to connect. → The frontend tests and a
  live-mode smoke run against a real binary are both required; a green `cargo build` is
  not sufficient evidence.
- **The vendored bundle drifting from its sources.** → D4's rebuild-don't-replace rule,
  backed by the existing CI staleness job.
- **A CI guard silently disarmed.** → D5: prove the ADR-0005 job still fails when fed a
  violation, rather than only observing that it is green.
- **Anyone holding a v0.1.0 `looq` binary.** No upgrade path; the next release installs a
  differently-named command and the old one stays on disk. → Both READMEs and the next
  release's notes state it plainly. The population is small — the release is days old and
  was never announced.
- **The theme preference resets** for anyone who had used the tool, because the
  `localStorage` key changes. Trivial in effect, but this project treats silent
  user-visible changes as defects. → Recorded in the devlog and mentioned in the release
  notes; not worth a migration shim for a single enum value.
- **Doing this at all, days after tagging v0.1.0.** The alternative is worse: after the
  first `cargo publish` the name is permanent, and the collision is with a tool in the same
  category selling the same privacy promise. This is the last cheap moment, which is
  exactly the moment ADR-0006 said to use.
