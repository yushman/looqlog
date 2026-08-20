# looqlog

Single-binary CLI that opens a local web UI for browsing a log file or a live stdin
stream. Parsing happens in WebAssembly inside your own browser, so log contents never
leave your machine.

[Русская версия](README.ru.md) · [Website](https://yushman.github.io/looqlog/)

> **Status: v0.2.0, on [crates.io](https://crates.io/crates/looqlog).** Since v0.1.0 the
> project was renamed from `looq` (see
> [ADR-0009](docs/adr/0009-project-renamed-to-looqlog.md)) and learned to group multi-line
> events: Java stack traces, Python tracebacks and pretty-printed payloads collapse into
> the entry they continue, and the timeline counts events rather than physical lines.
>
> The server, CLI, browser-side parser
> (JSON Lines / logfmt / plain text, auto-detected — with syslog, klog, Apache/CLF and
> Docker-wrapped lines read through the plain-text path, see "Supported log formats")
> and live stdin streaming exist
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
Online log viewers send your data to someone else's server. looqlog is one
binary: point it at a file or pipe stdin into it, and get a searchable, filterable,
timeline-driven UI in your own browser — privacy-first, zero config.

## Install

Download a prebuilt binary from the
[Releases page](https://github.com/yushman/looqlog/releases/latest), matching
your platform:

```bash
# Linux x86_64 (statically linked, runs on any distribution)
curl -LO https://github.com/yushman/looqlog/releases/latest/download/looqlog-0.1.0-x86_64-unknown-linux-musl
chmod +x looqlog-0.1.0-x86_64-unknown-linux-musl
./looqlog-0.1.0-x86_64-unknown-linux-musl --version
```

Also published for `aarch64-apple-darwin`, `x86_64-apple-darwin` and
`x86_64-pc-windows-msvc`. Every asset is smoke-tested on its own platform before
being published — see "Which platforms are verified how" below.

`cargo install looqlog` is planned but not published yet — publishing to crates.io is
irreversible and needs the maintainer's own token, so it isn't done casually.

> **If you already installed `v0.1.0`:** that release shipped a binary named `looq`.
> The project and every release from here on are named `looqlog`; there is no
> in-place upgrade path, and the old `looq` binary is not touched or removed by
> installing this one. Reinstall using the instructions above and, if you like,
> remove the old `looq` binary yourself.

Building from source needs only the Rust toolchain — the compiled frontend
(`core.wasm` + JS glue) is committed to the repository, so `cargo build`/`cargo
install` never invoke Node.js or `wasm-pack` (see
[ADR-0008](docs/adr/0008-vendored-frontend-artifacts.md)):

```bash
git clone https://github.com/yushman/looqlog
cd looqlog
cargo build --release
./target/release/looqlog --version
```

### Which platforms are verified how

Every release binary passes the same automated smoke test (server starts, serves
the page and `core.wasm`, streams a piped stdin line over `/ws`) before it is
published — see `.github/workflows/release.yml`. Beyond that, only
`aarch64-apple-darwin` has been run by hand through the three PRD usage flows
during development (it's this project's own dev machine). `x86_64-unknown-linux-musl`,
`x86_64-apple-darwin` (Intel Mac) and `x86_64-pc-windows-msvc` ship verified only by
the automated smoke test — their evidence beyond that is a pure-Rust dependency tree
with no `target_os`-specific code and `webbrowser` supporting all three. If one of
them turns out broken in practice, that's a bug report worth filing.

### Frontend development

The `web/` directory is a Vite + TypeScript project; it's only needed if you're
changing the frontend, not for building or installing `looqlog` itself (that path is
Rust-only, see above). `scripts/build-frontend.sh` builds it (via `wasm-pack` +
`npm`) and vendors the output into `crates/looqlog/assets/`, which is what actually
ships:

```bash
./scripts/build-frontend.sh   # requires Node.js + npm + wasm-pack
cargo build --release
```

## Usage

looqlog has two run modes, and — this matters — they give different privacy
guarantees. See [ADR-0002](docs/adr/0002-wasm-browser-parsing-file-mode.md) and TDR §12
for the full reasoning.

### File mode

```bash
looqlog app.log
```

This starts the server and prints `app.log` as a **hint** — it does not open or read
the file itself, and it can't: no browser API lets a page auto-open a path chosen by a
server (see [ADR-0007](docs/adr/0007-argv-path-is-a-hint-not-an-auto-loaded-file.md)).
Open the printed URL, then pick `app.log` yourself through the page's file picker or
drag it onto the drop target. The file is read and parsed entirely in your browser;
`looqlog` never sees its contents, which you can verify yourself — the DevTools Network
panel stays empty after the page loads, and parsing keeps working with the network
disabled.

### Stdin mode

```bash
myapp | looqlog
# or explicitly:
tail -f /var/log/app.log | looqlog --stdin
```

Lines from stdin are streamed to connected browsers over an unauthenticated localhost
WebSocket. This is a **weaker** guarantee than file mode: the data crosses a process
boundary (CLI → browser) even though it never leaves the machine — the page's mode
indicator says so explicitly, in different words from file mode's "never leaves the
browser" (TDR §12).

The backend holds a bounded ring buffer (`--max-lines`, default 100,000) filled from
process start regardless of whether a browser is connected, so `myapp | looqlog` followed
a few seconds later by opening the browser still shows everything `myapp` already
printed: a new or reloaded connection gets a snapshot of the buffer, then switches to
live streaming. A slow or absent browser never blocks the producer — under sustained
backpressure the oldest undelivered messages are dropped, and the page shows a visible
gap marker naming how many lines were lost, never a silent shorter tail. The page also
shows a live/connecting/ended/disconnected indicator, a lines/sec counter, autoscroll
that pauses when you scroll away, and reconnects with backoff if the connection drops.

### Other flags

```bash
looqlog --port 9000 app.log     # pick a port (0 = random free port)
looqlog --open app.log          # auto-open the default browser once ready
looqlog --host 0.0.0.0 app.log  # expose beyond localhost — prints a mandatory warning
```

Run `looqlog --help` for the full flag list (`--port`, `--host`, `--open`,
`--no-browser`, `--stdin`, `--max-lines`).

## Supported log formats

looqlog auto-detects **JSON Lines**, **logfmt** and **plain text** by sampling the first
100 non-empty lines; a format wins only if at least 80% of them parse under it. You can
override the choice with `#format=json|logfmt|plain` in the URL.

Plain text is not a dead end. Most real log lines are a *prefix* carrying when and how
bad, followed by a *payload* carrying everything else, so looqlog reads both:

**Timestamps.** Recognised at token starts within the first 64 bytes of the line — not
only at its very beginning, so an access log that opens with a client address still gets
a timeline:

| Shape | Example |
|---|---|
| ISO 8601 / RFC 3339 | `2026-08-08T17:42:01Z` |
| Slash date | `2026/08/08 17:42:01` |
| syslog RFC 3164 | `Aug  8 17:42:01` |
| klog / glog | `I0808 17:42:01.123456` |
| Android logcat | `04-21 13:07:53.198  1000   806   995 D ActivityManager:` |
| Apache / nginx CLF | `[08/Aug/2026:17:42:01 +0000]` |
| Epoch seconds/ms/µs | `1786000000000` (line start only) |

**Levels.** Taken first from a `level`/`lvl`/`severity` field, then from the token right
after the timestamp — `INFO`, `[INFO]`, `INFO:`, a syslog priority like `<130>`, or a
klog/logcat single letter (`I`, `W`, `E`, `D`, `V`, `F`) — and only then by scanning the
message text. syslog's eight severities fold onto the six looqlog uses
(`emerg`/`alert`/`crit` → FATAL, `err` → ERROR, `warning` → WARN, `notice`/`info` → INFO,
`debug` → DEBUG).

> **Behavior change in this release.** Because the positional token now wins over the
> message scan, `2026-08-08T17:42:01Z INFO retrying after ERROR response` reports **INFO**
> — it used to report ERROR, picked up from the word inside the message.
>
> **Behavior change in this release.** The message scan now skips a token immediately
> followed by `=`, because such a token is a key, not a level. `Done taking incident report
> err=Success` reports **no level** instead of ERROR (which it used to, through the `ERR` →
> ERROR alias). The value side is unaffected: `level=err` still means ERROR.

**Payloads.** Text after the prefix that starts with `{`, or that carries two or more
`key=value` pairs, is parsed by the JSON or logfmt parser and its members become
filterable fields. So `2026-08-08 17:42:01 INFO {"status":500,"path":"/x"}` gives you
`status` and `path` as filter chips instead of a wall of message text. Prose containing a
single `foo=bar` is left alone. A logfmt value that starts with `{` is taken whole, up to
its matching brace, so a structure dumped into a log line stays one field.

> **Behavior change in this release.** A brace-delimited logfmt value is one field now,
> not many. `time=18ms ret=204 headers={null=[HTTP/1.1 204], Alt-Svc=[h3], Content-Length=[0]}`
> gives you exactly `time`, `ret` and `headers` — where a Java map dump used to contribute
> `Alt-Svc` and `Content-Length` as top-level filter chips of their own. This is the same
> rule nested JSON objects already follow: kept as their text, not flattened.

**Android logcat.** A logcat record is recognised as a whole — `MM-DD hh:mm:ss.mmm`, two or
three uid/pid/tid columns, a severity letter from `V D I W E F`, and a `Tag:`. The letter
supplies the level (`S`, silent, is not a severity and supplies none); the columns become
the fields `tag`, `pid` and `tid`, plus `uid` when a third column is present; and the
message is the text after the tag's colon, carrying neither. Both the `uid pid tid` and
`pid tid` layouts are read, and a column may be a name rather than a number (`root`,
`shell`, `u0_a2`). The entire shape has to match through that colon before any of it is
consumed, so an ordinary line that merely opens with a date and a few numbers is not
mistaken for a record.

**Docker.** A JSON line whose members are exactly `log`, `stream` and `time` is
unwrapped: the `log` member is parsed as a line in its own right, `time` supplies the
timestamp when the inner line has none, and `stream` becomes a field. So
`docker logs > file.log` reads as the application's own log, not as an escaped blob.

**Inferred years.** syslog, klog and logcat timestamps carry no year. looqlog infers one from your
browser's clock — stepping back a year rather than dating an entry in the future — and
says so on every affected entry (*"year inferred — this timestamp shape carries none"* in
the detail view). A log older than about a year will be dated wrong; the flag is there so
you can tell.

**Multi-line events.** A stack trace, a Python traceback and a pretty-printed JSON payload
are each one event that happens to span several lines, and looqlog links those lines back to
the entry that starts them. It does so on positive evidence only: an explicit frame marker
(`at `, `Caused by:`, `... N more`, `Suppressed:`, `Traceback (most recent call last):`,
`File "…", line N`, or an exception header whose last dotted segment ends in `Exception`,
`Error` or `Throwable`), a repeated logcat prefix whose `pid`, `tid`, level and `tag` match
the line above and whose message is indented or itself a frame, or an unclosed `{` left open
by the line above. A chain only ever opens beneath a line that carried a recognised
timestamp, and a blank line closes it — which is what keeps a thread dump or a `dumpsys`
section from collapsing into one giant entry. In the table the event is one collapsible row
carrying its line count; filters are evaluated against its first line, so `level=ERROR` shows
the whole trace; and a search hit on any line surfaces the whole event, expanded, with the
matching line highlighted. A chain past 1,000 lines is closed and reported in Diagnostics
rather than truncated quietly.

> **Behavior change in this release.** The timeline counts *events*, not lines, so its totals
> no longer equal the number of table rows. A 36-frame exception is one point on the timeline
> instead of a spike that reads as 36 failures.

## Filtering, search and sharing a view

Filters, search and the active time range all narrow the *same* dataset — the
table, the timeline and every count you see agree, because they all read from one
predicate.

**Filter controls.** Every field the parser found (`level`, plus whatever fields your
logs carry) gets its own collapsible section in the left rail, listing the actual
values seen, with counts. Click a value to activate it. **Values of the same field
are OR'd together** (selecting `ERROR` and `WARN` shows both); **different fields are
AND'd** (adding `service=api` narrows to entries matching *both*). A field with too
many distinct values to usefully list (past the parser's cardinality cap, or just
impractically many to list) offers a text box instead — type a value and click Add.

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

**Column widths.** The table's `#`, timestamp and level columns are resizable: drag the
boundary at a column's right edge in the header row. The message column absorbs whatever
the other three leave, so a row always fills the width and there is never a horizontal
scrollbar. Each column has a minimum width; dragging past it stops there rather than
letting a column vanish. Double-click a boundary to restore that one column to its
default, or use the **Reset columns** button — which appears only once some width differs
from the default — to restore all of them.

**Collapsing the side panes.** The filter rail and the detail pane each carry their own
collapse button, on the pane. Collapsing gives most of the pane's width to the entry
table and leaves a narrow strip behind, holding that button and the pane's name —
**Filters**, **Details** — written vertically, so a collapsed pane always says what is
hidden and how to get it back. In the narrow stacked layout the strip becomes the
full-width bar the block collapses to instead. Collapsing never hides a warning: while a
surface inside the rail needs attention (skipped lines, or a format detection that fell
back or landed below the confidence threshold) the button on the rail's strip says so,
and a condition serious enough to open a rail section on its own — a severe skip ratio, a
fallback detection — reopens the rail as well. Selecting a row while the detail pane is
collapsed reopens it, so a selection never appears to do nothing.

**URL sharing.** Filters, search, the selected time range, a format override, a
timezone override, the table's column widths and which side panes are collapsed all
round-trip through the URL hash (`#filter=...&q=...&range=...&cols=...&panes=...`),
written a moment after you stop changing
something (not on every keystroke, so typing doesn't flood your browser history) and
applied automatically once you open the same URL and pick the same file again. Column
widths are `cols=<#>,<timestamp>,<level>` in `rem`, in that order; the message column is
derived from the other three and so has no place in the grammar, and widths that are all
at their defaults are left out of the link entirely. Collapsed panes are
`panes=rail`, `panes=detail` or `panes=rail+detail` — the key names what is *collapsed*,
so the usual state of both panes open is absent from the link entirely. **The hash contains your search text
and filter values — real fragments of your log** — so a link copied via the "Copy
shareable link" button, or the caveat shown at that moment, is telling you something
true: don't paste it somewhere the log contents shouldn't go. A URL with an unrecognised
or malformed piece applies whatever it can and tells you what it couldn't, rather than
discarding the whole thing silently — a column width that isn't a number falls back to
that column's default, and one below a column's minimum is clamped to it, and a `panes=`
value naming a pane that doesn't exist is dropped and reported while the rest of the
value still applies, so neither a bad width nor a bad pane name is ever a reason for a
log not to load.

**Live streams.** All of the above applies to `myapp | looqlog` too — filters and search
are evaluated against each line as it arrives, the entry count distinguishes what
matched from the total received, and changing a filter mid-stream re-evaluates every
retained line immediately, without reconnecting.

## Security

The server sends `Content-Security-Policy: default-src 'self'` on every response.
`script-src` also allows `'wasm-unsafe-eval'` for same-origin WASM compilation — a
deliberate, narrow addition, not a loosened default. Inline styles are **not**
permitted: `style-src` is `'self'`, with no `'unsafe-inline'`, because the
virtual-scrolled table positions its rows through rules in the page's own
stylesheet rather than through `style` attributes.

`/ws` (stdin mode's live-tail transport) is protected two ways:

- **Origin check.** A WebSocket upgrade whose `Origin` header doesn't match the
  request's own `Host` is refused with `403` before any handshake completes and
  before any stdin data can flow.
- **Per-process token handshake.** The served page embeds a random token generated
  once when `looqlog` starts. The page's own JS sends it as the first message on every
  `/ws` connection (never in the URL or query string, so it can't end up in shell
  history or a proxy log); a connection that doesn't present it within a few seconds
  is closed without any data being sent.

**What this protects against, stated plainly so it isn't mistaken for more:** a page
on a different origin — e.g. a malicious tab open in the same browser — cannot read
the token (it can't fetch your `looqlog` page's HTML cross-origin) and so cannot open
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
- **Above 400MB**, looqlog refuses to start parsing at all and explains why, rather than
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
- **A multi-line event is grouped, not merged.** The lines of a stack trace or a
  pretty-printed JSON payload are linked into one collapsible row and counted once on
  the timeline, but each line is still its own entry underneath — and the keys nested
  inside a multi-line JSON payload do not become filterable fields, because the line
  that opens it is incomplete on its own and stays message text.
- **Nested JSON objects/arrays are kept as raw text**, not flattened into dotted keys
  (`http.status`) — shown as-is in the detail view, not filterable by their nested
  fields.
- **Access-log fields aren't broken out.** An Apache/nginx combined line gets a
  timestamp and a message, but `status`, `method` and `path` do not become their own
  filterable fields — looqlog recognises timestamp shapes generically rather than shipping a
  named grammar per format, so nothing knows that the number after the request is a
  status code.
- **No user-defined format patterns.** If your in-house format's timestamp matches none
  of the shapes above, the line still becomes an entry, just without a timestamp.
- **dmesg / kernel monotonic timestamps aren't read.** `[ 1538269.814760]` is seconds
  since boot, not an instant. Turning it into one needs a boot anchor that exists only in
  a bugreport's preamble, and reading that would make the parser extract file-level
  metadata — something it deliberately doesn't do. Those lines still become entries, just
  without a timestamp.
- **No bugreport section awareness.** `------ DUMPSYS … ------` banners are ordinary
  lines to looqlog; it does not treat an Android bugreport as a container of sections with a
  format each. Every line is parsed on its own terms, which is why a bugreport's ~74% of
  `dumpsys` output stays off the timeline while its logcat sections land on it.
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
