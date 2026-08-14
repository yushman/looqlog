## Why

The repository has documentation (PRD, TDR, six ADRs, a 30-day MVP plan) and no code. The
riskiest assumption in the whole project — that browser-side WASM parsing can meet the
TDR §11 latency targets that ADR-0002 depends on — is untested, and two decisions that
are cheap now and expensive later are unrecorded: how a log file actually reaches the
browser when the backend is forbidden to read it, and how the compiled frontend reaches a
user who runs `cargo install looq` without Node installed.

This change builds the walking skeleton end to end (CLI → embedded UI → WASM parse in the
browser → `/ws` stdin transport), produces the day-4 benchmark number, and records the two
missing decisions as ADR-0007 and ADR-0008 before anything is built on top of them. It
covers days 1–5 of `docs/mvp-plan.md` plus the CLI flags scheduled for day 23, which belong
to the same argv surface and are cheaper to write once than to retrofit.

## What Changes

- Cargo workspace: `looq` (binary) + `looq-core` (target-agnostic lib crate per ADR-0005).
- `axum` HTTP server bound to `127.0.0.1:7891` by default, serving assets embedded with
  `include_bytes!`; graceful shutdown on Ctrl+C.
- Full `clap` argv surface per TDR §6: `--port`, `--host`, `--open`, `--no-browser`,
  `--stdin`, `--max-lines`, `--version`, `--help`. Includes `--port 0` (random free port)
  and `--open` / `--no-browser`, pulled forward from mvp-plan day 23.
- Mandatory stdout exposure warning whenever `--host` != `127.0.0.1` (ADR-0003), wired at
  the moment the flag is introduced rather than at the day-27 security pass.
- Minimal WASM adapter over `looq-core` with a single hardcoded JSON-Lines parse entry
  point, built by `wasm-pack` and embedded into the binary.
- Browser page that loads a user-selected local file through the File API, parses it in
  WASM, and reports the entry count — with zero backend reads of the log file (US-6).
- `/ws` WebSocket endpoint plus a stdin reader task; in this change the path is
  line-in → line-out (echo-grade). The ring buffer, snapshot-on-connect and backpressure
  from ADR-0004 are explicitly deferred to the `live-tail` change.
- Vendored frontend build artifacts (JS bundle + `core.wasm`) committed to the repository
  and included in the published crate, so `cargo build` and `cargo install` need no Node
  or `wasm-pack`, with a CI check that the vendored artifacts match their sources.
- **ADR-0007** — the argv path is a hint, not an auto-loaded file. PRD US-1 and mvp-plan
  day 3 both read as if `looq app.log` parses the file by itself; no browser API can open
  a server-supplied path without a user gesture, so under ADR-0002 the user must pick or
  drop the file. Alternatives (streaming the file bytes over the stdin path; an opt-in
  `--serve-file`) get recorded and rejected rather than left implicit.
- **ADR-0008** — vendored frontend artifacts as the distribution mechanism, against a
  `build.rs` that shells out to `wasm-pack` (breaks offline and Node-less installs) and
  against not publishing to crates.io at all (breaks the `cargo install looq` promise in
  TDR §5).
- Day-4 benchmark: parse a ~1MB JSON-Lines fixture in the browser, record the measured
  number against the <200ms/MB target in `docs/devlog.md` together with the command that
  produced it.

Not in this change: real multi-format parsing and auto-detect, `Entry`/index structures,
`serde-wasm-bindgen` typed interop, `comlink`, Vite/Web Components app shell, ring buffer,
timeline, table, filters, search, CSP, `/ws` origin check and token.

## Capabilities

### New Capabilities

- `cli`: argv surface, flag semantics, startup output, the `--host` exposure warning,
  browser auto-open, and file-vs-stdin mode selection.
- `local-server`: HTTP listener, bind address and port allocation, serving of
  compile-time-embedded assets, graceful shutdown.
- `stdin-stream`: reading stdin off the main path and transporting lines to connected
  browsers over `/ws`.
- `browser-file-loading`: how a log file reaches the browser and gets into WASM without the
  backend ever reading its contents (the ADR-0002/ADR-0007 contract, and the surface US-6
  is verified against).
- `packaging`: how the frontend artifacts get into the binary and into a published crate
  that builds without a JavaScript toolchain.

### Modified Capabilities

None — `openspec/specs/` is empty; this is the first change in the project.

## Impact

- New: `Cargo.toml` (workspace), `crates/looq/`, `crates/looq-core/`, `crates/looq-wasm/`,
  `web/` (minimal page + build script), `tests/fixtures/`, vendored `assets/` build output.
- New docs: `docs/adr/0007-*.md`, `docs/adr/0008-*.md`; first entries in `docs/devlog.md`.
- Dependencies introduced: `axum` 0.8, `tokio`, `clap` 4, `tracing`, `tracing-subscriber`,
  `wasm-bindgen`, and `wasm-pack` + Node as build-time-only (not install-time) tooling.
- Both READMEs get their first real content: install steps and the two run modes, kept in
  sync per the project rule.
- Constrains later changes: `looq-core` must stay free of `wasm-bindgen`/`web-sys`
  (ADR-0005), and the vendored-artifact workflow becomes part of every frontend change
  after this one.
