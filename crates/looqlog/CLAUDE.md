# looqlog

The binary: CLI, HTTP and WebSocket server, source registry, and the file, upload and stdin
readers that feed `looqlog-core` sessions.

## CLI
- Flags: `--port` (7891), `--host` (127.0.0.1), `--open`, `--no-browser`, `--stdin`,
  `--max-lines` (100000), `--version`, `--help`, plus zero or more log paths. An unknown flag exits
  non-zero naming it and starts no server; an invalid `--host` exits 2.
- Each path becomes its own source. A stdin source is added when `--stdin` is passed or stdin is
  not a TTY; both kinds coexist. With no sources the server starts with an empty registry.
- `--max-lines` bounds only the stdin source; given without one, a note on stderr says it has no
  effect.
- Any `--host` other than `127.0.0.1` prints, on stdout before the banner, a warning that log
  contents — opened files, uploads and stdin — become reachable from the network. It is
  unconditional and no flag suppresses it (ADR-0003).
- The banner prints the version, the landing URL with the port actually bound, one line per source
  with its `/l/<id>` URL (stdin also shows its max-lines), and how to quit; it says when the default
  port was busy.
- `--open` opens one browser tab per source, or the landing when there are none; `--no-browser`
  overrides it. A failed launch is not fatal: it prints that, a hint to open the URLs manually and an
  `ssh -L <port>:127.0.0.1:<port>` hint.
- Ctrl+C prints "shutting down" and exits 0 immediately, which releases the port and drops every
  socket within the 1 s budget with no panic output.

## Listener
- Without `--port`, 7891–7900 are tried in order. `--port 0` asks the OS. A busy explicit port, or the
  whole default range busy, exits 1 naming the port or range and suggesting `--port 0`.

## Routes
- `/` and `/l/{id}` — for any id, known or not — serve the same page: `assets/index.html` with the mode
  JSON (`{"mode":"landing"}`, `{"mode":"file"}`, or `{"mode":"stdin","maxLines":N}` for the stdin
  source) and the token substituted.
- `/assets/index.js` and `/assets/index.css` are embedded at compile time from the vendored `assets/`
  (`include_str!`/`include_bytes!`, ADR-0008); nothing is read from disk at runtime, and no WASM or
  worker route exists.
- `POST /upload`, `POST /sources/{id}/close`, `GET /ws`.
- Every response carries `Content-Security-Policy: default-src 'self'; style-src 'self'` — no
  `'unsafe-inline'`, no `'wasm-unsafe-eval'`.

## Security
- One token per process: 128 random bits, hex-encoded, embedded in the page, never in a URL.
- `Host` must name the bound address and port: an IP literal equal to the bound IP (any IP when bound
  to an unspecified address), or `localhost` on a loopback bind. A DNS-rebound name never passes.
- `/ws` upgrade: a bad `Host`, or an `Origin` that does not equal `Host`, gets 403. A missing `Origin`
  (a non-browser client) is still gated by the token.
- The first text frame must be `{"type":"auth","token":…,"source":…}` with the right token within 5 s.
  Anything else — including a query sent first — closes the socket with code 4001 and nothing sent.
- `/upload` and close require `X-Looqlog-Token`, an `Origin` equal to `Host`, and an allowed `Host`;
  otherwise 403 before any body is read and before anything is created or closed.

## Source registry
- Kinds: argv file, stdin (at most one), upload. An id is 8 lowercase RFC 4648 base32 characters
  (40 bits) from `OsRng`; ids never repeat in a process, and gone ids are remembered with their reason
  so a stale tab learns why.
- argv files and stdin are process-owned: GC and the budget never unload them. An argv file can be
  closed; closing stdin is refused (409 over HTTP) because stopping reading would block or kill the
  producer. Closing an unknown id is 404; success is 204.
- An upload is unloaded when closed, or after 10 minutes with no tab connected (`ORPHAN_GRACE`, swept
  every 30 s). The grace starts when the last tab leaves, or when the upload finishes with no tab; a
  tab reconnecting within it, such as a reload, keeps the source.
