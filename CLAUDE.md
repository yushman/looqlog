# looqlog

Single-binary Rust CLI that opens a local web UI for log files and a live stdin
stream. Parsing, indexing and search run natively in the local `looqlog` process; the
browser is a thin client — privacy first (nothing leaves the machine), zero config, no
runtime dependencies.

Read [docs/adr/](docs/adr/) before changing anything — it explains why the
architecture is the way it is.

## Language and READMEs

Code, comments, commits, docs and the English README are in English. `README.ru.md`
is the only Russian file.

**Both READMEs are mandatory and must stay in sync.** Any change to user-facing
behavior — install steps, commands, flags, output, limitations — updates `README.md`
and `README.ru.md` in the same commit. A Russian README that lags the English one is
worse than none, because it documents behavior the tool no longer has.

## Architecture

```
CLI (axum + tokio, 127.0.0.1:PORT)
  ├── HTTP: serves index.html + TS bundle + stylesheet at / and /l/<id> (embedded)
  ├── POST /upload: picked/dropped file streamed into a new source (token + Origin/Host)
  ├── Registry of sources (ADR-0011): argv files and stdin owned by the process,
  │   uploads owned by tabs (10-min GC), one 4 GiB retained-bytes budget
  ├── each source is a Session (looqlog-core, target-agnostic, ADR-0005): detect,
  │   parse, index, filter, search, timeline buckets (ADR-0010); stdin ring buffer (ADR-0004)
  └── WebSocket /ws: token + source handshake → queries/windows/buckets + coalesced
      notifications; no source = the landing's live source list
        ↓
Browser (TypeScript, thin client)
  ├── / : landing — source list, upload
  └── /l/<id>: timeline (uPlot), virtual-scrolled table, filters, search for one
      source — renders only the rows on screen, pulled from the backend
```

Key constraints, each with an ADR:
- Web UI over a local server, not a TUI — [ADR-0001](docs/adr/0001-web-ui-over-local-server-not-tui.md)
- File-mode parsing was WASM-only; superseded by ADR-0010 — [ADR-0002](docs/adr/0002-wasm-browser-parsing-file-mode.md)
- Bind to `127.0.0.1` by default, `--host` is an explicit, warned opt-in — [ADR-0003](docs/adr/0003-bind-127-0-0-1-by-default.md)
- stdin buffering is a bounded ring buffer with snapshot-on-connect — [ADR-0004](docs/adr/0004-ring-buffer-stdin-snapshot.md)
- Core parser crate is target-agnostic (native today, MCP adapter later), MCP transport is `rmcp` — [ADR-0005](docs/adr/0005-target-agnostic-core-crate-rmcp.md)
- The project, binary and crate were originally named `looq`, superseded by ADR-0009 — [ADR-0006](docs/adr/0006-project-named-looq.md)
- The argv path was a hint, not an auto-loaded file; superseded by ADR-0010 — [ADR-0007](docs/adr/0007-argv-path-is-a-hint-not-an-auto-loaded-file.md)
- Compiled frontend artifacts are committed and shipped in the crate, so building needs no JS toolchain — [ADR-0008](docs/adr/0008-vendored-frontend-artifacts.md)
- The project, binary and every crate are named `looqlog` — [ADR-0009](docs/adr/0009-project-renamed-to-looqlog.md)
- Parse on the backend; the browser is a thin client — [ADR-0010](docs/adr/0010-parse-on-the-backend-thin-client.md)
- Sources are owned by the process (argv, stdin) or by tabs (uploads), under one memory budget — [ADR-0011](docs/adr/0011-sources-owned-by-process-or-tabs.md)

**Privacy guarantee to keep straight:** every mode — argv file, uploaded file, stdin — is
read by the local `looqlog` process and reaches the browser over a localhost WebSocket in
the clear, so log contents "never leave your machine" but do cross a process boundary
(ADR-0010). The origin/host checks and per-process token (`security` spec) guard `/ws` and
`/upload` against another page in the same browser, not against another process on the
machine — and none of them protects a non-loopback `--host` (ADR-0003). Never write "never
leaves your browser" in UI copy or docs; it is no longer true for any mode.

## Stack

- Backend: Rust 1.86+, `axum` 0.8, `tokio`, `clap`, `tracing`
- Core engine (`looqlog-core`): target-agnostic Rust lib crate, no I/O, no async
- Frontend: TypeScript (strict), Vite, Web Components, `uPlot`
- MCP (P2): `rmcp` SDK, native adapter over `looqlog-core`

