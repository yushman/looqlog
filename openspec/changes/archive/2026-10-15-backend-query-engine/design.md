## Context

Today the backend serves static assets and relays raw stdin lines over `/ws`. Everything
else — detection, parsing (`looqlog-wasm` over `looqlog-core`), the time-ordered index
(`entry-index.ts`), field filters and search (`predicate.ts`, `search-query.ts`), chain-aware
matching and timeline bucketing (`timeline-bucket.ts`) — runs in the browser, in a Web
Worker and on the main thread. ADR-0002 put it there to keep file contents inside the
browser; ADR-0007 followed from it (argv path is a hint).

This change reverses that: the engine moves to the native backend, the browser keeps only
presentation state. It is the first of two changes; `multi-source-tabs` builds a source
registry and per-tab URLs on top of the engine introduced here.

## Goals / Non-Goals

**Goals:**
- One engine in `looqlog-core` that ingests lines (file or stdin), maintains the index and
  answers queries. Target-agnostic, so the future MCP adapter (ADR-0005) reuses it.
- Thin client: no entry set held in the browser beyond the rows currently rendered (plus a
  small overscan cache).
- `looqlog app.log` shows the log without a picker step; picker/drop upload still works.
- Every behavior the existing specs describe at the UI level — chips, search, `field=value`,
  chain-aware filtering, timeline, range drag, live tail, url-state — keeps working.
- Remove WASM from the build entirely.

**Non-Goals:**
- More than one source per process, `/l/<id>` URLs, empty `/` landing, several argv paths,
  upload lifetime/GC, memory budget, port fallback — all `multi-source-tabs`.
- Following a growing file (`tail -f`). The argv file is read once.
- Persisting anything to disk.
- Changing parser behavior (`log-parsing`, `format-detection`, `field-extraction`,
  `entry-continuations` specs stand as they are).

## Decisions

### D1. Engine lives in `looqlog-core`, the server only owns it

New modules in `looqlog-core`: `index` (port of `EntryIndex`: input-order store, sorted
timestamp refs, timestampless set, chain members, level counts, eviction from the front),
`query` (compiled query: substring / `/regex/` / field filters, chain-aware predicate with the
root/member split from `predicate.ts`), `bucket` (port of `timeline-bucket.ts` bucket width
picking and robust span). A `Session` type holds the data only — `Parser`, index, retained raw
lines — and exposes `ingest(chunk)`, `finish()`, `set_format(..)`, `stats()`. Query state lives
in a separate `View` (query, match set, generation, toggled chains), one per `/ws` connection,
and is passed to `session.set_query(&mut view, q)`, `session.window(&mut view, ..)`,
`session.buckets(&mut view, ..)`. A view catches up with appended/evicted entries lazily via
`MatchSet::catch_up`, so two tabs (or a reload next to an old tab) never see each other's
filters. No async, no I/O, no serde-on-the-wire assumptions beyond `Serialize` on result types.

*Alternative: one shared query per process.* Rejected during apply: every connection to the same
session would overwrite every other connection's match set.

The `looqlog` binary wraps one `Session` in `Arc<Mutex<_>>` (std mutex; every call is
CPU-bound and short except the full re-filter, see D6) and drives ingestion from a blocking
task.

*Alternative: keep logic in TS, only move parsing.* Rejected in explore: it keeps the
browser-memory ceiling and gains nothing for the MCP adapter.

### D2. Transport: one `/ws` session per tab, request/response + notifications

The existing token handshake stays as the first frame. After it, the client sends requests
tagged with an `id` and a `type` (`query`, `window`, `buckets`, `entry`, `set_format`);
the server replies with the same `id`. `matched` in replies and notifications is per
connection (its own view). The server also pushes untagged notifications:
`progress` (bytes parsed / total), `grown` (new total and matched counts after an ingest
batch), `evicted` (count of lines dropped from the ring buffer), `ended` (stdin EOF),
`reset` (source replaced or re-parsed with a new format — client must re-query).

Notifications are coalesced on a fixed interval (100 ms), so a fast producer costs one
message per tick, not one per line. Since the client pulls rows, nothing the client needs is
lost when it is slow — this is why per-client gap events disappear.

*Alternative: REST for queries + WS for notifications.* Rejected: two auth paths and two
places to keep in sync per tab. One socket per tab also gives `multi-source-tabs` a natural
lifetime signal (WS closed → tab gone).

Upload is the one HTTP route that is not on the socket (D4).

### D3. Query model: the client owns the query, the server owns the result

Each `query` request carries the full view state: field filters, raw search text, time
range, and the session's current format override. The server compiles it (search grammar,
including `field=value` token extraction against the field inventory, moves to
`looqlog-core::query`), rebuilds the match set, and replies with `{ total, matched,
timestampless_excluded, compile_error? }`. `window(offset, limit)` returns rendered rows of
the *current* match set with their ordinals and chain structure. A `query` bumps a
generation number; replies to stale generations are discarded client-side.

Chain collapsing is view state on the backend: the client sends the set of toggled root
ordinals (`toggle_chain` request or inside `query`). A chain is expanded when (the search
matched one of its members) XOR (its root is toggled). `window` offsets and the reply's
`row_count` are computed over the collapsed view, so virtual scroll arithmetic stays on the
server; a toggle recomputes visible rows without re-filtering.

