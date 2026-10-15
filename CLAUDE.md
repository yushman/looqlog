# looqlog

Single-binary Rust CLI that opens a local web UI for a log file or a live stdin
stream. Parsing happens in WebAssembly inside the user's browser — privacy first,
zero config, no runtime dependencies.

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
  ├── HTTP: serves index.html + TS bundle + core.wasm (include_bytes!, embedded)
  ├── WebSocket /ws: stdin ring buffer → snapshot-on-connect → live stream (ADR-0004)
  └── never reads the log file itself in file mode (ADR-0002)
        ↓
Browser (TypeScript + WASM)
  ├── File API: file stays in browser memory, never sent to the backend
  ├── WASM core (looqlog-core, target-agnostic, ADR-0005): format detect, parse, index
  └── UI: timeline (uPlot), virtual-scrolled table, filters, search
```

Key constraints, each with an ADR:
- Web UI over a local server, not a TUI — [ADR-0001](docs/adr/0001-web-ui-over-local-server-not-tui.md)
- File-mode parsing is WASM-only; backend never reads the file — [ADR-0002](docs/adr/0002-wasm-browser-parsing-file-mode.md)
- Bind to `127.0.0.1` by default, `--host` is an explicit, warned opt-in — [ADR-0003](docs/adr/0003-bind-127-0-0-1-by-default.md)
- stdin buffering is a bounded ring buffer with snapshot-on-connect — [ADR-0004](docs/adr/0004-ring-buffer-stdin-snapshot.md)
- Core parser crate is target-agnostic (wasm + native adapters), MCP transport is `rmcp` — [ADR-0005](docs/adr/0005-target-agnostic-core-crate-rmcp.md)
- The project, binary and crate were originally named `looq`, superseded by ADR-0009 — [ADR-0006](docs/adr/0006-project-named-looq.md)
- The project, binary and every crate are named `looqlog` — [ADR-0009](docs/adr/0009-project-renamed-to-looqlog.md)

**Privacy asymmetry to keep straight:** file mode never leaves the browser;
stdin/live-tail mode goes over a localhost WebSocket in the clear, so it "does not leave
the machine" but does cross a process boundary. The `/ws` origin check and per-process
token (`security` spec) guard that socket against another page in the same browser, not
against another process on the machine — and neither protects a non-loopback `--host`
(ADR-0003). Do not describe both modes with the same guarantee in UI copy or docs:
"never leaves your machine" holds for both, "never leaves your browser" only for file
mode.

## Stack

- Backend: Rust 1.86+, `axum` 0.8, `tokio`, `clap`, `tracing`
- Core parser (`looqlog-core`): target-agnostic Rust lib crate, no `wasm-bindgen`/`web-sys`
- WASM adapter: `wasm-bindgen`, `serde-wasm-bindgen`, built via `wasm-pack`
- Frontend: TypeScript (strict), Vite, Web Components, `uPlot`, `comlink`
- MCP (P2): `rmcp` SDK, native adapter over `looqlog-core`

## Testing

Cover the silent-failure list first — these are the places a bug would be quiet
rather than loud (see ADR-0004):

- Malformed log lines must be skipped **with a reported warning**, never silently
  dropped or crash the parser.
- Auto-detect misclassifying a format (e.g. plain text detected as logfmt) should be
  visible/overridable (`#format=`), not a silent wrong parse.
- Ring buffer drop-under-backpressure must surface as a "gap" indicator in the UI —
  a silently shortened live tail looks like "nothing happened" to the user.
- `--host` != `127.0.0.1` must always print the exposure warning — a missed warning
  is a silent privacy regression.
- WASM parse/filter latency regressions are easy to introduce
  silently; benchmark before merging changes to `looqlog-core`'s hot paths.

## Spec-driven workflow (OpenSpec) — required

This repo uses OpenSpec. The `openspec/` directory is **mandatory**.

- `openspec/specs/<capability>/spec.md` — current, accepted behavior per capability.
- `openspec/changes/<name>/` — an in-flight proposal (`proposal.md`, `design.md`,
  `tasks.md`) before it lands in `specs/`.
- `openspec/changes/archive/` — completed changes, kept for history.

**No feature work without a change proposal.** Every unit of work larger than a
bugfix starts as a proposal, is implemented against its `tasks.md`, and is archived
when done, so that `specs/` always describes what the code actually does.

**The flow is `opsx:explore` → `opsx:ff` → `opsx:apply` → `opsx:archive`, and nothing
longer.** This is a startup: the point of the spec artifacts is that the next person
(or agent) can tell what the code is supposed to do, not to run a committee. Use the
`opsx:` commands, not the slower `openspec-propose` dialogue.

- **`opsx:explore`** — think the problem through and resolve open questions *before*
  anything lands in `openspec/changes/`. Read the relevant docs/ADRs, ask the user
  what's ambiguous, decide scope.
- **`opsx:ff`** ("fast-forward propose") — once explore has resolved the open
  questions, write `proposal.md` + `design.md` + `specs/*.md` + `tasks.md` directly,
  in one pass. `openspec validate <name> --strict` must pass before moving on.
- **`opsx:apply`** — hand the validated `tasks.md` to a fresh subagent: groom in the
  main session, execute in a clean agent with no leftover context. Verify, don't just
  trust its self-report.
- **`opsx:archive`** — after `openspec validate --strict` passes again
  post-implementation, archive so `openspec/specs/` reflects what actually shipped.

A defect found while verifying an unarchived change is fixed inside that change —
new tasks appended to its `tasks.md` — not spun out as a follow-up proposal.

## Working rhythm — required

- **Write an ADR when a decision is expensive to reverse** or when a reader would
  question it. Never edit an accepted ADR to reflect a reversal — mark it superseded
  and write a new one.

## GIT

- **Commit docs and openspec artifacts along with the code they describe.** The specs
  say what the code is supposed to do and the archived changes say why it was built
  that way; a clone without them is a clone that cannot answer either question.

## UI

- **UI reference - [docs/ui.png](docs/ui.png)**
