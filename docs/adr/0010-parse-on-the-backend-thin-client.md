# 0010. Parse on the backend; the browser is a thin client

- **Status:** Accepted
- **Date:** 2026-10-15
- **Supersedes:** [ADR-0002](0002-wasm-browser-parsing-file-mode.md),
  [ADR-0007](0007-argv-path-is-a-hint-not-an-auto-loaded-file.md)

## Context

ADR-0002 put parsing, indexing, filtering and search in browser-side WASM so that a file
opened in file mode never crossed a process boundary. ADR-0007 followed from it: since the
backend could not read the file, `looqlog app.log` could only print `app.log` as a hint and
the user still had to pick it in the page.

Both decisions have run into their stated costs. `wasm32` linear memory and the tab's JS heap
cap usable file size (the measured working set was ~3.4x the raw file), a page reload loses the
parsed file, and the golden path `looqlog app.log` needs an extra click that the product pitch
keeps having to apologise for. The follow-up `multi-source-tabs` change — binding a browser tab
to a specific log — needs the backend to own the parsed data in any case.

## Decision

The native backend reads and parses the log. `looqlog-core` gains the engine that used to live
in TypeScript and WASM: the time-ordered index, field filters, search (substring, `re:` regex,
`field=value`), chain-aware matching and timeline bucketing. The browser becomes a thin client
over one authenticated `/ws` socket per tab: it sends queries, pulls windows of the current
result set and renders them, and receives coalesced change notifications.

- `looqlog app.log` reads `app.log` at startup; the page shows it with no picker step.
- A file picked or dropped in the page is uploaded to the local backend (`POST /upload`) and
  parsed there.
- stdin lines are parsed as they arrive; the ring buffer (ADR-0004) evicts parsed entries.
- The `looqlog-wasm` crate, the Web Worker, `comlink` and `core.wasm` are removed.

The privacy guarantee becomes the same for both modes: log contents are read by the local
`looqlog` process and never leave the machine on the default bind (ADR-0003). The claim that a
file never leaves the browser is withdrawn everywhere.

## Alternatives considered

### Keep the engine in TypeScript, move only parsing to the backend

Rejected: the browser would still hold the whole entry set, so the memory ceiling and the
reload loss stay, and the future MCP adapter (ADR-0005) would gain nothing.

### Opt-in server-side parsing behind a flag

ADR-0002 and ADR-0007 both reserved this (`--enable-server-side-parse`, `--serve-file`).
Rejected: two engines to keep in sync, two privacy stories to explain, and the default path
would keep every cost listed in Context.

## Consequences

**Good:** no browser memory ceiling beyond the rows on screen; zero-click open from the command
line; a reload re-queries instead of re-parsing; one engine in `looqlog-core` that the MCP
adapter can reuse; WASM disappears from the build, the CI drift check and the CSP.

**Bad / accepted cost:** file contents now cross a local process boundary, so the empty
Network-tab test (PRD US-6) no longer holds and the strongest privacy claim is gone. Scrolling
and filtering pay a localhost round trip. The `re:` dialect is Rust's `regex` (no look-around,
no backreferences) instead of JavaScript's `RegExp`.

ADR-0004 stands — the bounded ring buffer and snapshot-on-connect are unchanged in purpose — but
its per-client backpressure paragraph is obsolete: clients pull rows and the backend pushes only
cumulative, coalesced notifications, so a slow client loses nothing it cannot re-query and there
is no per-client "gap" any more. The only loss left is ring-buffer eviction, which is reported.

**What would make us revisit:** a credible requirement that log contents must not reach any
process other than the browser — that would bring back a browser-side engine as an explicit,
separately documented mode rather than a reversal of this default.