- Memory budget: 4 GiB over every source's `Session::retained_bytes()`. When an ingest goes over,
  tabless finished uploads are unloaded longest-orphaned first (reason `budget`) — never an upload
  still arriving, never a process-owned source. If it still does not fit, an upload aborts with 507
  and the budget message and its partial source is dropped; an argv file stops reading and shows the
  error. stdin records its bytes but never trips the budget: `--max-lines` bounds it.
- Removing a source marks it closed for its reader and tabs and drops its session at once.
- Landing rows: id, name, kind, size (file size or bytes ingested), state (`parsing`, `live`, `ended`,
  `error`), connected tab count, error.
- Lock order is registry, then source; a source lock is never held while taking the registry lock.

## File ingest
- An argv file is read once at startup on its own thread in 64 KiB chunks; later appends are not
  followed. A missing file, a directory or a read error is printed on the terminal and stored as that
  source's error; the server still starts.
- Above 50 MiB (`WARN_BYTES`) a file is read with a warning on the terminal and the page; above
  400 MiB (`CAP_BYTES`) it is refused with a message stating the limit, that looqlog cannot hold the
  index for a file that size, and to split or filter it. Both derive from the measured worst case of
  ~10.24 bytes retained per input byte (logfmt, `mem_probe -- session`): ~0.5 GB and ~4.1 GB. The page
  receives both numbers with the source.
- `/upload` parses the body as it streams in, never buffering it (default body limit disabled). The
  name comes from the percent-encoded `X-Looqlog-Filename`. A declared `Content-Length` over the cap
  gets 413 before reading, and received bytes are re-checked while streaming (413). An empty body is
  400. Success is 201 `{"id"}`; errors are JSON `{"error"}`. A new upload never alters another source,
  and uploads work with or without stdin.
- An upload that fails, or whose client disconnects mid-body, drops its partial source; nothing else
  changes.
- Sources with a known size report `progress`; entries are queryable while ingest runs.

## Stdin
- Read on its own thread from process start in 64 KiB chunks, independent of clients and never
  touching a socket, so an absent or slow client cannot block the producer.
- Retention is `--max-lines` raw lines in the session; the oldest are evicted and counted
  cumulatively. Ordinals are 1-based line numbers, so the first retained ordinal shows where eviction
  cut (150 lines at `--max-lines 100` start at 51).
- Format detection holds the opening lines until 100 non-empty lines arrive or 300 ms after the first
  bytes (`DETECTION_HOLD`), then runs once; it is never forced before any data.
- At EOF the session finishes and the source is `ended`; the server keeps serving the retained entries.

## `/ws` protocol
- JSON frames tagged by `type` (snake_case types, camelCase fields). Each request carries an `id` and
  gets exactly one `reply {id, data}` or `error {id, message}`. Log text travels only inside reply data
  and is never interpreted.
- Requests: `query {filters, search, range, toggled}`; `window {offset, limit, anchor?}` (`anchor`
  starts at that ordinal's current row position); `entry {ordinal}`; `buckets {span?, fullSpan}`;
  `set_format {format: json|logfmt|plain|auto|null, tzOffsetMinutes?, tzName?}` (both zone fields
  set → error `tzOffsetMinutes and tzName are mutually exclusive`; neither → UTC; core errors such as
  `unknown time zone: <name>` come back as `error`); `toggle_chain {root}` →
  `{rowCount}`; `stats` → `{source, stats}` (name, error, size warning and thresholds beside the
  session stats); `close {source}` on the landing connection only.
- Each source connection has its own `View`: one tab's query never changes another tab's windows,
  buckets or counts.
- Notifications are computed every 100 ms per client and sent only when changed, all carrying
  cumulative values so a coalesced one never makes a count wrong: `progress {bytes, total, done}`,
  `grown {total, matched, rowCount}`, `evicted {count}`, `ended`, `reset` (the source was re-parsed by
  any of its tabs; other sources' tabs get nothing), `closed {reason}`, `sources {sources}` (landing).
- The first tick after authentication carries the current totals, and `ended` if stdin already
  closed, so a late or reconnecting client positions itself without a snapshot transfer.
- An unknown id at authentication, or a source removed while connected, gets `closed` with reason
  `unknown`, `closed`, `unloaded` or `budget`; the connection stays open and data requests get errors.
- A connection authenticated without a source gets the source list right after authentication and
  whenever a row changes.
- Session work runs in `spawn_blocking`; `set_format` updates the source's retained-bytes record.
