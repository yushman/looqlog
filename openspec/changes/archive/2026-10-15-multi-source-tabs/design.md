## Context

`backend-query-engine` left one `Source` (`crates/looqlog/src/source.rs`) behind `AppState.source`,
replaced in place by uploads, with a process-wide file-vs-stdin mode. The engine is already shaped
for more: query state lives in a per-connection `View`, and `Source` already has an `id`. This
change turns the single slot into a registry and binds each tab to one entry.

Several stdin streams are handled by running several processes (explore decision); each process
gets its own port via the fallback below. There is no cross-process hub.

## Goals / Non-Goals

**Goals:**
- Many sources per process, a tab bound to exactly one, fresh landing at `/`.
- Bounded memory across sources with a predictable eviction order.
- No silent failure: a tab on a gone source says so.

**Non-Goals:**
- A shared server across `looqlog` processes.
- Persisting sources across restarts.
- Following growing files; shrinking per-entry memory (separate change if needed).
- Configurable budget or GC grace (no flags).

## Decisions

### D1. Registry and ids
`Registry { sources: HashMap<SourceId, Shared>, order: Vec<SourceId> }` behind a mutex in
`AppState`. `SourceId` is 8 characters of lowercase RFC 4648 base32 from the OS RNG (40 bits) —
unguessable enough that a stale link never lands on a different source by accident, short enough to
read. Ids are never reused within a process.

*Alternative: sequential ids.* Rejected: `/l/2` from a previous run would silently open a different
log after a restart.

### D2. Ownership and lifetime
| Kind | Owner | Close | GC | Budget eviction |
|---|---|---|---|---|
| argv file | process | yes | no | no |
| stdin (≤1) | process | no | no | no (bounded by `--max-lines`) |
| upload | tabs | yes | 10 min after last WS disconnect | yes, oldest orphan first |

Each source tracks `connected_tabs` and `orphaned_since`. A sweeper task runs every 30 s and drops
uploads orphaned for ≥10 min. Reload reconnects well inside the grace. Stdin is not closable because
stopping reading would block or `SIGPIPE` the producer; draining while discarding would make "closed"
a lie about memory, not about data.

### D3. Memory budget
`Session::retained_bytes()` (new) sums raw line bytes plus a per-entry estimate calibrated by the
`mem_probe` example; the registry total is the sum over sources. Budget = 4 GiB fixed. Ingest checks
the total per chunk:
- Upload over budget → unload orphaned uploads oldest-first; still over → abort the upload, drop
  its partial source, reply with an error naming the budget. Other sources untouched.
- argv file over budget → stop reading, mark the source `error` ("memory budget"), keep serving.
- Stdin never trips the budget check by itself; its `--max-lines` bound is counted, and other
  ingests see less headroom.

### D4. Routing and auth
- `GET /` and `GET /l/<id>` serve the same `index.html`; the client routes on `location.pathname`.
  `/l/<id>` for an unknown id still returns 200 so the page can render the closed state.
- `/ws` auth frame: `{type:"auth", token, source: id | null}`. `null` = landing connection, which
  receives `sources` snapshots (list rows) and pushes on any registry change. A source id that is
  unknown or closed → reply `closed { reason: "unknown" | "closed" | "unloaded" | "budget" }` and
  the connection stays open so the tab can upload.
- Closing a source sends `closed` to its tabs, then drops it.
- `POST /upload` → `201 {id}`; `POST /sources/<id>/close` (token/origin/host-checked) → 204, or 409
  for stdin.

### D5. CLI
Positional `paths: Vec<PathBuf>`. Stdin source is added when `--stdin` or stdin is not a TTY,
independent of paths. Banner: landing URL, then one `name → URL` line per source. `--open` opens
each source URL, or the landing when there are none.

Port: when `--port` was not given, try binding 7891..=7900 in order and report the one bound; all
busy → exit non-zero naming the range and suggesting `--port 0`. Explicit `--port N` busy → same
error as today.

### D6. ADR-0011
"Sources are owned by the process or by tabs" — records D2/D3 and why uploads are GC'd while argv
files are not.

### D7. Decisions made during apply
- "Oldest orphan" = the upload that has been without a tab the longest; an upload still being
  received is never evicted.
- `POST /upload` refused by the budget → `507 Insufficient Storage`; closing an unknown id over
  HTTP → `404`.
- A `reset` keeps the tab's filters, search and range and re-queries: sources are never replaced
  any more, so a `reset` only means "re-parsed" (another tab's `set_format`).
- Landing state wire value for a fully read file is `ended`; the landing labels it "loaded" for
  files and "ended" for stdin.
- Whether a source tab renders the live-tail or the file UI is still decided by the server filling
  the page's mode per request; the client only routes landing vs source.
- "Open another file" in a source tab links to the landing in a new tab.

## Risks / Trade-offs

- [Retained-bytes estimate drifts from real RSS] → Calibrated against `mem_probe`; budget is a
  guard, not an accounting system. Revisit if RSS exceeds budget by >25% in the probe.
- [Laptop sleeps >10 min with an upload tab open] → Tab wakes to "source unloaded, upload again".
  Accepted cost; stated in both READMEs.
- [Port fallback hides that another looqlog is running] → Banner states "7891 busy, using 7892".
- [Landing connection count adds WS load] → One small message per registry change; negligible.

## Migration Plan

Single release. Old bookmarks to `/` now land on the source list instead of the log — the log is one
click away. No persisted state.

## Open Questions

None.