Rows carry what the table and detail pane need (timestamp, level, message, fields, raw line,
continuation link, match ranges for search highlighting) — the same DTO shape the WASM
bridge produced, now `Serialize`d to JSON.

### D4. File ingest

- **argv path**: read on a blocking task at startup, in 64 KiB chunks, fed to the session.
  The page opens straight into the log with progress. A read error is a session error the
  page shows (`error-states`), not a startup crash, so `looqlog missing.log` still starts
  and says what failed.
- **Upload**: `POST /upload` with the raw file body (`Content-Type:
  application/octet-stream`, filename in a header), token in a header, streamed into the
  session as it arrives — never buffered whole. A successful upload replaces the current
  source (single source per process in this change); open sockets receive `reset`. An
  upload that fails (cap exceeded, read error, binary refused) leaves the previous source
  intact (`error-states` "Failure does not destroy existing state").
- **Size cap for argv**: passing the path is the confirmation. Above the warning threshold the
  file is read immediately and the warning is shown on the terminal and the page; above the hard
  cap it is not read and the error is shown in both places. Uploads keep the pre-parse
  continue/cancel prompt.
- **Size cap**: re-derived for backend memory. Warn threshold and hard cap remain, measured
  via `cargo bench` on the index footprint per MiB of input; both numbers recorded in
  `docs/devlog.md`. Default memory budget for `multi-source-tabs` will reuse this
  footprint-per-MiB estimate rather than a line count.

### D5. Stdin: parse at ingest, ring buffer of parsed entries

The stdin reader feeds lines into the same `Session`. Detection hold (`live-tail-ui` "Stream
format detection holds the opening lines") moves to the backend: the session buffers the
opening lines until the sample is sufficient or the timeout expires, then detects once. The
ring buffer becomes the index's front eviction at `--max-lines`; each eviction batch emits
`evicted`. Sequence numbers stay as entry ordinals.

`set_format(format | auto, tz_offset_minutes?)` (url-state `#format=` and `#tz=` overrides)
re-parses the retained raw lines with the new
format and emits `reset`. The session keeps each entry's raw line(s) alongside the index,
evicted together, so re-parse needs neither the file nor a re-upload and works the same for
argv, upload and stdin. `--max-lines` caps retained raw lines (blank and malformed lines yield
no entry but still count); evicting line N evicts entries with ordinal ≤ N and `evicted` counts
lines. The parser accepts a starting line number so a re-parse after eviction keeps ordinals.

### D6. Filtering cost

Full re-filter on query change is O(n) over retained entries, done on a blocking task while
the mutex is held. Target: 10k entries within the existing 50 ms (TDR §11) including one
round trip, measured natively and recorded. Live append tests each new entry once against
the current predicate, as `EntryIndex.append` does today.

### D7. Security surface

- Token handshake unchanged for `/ws`; every request on the socket is therefore
  authenticated.
- `POST /upload` requires the token in an `X-Looqlog-Token` header and a matching
  `Origin`; it also checks `Host` against the bound address, closing DNS rebinding for both
  routes (`/ws` gains the Host check too).
- CSP drops `'wasm-unsafe-eval'`; no worker is created any more.
- `--host` warning: "log contents — files and stdin — become reachable from the network".

### D8. ADRs

ADR-0010 "Parse on the backend; the browser is a thin client" supersedes ADR-0002 and
ADR-0007. Their status lines change to `Superseded by ADR-0010`; their bodies stay.
ADR-0004 stands (ring buffer, snapshot) — its per-client backpressure paragraph is described
as obsolete in ADR-0010's Consequences rather than editing 0004.

## Risks / Trade-offs

- [Privacy claim weakens] → Explicit in ADR-0010, both READMEs, UI mode indicator: "never
  leaves your machine", for both modes. `CLAUDE.md` privacy-asymmetry section is rewritten.
- [Round-trip latency on scroll] → Overscan window cache client-side (2 screens), windows
  requested ahead of the viewport; measured on a 1M-line file.
- [Mutex contention: a full re-filter blocks ingest] → Acceptable at current scale; ingest
  batches queue in the bounded channel. Revisit trigger: re-filter over the hard-cap file
  exceeding 500 ms.
- [Retained raw lines roughly double memory per entry] → Counted in the footprint-per-MiB
  measurement (D4) that sets the size cap; buys format override without re-reading input.
- [Regex dialect change] → Rust `regex` lacks look-around/backreferences; compile error is
  shown as today's "Invalid regex fails loudly". Documented in both READMEs.
- [Large rewrite of the frontend] → Components keep their public shape; only the data source
  changes. Task order keeps the app working between steps where possible.

## Migration Plan

Single release. No persisted state to migrate. Old bookmarks with `#format=`/filters keep
working because url-state grammar is unchanged. Rollback = previous release binary.

## Open Questions

- Default memory budget for `multi-source-tabs`: derive from the measured index
  footprint-per-MiB (D4), not a line count. Exact number decided there.
- Size cap after measurement: retained memory is 3.46x (plain) to 10.24x (logfmt) of input, so
  the unchanged 50 MB warn / 400 MB cap can cost ~4 GB of process memory. Decided: keep
  50/400 MB; shrinking per-entry memory goes with the memory budget in `multi-source-tabs`.
