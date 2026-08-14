# looq

Single-binary CLI that opens a local web UI for browsing a log file or a live stdin
stream. Parsing happens in WebAssembly inside your own browser, so log contents never
leave your machine.

[Русская версия](README.ru.md)

> **Status: v0.1.0, feature-complete MVP.** The server, CLI, browser-side parser
> (JSON Lines / logfmt / plain text, auto-detected) and live stdin streaming exist
> end to end. File mode runs the parser in a Web Worker so a large file never
> freezes the page; stdin mode holds a bounded ring buffer so lines emitted before a
> browser connects are not lost, and a slow client never blocks the producer. A
> `uPlot` histogram timeline shows entries per time bucket with drag-to-select range
> narrowing, and a virtual-scrolled table renders only the visible rows regardless of
> dataset size (50,000 entries scroll smoothly). Row selection opens a detail view
> with the full message and every extracted field. Filter chips, full-text/regex
> search and a shareable URL hash narrow the same dataset the timeline and table show
> — see "Filtering, search and sharing a view" below. Both surfaces track a live
> stream that grows and evicts, filters included. A CSP, a WebSocket origin check and
> a per-process auth token close the cross-tab hijacking gap on `/ws` (see
> "Security"); file-size limits, dedicated error messages and a light/dark theme are
> in place (see the sections below). Styling is minimal, not polished.

## Why

`grep`/`awk`/`less` have no timeline or structure. Kibana/Grafana need infrastructure.
Online log viewers send your data to someone else's server. looq is one
binary: point it at a file or pipe stdin into it, and get a searchable, filterable,
timeline-driven UI in your own browser — privacy-first, zero config.

## Install

```bash
cargo install looq   # planned — not published yet
```

Building from source needs only the Rust toolchain — the compiled frontend
(`core.wasm` + JS glue) is committed to the repository, so `cargo build`/`cargo
install` never invoke Node.js or `wasm-pack` (see
[ADR-0008](docs/adr/0008-vendored-frontend-artifacts.md)):

```bash
git clone https://github.com/looq-dev/looq
cd looq
cargo build --release
./target/release/looq --version
```

A prebuilt release binary will be published once v0.1.0 ships (see Releases). The
release target is Linux x86_64 at minimum (TDR §5); sizes recorded for both that
target and the platform this repo's own release build was actually produced on are
in `docs/devlog.md`'s `release-hardening` entry.

### Frontend development

The `web/` directory is a Vite + TypeScript project; it's only needed if you're
changing the frontend, not for building or installing `looq` itself (that path is
Rust-only, see above). `scripts/build-frontend.sh` builds it (via `wasm-pack` +
`npm`) and vendors the output into `crates/looq/assets/`, which is what actually
ships:

```bash
./scripts/build-frontend.sh   # requires Node.js + npm + wasm-pack
cargo build --release
```

## Usage

looq has two run modes, and — this matters — they give different privacy
guarantees. See [ADR-0002](docs/adr/0002-wasm-browser-parsing-file-mode.md) and TDR §12
for the full reasoning.

### File mode

```bash
looq app.log
```

This starts the server and prints `app.log` as a **hint** — it does not open or read
the file itself, and it can't: no browser API lets a page auto-open a path chosen by a
server (see [ADR-0007](docs/adr/0007-argv-path-is-a-hint-not-an-auto-loaded-file.md)).
Open the printed URL, then pick `app.log` yourself through the page's file picker or
drag it onto the drop target. The file is read and parsed entirely in your browser;
`looq` never sees its contents, which you can verify yourself — the DevTools Network
panel stays empty after the page loads, and parsing keeps working with the network
disabled.

### Stdin mode

```bash
myapp | looq
# or explicitly:
tail -f /var/log/app.log | looq --stdin
```