Module specs: [crates/looqlog/CLAUDE.md](crates/looqlog/CLAUDE.md),
[crates/looqlog-core/CLAUDE.md](crates/looqlog-core/CLAUDE.md), [web/CLAUDE.md](web/CLAUDE.md).

## Packaging and release

- `cargo build` and `cargo install looqlog` need only a Rust toolchain: the built frontend
  (`index.html`, `assets/index.js`, `assets/index.css`) is committed under `crates/looqlog/assets/`
  and ships in the crate (ADR-0008). No WebAssembly is built or vendored. `looqlog-core` is
  published before `looqlog`, which depends on it by path and version.
- `scripts/build-frontend.sh` is the one rebuild command. Vite and TypeScript are pinned exactly in
  `web/package.json`, Node to major 24 in CI; the script warns when the local Node major differs.
  CI rebuilds and fails when `crates/looqlog/assets/` drifts, uploading the rebuilt files as a run
  artifact.
- A `v*` tag runs `release.yml`: it fails unless the tag equals the workspace version; builds
  `x86_64-unknown-linux-musl` (static), `aarch64-apple-darwin`, `x86_64-apple-darwin` and
  `x86_64-pc-windows-msvc`; smoke-tests each binary on its runner with the same checks
  (`scripts/smoke-release-binary.sh`: version, every flag in help, serving the page, a stdin line
  queryable over an authenticated WebSocket — runners have no TTY, so stdin mode); records each size
  in the release body; and fails when the Linux x86_64 binary exceeds 10 MiB. It then confirms through
  the GitHub API that the release exists, is not a draft and carries exactly the uploaded asset names,
  retrying briefly on 404.
- `Cargo.toml`'s `repository` and the READMEs' clone and Releases links point at
  `github.com/yushman/looqlog`; `LICENSE` holds the MIT text the `license` field names.

## Testing

Cover the silent-failure list first — these are the places a bug would be quiet
rather than loud (see ADR-0004):

- Malformed log lines must be skipped **with a reported warning**, never silently
  dropped or crash the parser.
- Auto-detect misclassifying a format (e.g. plain text detected as logfmt) should be
  visible/overridable (`#format=`), not a silent wrong parse.
- Ring buffer eviction must surface as an "N oldest lines evicted" marker in the UI —
  a silently shortened live tail looks like "nothing happened" to the user.
- `--host` != `127.0.0.1` must always print the exposure warning — a missed warning
  is a silent privacy regression.
- Parse/filter latency regressions are easy to introduce silently; run `cargo bench`
  on `looqlog-core` before merging changes to its hot paths.

## Spec-driven workflow (slimspec) — required

Specs are the module `CLAUDE.md` files —
[crates/looqlog/CLAUDE.md](crates/looqlog/CLAUDE.md),
[crates/looqlog-core/CLAUDE.md](crates/looqlog-core/CLAUDE.md), [web/CLAUDE.md](web/CLAUDE.md) —
plus this file for cross-module facts. They describe what the code does now; read the
module's spec before changing it.

**No feature work without a slimspec change.** Anything larger than a bugfix runs through
`/slimspec <idea>`: explore → plan → apply → verify → ground. The in-flight change lives in
`.slimspec/changes/<slug>/` (`explore.md`, `tasks.md`) and is deleted by ground once its durable
knowledge is merged into the specs.

- Pick the mode by risk: `fast` — one module, a few files, no open decisions; `medium` — real
  decisions or several files; `full` — cross-module, public contract, security or migration.
- Decisions are made in explore, in the main session; implementation is red-green TDD in
  fresh agents. Verify the agents' evidence, don't trust their self-report.
- A defect found by verify is fixed inside the change (a reopened or new task), not spun out
  as a follow-up change.
- Never edit module specs by hand to describe unshipped behavior; ground updates them from
  what actually shipped.

## Working rhythm — required

- **Write an ADR when a decision is expensive to reverse** or when a reader would
  question it. Never edit an accepted ADR to reflect a reversal — mark it superseded
  and write a new one.

## GIT

- **Commit docs and specs along with the code they describe.** The specs say what the
  code is supposed to do; a clone without them is a clone that cannot answer that question.

## UI

- **UI reference - [docs/ui.png](docs/ui.png)**
