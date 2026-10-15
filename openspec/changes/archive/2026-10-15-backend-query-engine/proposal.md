## Why

Parsing, indexing, filtering and search run in browser-side WASM (ADR-0002), which caps
usable file size at what `wasm32` linear memory can hold, makes a page reload lose the file,
and forces a file-picker click even when the path was given on the command line (ADR-0007).
Moving the engine into the native backend removes the memory ceiling and the extra click, and
is the prerequisite for binding a browser tab to a specific log (`multi-source-tabs`, the
follow-up change).

## What Changes

- **BREAKING** The backend reads the log file. `looqlog app.log` reads and parses `app.log`
  directly; the page shows it with no picker step. The argv path is no longer a hint.
- **BREAKING** A file picked or dropped in the browser is uploaded to the local backend
  over HTTP and parsed there. The "file never leaves the browser" guarantee is withdrawn
  everywhere — UI copy, `README.md`, `README.ru.md`, specs. Both modes are described as
  "never leaves your machine".
- Parsing, the time-ordered index, field filters, search (substring, `re:` regex,
  `field=value`), chain-aware matching and timeline bucketing run in `looqlog-core`
  natively. The browser becomes a thin client: the table requests windows of the current
  result set, filter/search/range changes are queries to the backend, and the timeline
  receives bucket counts rather than computing them.
- Stdin lines are parsed on the backend as they arrive. The live connection pushes
  "result set grew/shrank" notifications instead of raw lines; the client pulls what it
  displays.
- **BREAKING** The `looqlog-wasm` crate, the Web Worker, `comlink`, `core.wasm` and the
  TypeScript copies of the predicate/search/index/bucket logic are removed.
- Client-backpressure gaps disappear (the client pulls; nothing is pushed to drop). Ring
  buffer eviction remains and stays visible: "N oldest lines evicted".
- The per-process token and Origin/Host checks extend from `/ws` to every query and upload
  endpoint. The `--host` exposure warning names file contents, not only stdin.
- New ADR-0010 supersedes ADR-0002 and ADR-0007.
- Still one source per process. Multiple sources, `/l/<id>` tab URLs, an empty `/`
  landing, several argv paths, upload lifetime and memory budget, and port fallback are the
  follow-up change `multi-source-tabs`.

## Capabilities

### New Capabilities
- `query-api`: the browser↔backend query protocol — session over `/ws`, window requests,
  filter/search/range queries, bucket queries, change notifications, error replies.
- `file-ingest`: how a file reaches the backend — argv path read at startup, browser upload,
  chunked parse with progress, size cap, replacement of the current file.

### Modified Capabilities
- `browser-file-loading`: picker/drop become an upload; the hint and "stays in browser"
  requirements are removed.
- `wasm-bridge`: removed entirely.
- `local-server`: no `core.wasm`; the server reads the argv file and accepts uploads; the
  "never reads the log file" requirement is removed.
- `security`: token and origin/host check cover HTTP query/upload routes; CSP no longer
  needs WASM/worker directives.
- `cli`: positional path is read by the backend; startup output and `--host` warning change.
- `entry-index`: the index lives in the backend.
- `filtering`: the latency target is measured natively plus one query round trip.
- `search`: `re:` uses the Rust `regex` dialect instead of JavaScript `RegExp`
  (no look-around, no backreferences); an unsupported construct fails loudly.
- `stdin-stream`: lines are parsed server-side; the wire carries notifications, not raw
  lines; per-client gap events are removed; eviction is reported.
- `live-tail-ui`: retention is the backend's; gap markers come from eviction only; the
  privacy indicator wording changes; detection hold moves to the backend.
- `error-states`: module-load failure removed; the size cap is re-derived from backend
  memory; upload/read failures added.
- `packaging`: no `wasm-pack`, no `core.wasm` in the vendored artifacts.

## Impact

- Crates: `looqlog-core` gains `index`, `query` and `bucket` modules; `looqlog-wasm` is
  deleted from the workspace; `looqlog` gains a source/engine layer, query routes, upload.
- Frontend: `worker.ts`, `parser-worker-client.ts`, `parser-api.ts`, `bridge.ts`,
  `entry-index.ts`, `predicate.ts`, `search-query.ts`, `timeline-bucket.ts`,
  `core-wasm-types.ts`, `wasm-types.ts` removed or reduced to a query client; components
  read from it.
- Build: `scripts/build-frontend.sh` stops running `wasm-pack`; CI drift check covers the
  JS/CSS bundle only.
- Docs: ADR-0010; ADR-0002 and ADR-0007 marked superseded; both READMEs; `CLAUDE.md`
  architecture and privacy-asymmetry section.
- Benchmarks move from browser-measured WASM to `cargo bench` on `looqlog-core`.
