## 1. Decision record

- [x] 1.1 Write `docs/adr/0011-sources-owned-by-process-or-tabs.md` (registry, ownership table,
      10-minute upload GC, 4 GiB budget counted from retained bytes, stdin not closable, several
      stdin streams = several processes)
- [x] 1.2 Update `CLAUDE.md` architecture sketch and constraints list for the registry and ADR-0011

## 2. Core

- [x] 2.1 `Session::retained_bytes()` in `looqlog-core`, calibrated against the `mem_probe`
      example; unit test that it grows on ingest and shrinks on eviction; record calibration in
      `docs/devlog.md`

## 3. Registry in `looqlog`

- [x] 3.1 Turn `source.rs` into a registry: `SourceId` (8-char base32 from OS RNG, never reused),
      kind (argv / stdin / upload), owner, `connected_tabs`, `orphaned_since`, state for the
      landing (parsing / live / ended / error); `AppState` holds the registry
- [x] 3.2 Budget enforcement at ingest per chunk: unload orphaned uploads oldest-first; then abort
      the upload (drop partial source) or mark the argv source `error` with the budget reason
- [x] 3.3 Sweeper task every 30 s unloading uploads orphaned ≥10 min; sends `closed{unloaded}` (no
      tabs, but landing list updates)
- [x] 3.4 Close: `POST /sources/<id>/close` and landing `close` request with token/origin/host
      checks; 409 for stdin; tabs receive `closed{closed}`
- [x] 3.5 Unit tests: ids distinct and non-sequential; GC respects reconnect within grace;
      eviction order oldest orphan first; process-owned never evicted; stdin close refused

## 4. Server and protocol

- [x] 4.1 Serve the page at `/` and `/l/{id}` (200 for unknown ids)
- [x] 4.2 Auth frame `{token, source}`; per-source dispatch; tab count increments/decrements;
      `closed{unknown|closed|unloaded|budget}` reply without entry data
- [x] 4.3 Landing connection (`source: null`): `sources` list after auth and on every registry
      change; `close(id)` request
- [x] 4.4 `POST /upload` creates a source and returns `201 {id}`; independent of stdin; aborted
      upload leaves no source; `set_format`/`reset` scoped to the source
- [x] 4.5 Integration tests: two sources, two tabs see only their own entries; format override does
      not leak; unknown id → `closed{unknown}`; close from landing → tabs get `closed`; cross-site
      close → 403; upload next to stdin

## 5. CLI

- [x] 5.1 Positional `paths: Vec<PathBuf>`; stdin source when `--stdin` or not a TTY, coexisting
      with paths; no sources → empty registry
- [x] 5.2 Port fallback 7891..=7900 only when `--port` not given; banner says which port was busy;
      range exhausted / explicit busy port → clean error suggesting `--port 0`
- [x] 5.3 Banner: landing URL plus one `name → /l/<id>` line per source; `--open` opens one tab
      per source or the landing
- [x] 5.4 CLI integration tests: several paths; stdin + file; port fallback with 7891 held;
      explicit busy port fails

## 6. Frontend

- [x] 6.1 Route on `location.pathname`: `/` → landing, `/l/<id>` → existing workspace bound to id
- [x] 6.2 Landing component: live source list (name, kind, size, state, tab count, link, close
      except stdin), picker/drop upload → navigate to `/l/<id>`
- [x] 6.3 Closed-source state in the workspace for each reason, with upload option; never an empty
      table
- [x] 6.4 Remove in-place replacement: picker/drop in a viewing tab goes through the landing flow
- [x] 6.5 Sharing caveat mentions that a link names a source of a running process
- [x] 6.6 Unit tests for routing and the landing list reducer; `tsc --noEmit`, `vitest` pass;
      rebuild vendored assets with `scripts/build-frontend.sh`

## 7. Docs

- [x] 7.1 `README.md`: several paths, stdin with files, landing at `/`, `/l/<id>` per tab, upload
      lifetime (10 min without a tab), 4 GiB budget, port fallback, several streams = several
      processes
- [x] 7.2 `README.ru.md`: the same, in the same commit
- [x] 7.3 `docs/devlog.md`: entry for this change

## 8. Verification

- [x] 8.1 `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`,
      `cargo fmt --check`, `tsc --noEmit`, `vitest` pass
- [x] 8.2 Manual (Playwright): `cat fixture | looqlog a.jsonl b.log` → landing lists three sources;
      each `/l/<id>` shows its own log; new tab at `/` is the landing; reload keeps the source;
      upload from landing navigates to it; close from landing switches its tab to closed; stale id
      shows closed state
- [x] 8.3 Manual: second `looqlog` with 7891 busy binds 7892 and says so
- [x] 8.4 `openspec validate multi-source-tabs --strict` passes