Lines from stdin are streamed to connected browsers over an unauthenticated localhost
WebSocket. This is a **weaker** guarantee than file mode: the data crosses a process
boundary (CLI → browser) even though it never leaves the machine — the page's mode
indicator says so explicitly, in different words from file mode's "never leaves the
browser" (TDR §12).

The backend holds a bounded ring buffer (`--max-lines`, default 100,000) filled from
process start regardless of whether a browser is connected, so `myapp | looq` followed
a few seconds later by opening the browser still shows everything `myapp` already
printed: a new or reloaded connection gets a snapshot of the buffer, then switches to
live streaming. A slow or absent browser never blocks the producer — under sustained
backpressure the oldest undelivered messages are dropped, and the page shows a visible
gap marker naming how many lines were lost, never a silent shorter tail. The page also
shows a live/connecting/ended/disconnected indicator, a lines/sec counter, autoscroll
that pauses when you scroll away, and reconnects with backoff if the connection drops.

### Other flags

```bash
looq --port 9000 app.log     # pick a port (0 = random free port)
looq --open app.log          # auto-open the default browser once ready
looq --host 0.0.0.0 app.log  # expose beyond localhost — prints a mandatory warning
```

Run `looq --help` for the full flag list (`--port`, `--host`, `--open`,
`--no-browser`, `--stdin`, `--max-lines`).

## Filtering, search and sharing a view

Filter chips, search and the active time range all narrow the *same* dataset — the
table, the timeline and every count you see agree, because they all read from one
predicate.

**Filter chips.** Every field the parser found (`level`, plus whatever fields your
logs carry) gets a row of chips built from the actual values seen, with counts. Click
a value to activate it. **Values of the same field are OR'd together** (selecting
`ERROR` and `WARN` shows both); **different fields are AND'd** (adding `service=api`
narrows to entries matching *both*). A field with too many distinct values to
usefully list (past the parser's cardinality cap, or just impractically many for a
row of buttons) offers a text box instead — type a value and click Add.

**Search.** The search box matches message text and field values, case-insensitive,
substring by default. Prefix a query with `re:` for a regular expression, applied
exactly as written (no implicit case-insensitivity) — `re:^ERROR.*timeout`. An
invalid regex shows an inline error and leaves the previous results on screen rather
than emptying the table, so "bad regex" and "valid search, no matches" never look the
same. To search for literal text that itself starts with `re:`, escape it with a
leading backslash: `\re:something`. Typing `field=value` for a field that exists
(e.g. `service=api`) turns it into a chip instead of searching for the literal text —
so clicking a chip and typing its `field=value` shorthand produce the same result.
Press Escape to clear the search box without clearing your chips.

