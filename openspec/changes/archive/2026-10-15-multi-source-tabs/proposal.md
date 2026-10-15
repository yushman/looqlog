## Why

After `backend-query-engine` the backend owns parsed data, but a process still holds exactly one
source: an upload replaces the current file, every tab shows the same log, and a new tab cannot
start empty. Users want a tab bound to a specific log, a fresh tab on open, and several logs —
including several stdin streams — open side by side.

## What Changes

- **BREAKING** A process holds a registry of sources, each with a random 8-character id. A tab is
  bound to one source at `/l/<id>`; reload keeps it.
- **BREAKING** `/` is always an empty landing page: the list of the process's sources (name, kind,
  size, state, open tabs, close) plus picker/drop upload. It no longer shows a log directly.
- **BREAKING** File and stdin are no longer exclusive modes: `cmd | looqlog a.log b.log` creates a
  stdin source and two file sources. Several positional paths are accepted.
- **BREAKING** `POST /upload` adds a new source and returns its id instead of replacing the current
  one; uploads are allowed alongside stdin.
- Source lifetime: argv files and stdin are owned by the process (argv files closable, stdin not);
  uploads are owned by tabs — closed explicitly, or unloaded 10 minutes after their last tab
  disconnects.
- A fixed 4 GiB process-wide memory budget counted from real retained bytes: an upload that does
  not fit after unloading the oldest tabless uploads is aborted with an error; an argv file that
  does not fit becomes a source in an error state.
- A tab on a closed, unloaded or unknown source says so and offers an upload — never an empty table.
- `/ws` authentication names the source; `set_format` applies to that source and all its tabs.
- Banner prints the landing URL and one URL per source; `--open` opens one tab per source, or the
  landing when there are none.
- When `--port` is not given and 7891 is busy, the next free port in 7891–7900 is used; an explicit
  busy `--port` still fails.
- New ADR-0011 records the source ownership and lifetime model.
- Several stdin streams = several `looqlog` processes on different ports (decided in explore; no
  shared hub).

## Capabilities

### New Capabilities
- `source-registry`: source ids, kinds and owners, lifetime and GC of uploads, closing, the memory
  budget and its eviction order, the landing page's source list.

### Modified Capabilities
- `cli`: several positional paths; stdin and files coexist; banner with per-source URLs; `--open`
  per source.
- `local-server`: `/l/<id>` route; port fallback within 7891–7900 when `--port` is not given.
- `file-ingest`: argv files become independent sources; upload creates a source and returns its
  id; no stdin-mode refusal; a new ingest no longer cancels another source.
- `query-api`: authentication names the source; unknown/closed source reply; `closed`
  notification; format override scoped to the source.
- `browser-file-loading`: picker/drop live on the landing page and on a closed-source tab; the
  uploading tab navigates to the new source.
- `app-shell`: landing page as the unopened state; closed-source state.
- `error-states`: a failed upload never touches other sources; budget errors.
- `security`: upload response and landing list are behind the same token/origin/host checks.
- `url-state`: a link names a source of a running process; sharing caveat no longer claims the log
  stayed in the browser.

## Impact

- `crates/looqlog`: `source.rs` becomes a registry with owners, tab counts, GC timer and budget;
  `server.rs` routes `/l/<id>`, source-aware auth, landing list endpoint, close; `main.rs`/`cli.rs`
  accept several paths and stdin together, port fallback, banner.
- `crates/looqlog-core`: `Session` exposes retained bytes.
- `web/`: landing page component, routing by path, closed-source state, upload navigation.
- Docs: ADR-0011; both READMEs; `CLAUDE.md` architecture sketch.