**URL sharing.** Filters, search, the selected time range, a format override and a
timezone override all round-trip through the URL hash (`#filter=...&q=...&range=...`),
written a moment after you stop changing something (not on every keystroke, so typing
doesn't flood your browser history) and applied automatically once you open the same
URL and pick the same file again. **The hash contains your search text and filter
values — real fragments of your log** — so a link copied via the "Copy shareable
link" button, or the caveat shown at that moment, is telling you something true: don't
paste it somewhere the log contents shouldn't go. A URL with an unrecognised or
malformed piece applies whatever it can and tells you what it couldn't, rather than
discarding the whole thing silently.

**Live streams.** All of the above applies to `myapp | looq` too — filters and search
are evaluated against each line as it arrives, the entry count distinguishes what
matched from the total received, and changing a filter mid-stream re-evaluates every
retained line immediately, without reconnecting.

## Security

The server sends `Content-Security-Policy: default-src 'self'` on every response
(`script-src` also allows `'wasm-unsafe-eval'` for same-origin WASM compilation and
`style-src` allows `'unsafe-inline'` for the virtual-scrolled table's per-row
positioning — both deliberate, narrow additions, not a loosened default).

`/ws` (stdin mode's live-tail transport) is protected two ways:

- **Origin check.** A WebSocket upgrade whose `Origin` header doesn't match the
  request's own `Host` is refused with `403` before any handshake completes and
  before any stdin data can flow.
- **Per-process token handshake.** The served page embeds a random token generated
  once when `looq` starts. The page's own JS sends it as the first message on every
  `/ws` connection (never in the URL or query string, so it can't end up in shell
  history or a proxy log); a connection that doesn't present it within a few seconds
  is closed without any data being sent.

**What this protects against, stated plainly so it isn't mistaken for more:** a page
on a different origin — e.g. a malicious tab open in the same browser — cannot read
the token (it can't fetch your `looq` page's HTML cross-origin) and so cannot open
the stdin stream. It does **not** protect against another process on the same
machine, which can just fetch the page itself the same way a browser would. It does
**not** add any protection to `--host 0.0.0.0` or any other non-loopback bind —
anyone who can reach that host and port can fetch the page and then the token, same
as a legitimate browser tab. That gap is accepted, not closed
([ADR-0003](docs/adr/0003-bind-127-0-0-1-by-default.md)): binding beyond `127.0.0.1`
always prints a mandatory warning naming exactly this.

## File size limits

Parsing happens by holding every entry and its index in browser memory, so there's a
practical ceiling well below the nominal 4GB a 32-bit address space suggests.
Measured (not guessed — see `docs/devlog.md`'s `release-hardening` entry): the
browser-side working set (`entries` + the time-index) grows at a strikingly
consistent ~3.4x the raw file size. Two thresholds follow from that:

- **Above ~50MB**, opening a file shows a warning (continue/cancel) — parsing and
  memory use both become noticeable past this point.
- **Above 400MB**, looq refuses to start parsing at all and explains why, rather than
  starting a parse that could fail partway through with an out-of-memory error you
  can't interpret. Split the file (e.g. by time range) or filter it down first.

## Error states

Every failure mode has a specific, persistent message instead of a blank screen, an
infinite spinner, or a console-only error: an empty file, a file that looks binary
(NUL bytes near the start — offers a "proceed anyway" override, since a log with one
stray NUL byte is a real if rare case), a file the browser couldn't read, a WASM
module that failed to load, and a format that parsed to zero entries (read distinctly
from an empty file and from a filter matching nothing). Errors stay on screen until
you dismiss them or open another file successfully — never a toast that fades on its
own — and a failed second file never clears what the first one already loaded.

## Theme

Light and dark, following your OS preference by default. The toggle in the top bar
cycles Auto → Light → Dark → Auto; an explicit choice is remembered (`localStorage`)
across reloads on the same machine. No external fonts, stylesheets or images are used
anywhere, so both appearances render fully with the network disabled.

## Known limitations

- **Named timezones aren't supported** — only UTC and fixed offsets. A timestamp
  written in `Europe/Belgrade` local time with no explicit offset is read as UTC. A
  full IANA timezone database (`chrono-tz`) would very likely have blown the
  `core.wasm` size budget the same way `regex` did (see `docs/devlog.md`).
- **One physical line = one entry.** Multi-line payloads (e.g. a Java stack trace)
  become N separate table rows, not one aggregated entry — a deliberate MVP scope
  cut, not a parser bug.
- **Nested JSON objects/arrays are kept as raw text**, not flattened into dotted keys
  (`http.status`) — shown as-is in the detail view, not filterable by their nested
  fields.
- **No gzip/zstd decompression, no multi-file merge, no export** — all explicitly out
  of MVP scope (`docs/PRD.md` §8).

## Design docs

- `docs/PRD.md`, `docs/TDR.md` — product and technical requirements
- `docs/adr/` — architecture decisions and the alternatives that were rejected
- `docs/mvp-plan.md` — the day-by-day build plan
- `docs/devlog.md` — build log, with real measured numbers
- `openspec/` — the spec-driven change process this project follows; see
  `openspec/specs/` for the currently accepted behavior

## License

MIT
