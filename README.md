# looqlog

Single-binary CLI that opens a local web UI for browsing log files and a live stdin
stream. Parsing happens in the local `looqlog` process and the browser only shows the
result, so log contents never leave your machine.

[Русская версия](README.ru.md) · [Website](https://yushman.github.io/looqlog/)

> **Status: v0.3.0, on [crates.io](https://crates.io/crates/looqlog).** v0.3.0 moves
> parsing out of the browser into the local `looqlog` process (see
> [ADR-0010](docs/adr/0010-parse-on-the-backend-thin-client.md)): a file named on the
> command line opens straight into the log with no file picker, one process holds several
> files and stdin side by side under a source list at `/`, and a file picked or dropped
> there is uploaded to the local process. The `#format=` and `#tz=` URL overrides now work
> on a live stdin stream too, and `#tz=` takes an IANA zone name (`#tz=Europe/Berlin`) as
> well as a fixed offset. Earlier releases read the padded tags of `adb logcat` (v0.2.1),
> renamed the project from `looq` (see
> [ADR-0009](docs/adr/0009-project-renamed-to-looqlog.md)) and grouped multi-line events:
> Java stack traces, Python tracebacks and pretty-printed payloads collapse into the entry
> they continue, and the timeline counts events rather than physical lines.
>
> The server, CLI, parser
> (JSON Lines / logfmt / plain text, auto-detected — with syslog, klog, Apache/CLF and
> Docker-wrapped lines read through the plain-text path, see "Supported log formats")
> and live stdin streaming exist
> end to end. Parsing, indexing, filtering and search run natively in the `looqlog`
> process; the page asks for the rows it shows and never holds the whole log, so a
> large file neither freezes the page nor fills the tab's memory. Stdin holds a
> bounded ring buffer so lines emitted before a
> browser connects are not lost, and a slow client never blocks the producer. A
> `uPlot` histogram timeline shows entries per time bucket with drag-to-select range
> narrowing, and a virtual-scrolled table renders only the visible rows regardless of
> dataset size (50,000 entries scroll smoothly). Row selection opens a detail view
> with the full message and every extracted field. Filter chips, full-text/regex
> search and a shareable URL hash narrow the same dataset the timeline and table show
> — see "Filtering, search and sharing a view" below. Both surfaces track a live
> stream that grows and evicts, filters included. A CSP, origin and host checks and
> a per-process auth token close the cross-tab hijacking gap on `/ws` and `/upload` (see
> "Security"); file-size limits, dedicated error messages and a light/dark theme are
> in place (see the sections below). Styling is minimal, not polished.

## Why

`grep`/`awk`/`less` have no timeline or structure. Kibana/Grafana need infrastructure.
Online log viewers send your data to someone else's server. looqlog is one
binary: point it at a file or pipe stdin into it, and get a searchable, filterable,
timeline-driven UI in your own browser — privacy-first, zero config.

## Install

```bash
cargo install looqlog
looqlog --version
```

This installs the current release, `0.3.0`, from
[crates.io](https://crates.io/crates/looqlog), and needs nothing but a Rust toolchain.

Or take a prebuilt binary, which needs no Rust at all:

```bash
# Linux x86_64, statically linked against musl
curl -LO https://github.com/yushman/looqlog/releases/download/v0.3.0/looqlog-0.3.0-x86_64-unknown-linux-musl
chmod +x looqlog-0.3.0-x86_64-unknown-linux-musl
./looqlog-0.3.0-x86_64-unknown-linux-musl --version
```

Also built for `aarch64-apple-darwin`, `x86_64-apple-darwin` and
`x86_64-pc-windows-msvc`. Every asset is smoke-tested on its own platform before being
published — see "Which platforms are verified how" below for what that does and does not
prove.

The URL above is pinned to `v0.3.0` on purpose: asset names carry the version, so a
`releases/latest/download/` link breaks the moment a newer release exists. For any later
version, take the file name from the
[Releases page](https://github.com/yushman/looqlog/releases/latest).

> **If you already installed `v0.1.0`:** that release shipped a binary named `looq`.
> The project and every release from here on are named `looqlog`; there is no
> in-place upgrade path, and the old `looq` binary is not touched or removed by
> installing this one. Reinstall using the instructions above and, if you like,
> remove the old `looq` binary yourself.

Building from source needs only the Rust toolchain — the compiled frontend (the JS
bundle and the stylesheet) is committed to the repository, so `cargo build`/`cargo
install` never invoke Node.js (see
[ADR-0008](docs/adr/0008-vendored-frontend-artifacts.md)):

```bash
git clone https://github.com/yushman/looqlog
cd looqlog
cargo build --release
./target/release/looqlog --version
```

### Which platforms are verified how

Every release binary passes the same automated smoke test (server starts, serves
the page, and a piped stdin line becomes queryable over `/ws`) before it is
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
Rust-only, see above). `scripts/build-frontend.sh` builds it (via `npm`) and vendors
the output into `crates/looqlog/assets/`, which is what actually ships:

```bash
./scripts/build-frontend.sh   # requires Node.js + npm
cargo build --release
```

## Usage

looqlog reads log files and a stdin stream, each as its own **source**. Every source is
read and parsed by the local `looqlog` process and reaches the page over a localhost
connection, so it never leaves your machine on the default bind — it does cross a process
boundary, and nothing claims it stays inside the browser. See
[ADR-0010](docs/adr/0010-parse-on-the-backend-thin-client.md) for the reasoning.

### Sources, the source list and tabs

```bash
looqlog app.log                      # one file
looqlog app.log worker.log           # several files, each its own source
myapp | looqlog app.log              # stdin and a file side by side
```

The banner prints the source list URL and one URL per source:

```
→ http://127.0.0.1:7891  (all sources)
  app.log → http://127.0.0.1:7891/l/k3x9q2ma
  stdin (max-lines: 100000) → http://127.0.0.1:7891/l/p7d4ve2c
```

- **`/` is the source list**: every source with its kind, size, state (parsing, live,
  ended/loaded, error) and number of open tabs, a link to each, a **Close** button (for
  everything except stdin), and a file picker/drop target. A new tab at `/` always starts
  here — it never inherits another tab's log or filters.
- **`/l/<id>` is one source.** A tab stays bound to its source; a reload keeps it. The id
  is random, so a link from an earlier run never opens a different log: it says the source
  is gone (unknown, closed, unloaded) and offers an upload instead of an empty table.

### Files

Each path on the command line is read and parsed at startup, and its URL opens straight
into the log — no file picker step; progress is shown while a large file is still being
read. A file is read once: lines appended later are not followed. If a path cannot be
read, `looqlog` says so on the terminal and keeps serving, and that source shows the same
error with a way to open another file. A file named on the command line stays until
`looqlog` exits or you close it from the source list.

A file can also be picked or dropped on the source list. It is **uploaded to the local
`looqlog` process** (not to any other server), becomes a new source, and the tab opens it.
It never replaces what another tab shows; to open another file from a log's tab, use the
source list. An upload is kept while a tab shows it: once no tab has been connected to it
for **10 minutes** it is unloaded, and a tab coming back later (say, after the laptop
slept) says so. A failed upload leaves no source behind and touches no other source.

### Stdin

```bash
myapp | looqlog
# or explicitly:
tail -f /var/log/app.log | looqlog --stdin
```

When stdin is a pipe (or `--stdin` is passed) `looqlog` adds a stdin source, alongside any
files. Lines are parsed as they arrive; connected pages are told the stream grew and fetch
the rows they show over the localhost WebSocket.

The backend retains the most recent `--max-lines` lines (default 100,000), filled from
process start regardless of whether a browser is connected, so `myapp | looqlog` followed
a few seconds later by opening the browser still shows everything `myapp` already
printed, and a reload keeps the history. A slow or absent browser never blocks the
producer and loses nothing it cannot fetch again. Once the limit is reached the oldest
lines are evicted, and the page shows a marker above the first row naming how many —
never a silently shorter tail. The page also
shows a live/connecting/ended/disconnected indicator, a lines/sec counter, autoscroll
that pauses when you scroll away, and reconnects with backoff if the connection drops.
The stdin source cannot be closed: stopping reading would block or kill the program
writing to it.

**Several streams are several processes.** One `looqlog` reads one stdin. Pipe a second
stream into a second `looqlog`: with no `--port`, it takes the next free port from 7891 to
7900 and says which port was busy.

### Other flags

```bash
looqlog --port 9000 app.log     # pick a port (0 = random free port)
looqlog --open a.log b.log      # open one browser tab per source (the source list if none)
looqlog --host 0.0.0.0 app.log  # expose beyond localhost — prints a mandatory warning
```

With no `--port`, `looqlog` binds 7891, or the next free port up to 7900 when that one is
busy; when all ten are busy it exits and suggests `--port 0`. An explicit `--port` that is
busy fails the same way instead of falling back.

Run `looqlog --help` for the full flag list (`--port`, `--host`, `--open`,
`--no-browser`, `--stdin`, `--max-lines`).

## Supported log formats

looqlog auto-detects **JSON Lines**, **logfmt** and **plain text** by sampling the first
100 non-empty lines; a format wins only if at least 80% of them parse under it. You can
override the choice with `#format=json|logfmt|plain` in the URL. The override re-parses the
source itself, so every open tab on that source switches with it; other sources are not
affected.

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

`adb logcat | looqlog` works on the default output, with no `-v` flag of your own.
`-v threadtime` pads short tags out to a fixed column width — `I vold    : Vold 3.0
firing up` — and that padding is read as padding: `vold    :` is recognised exactly as
`ActivityManager:` is, and the `tag` field carries `vold`, not `vold    `, so `tag=vold`
is what you type to filter on it.

**Docker.** A JSON line whose members are exactly `log`, `stream` and `time` is
unwrapped: the `log` member is parsed as a line in its own right, `time` supplies the
timestamp when the inner line has none, and `stream` becomes a field. So
`docker logs > file.log` reads as the application's own log, not as an escaped blob.

**Inferred years.** syslog, klog and logcat timestamps carry no year. looqlog infers one from the
clock of the machine it runs on — stepping back a year rather than dating an entry in the future — and
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
same. Expressions use the Rust [`regex`](https://docs.rs/regex) syntax: **look-around
(`(?=…)`, `(?<!…)`) and backreferences (`\1`) are not supported**, and an expression
using them is reported as an error rather than run. To search for literal text that itself starts with `re:`, escape it with a
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
applied automatically when you open the same URL while its source is still loaded — the
`/l/<id>` part names a source of the running `looqlog` process, so the link does not work
on another machine or after that process exits. Column
widths are `cols=<#>,<timestamp>,<level>` in `rem`, in that order; the message column is
derived from the other three and so has no place in the grammar, and widths that are all
at their defaults are left out of the link entirely. Collapsed panes are
`panes=rail`, `panes=detail` or `panes=rail+detail` — the key names what is *collapsed*,
so the usual state of both panes open is absent from the link entirely. **Timezone.** `#tz=` takes either
a fixed offset in minutes (`#tz=180`, `|n| < 1440`) or an IANA name (`#tz=Europe/Berlin`),
applied to timestamps that carry no offset of their own. A name is resolved per
timestamp, so summer and winter lines get their own offsets; a local time that occurs
twice (clocks going back) takes the earlier instant, and one that never happens (clocks
going forward) uses the offset in effect before the transition. A name the embedded
timezone database doesn't know shows "could not apply the format override: unknown time
zone: …". Times are still displayed in UTC. **The hash contains your search text
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

The server sends `Content-Security-Policy: default-src 'self'; style-src 'self'` on
every response — same-origin scripts, styles and the `/ws` connection, nothing more.
Inline styles are **not** permitted: `style-src` is `'self'`, with no `'unsafe-inline'`, because the
virtual-scrolled table positions its rows through rules in the page's own
stylesheet rather than through `style` attributes.

`/ws` (every query the page makes and the source list), `/upload` (a file picked in the
page) and `/sources/<id>/close` are protected two ways:

- **Origin and Host check.** A request whose `Origin` doesn't match its own `Host`, or
  whose `Host` doesn't name the address `looqlog` is bound to (a DNS-rebound name such
  as `attacker.example`), is refused with `403` before any log data is sent and before
  any uploaded byte is parsed or any source is closed.
- **Per-process token handshake.** The served page embeds a random token generated
  once when `looqlog` starts. The page's own JS sends it as the first message on every
  `/ws` connection and in the `X-Looqlog-Token` header of an upload (never in the URL
  or query string, so it can't end up in shell history or a proxy log); a connection
  that doesn't present it within a few seconds is closed without any data being sent.

**What this protects against, stated plainly so it isn't mistaken for more:** a page
on a different origin — e.g. a malicious tab open in the same browser — cannot read
the token (it can't fetch your `looqlog` page's HTML cross-origin) and so cannot read
the log or upload one. It does **not** protect against another process on the same
machine, which can just fetch the page itself the same way a browser would. It does
**not** add any protection to `--host 0.0.0.0` or any other non-loopback bind —
anyone who can reach that host and port can fetch the page and then the token, same
as a legitimate browser tab, and with it the file and the stdin stream. That gap is
accepted, not closed
([ADR-0003](docs/adr/0003-bind-127-0-0-1-by-default.md)): binding beyond `127.0.0.1`
always prints a mandatory warning naming exactly this.

## File size limits

The `looqlog` process holds every retained entry, its index and its raw line in memory.
Measured (`cargo run -p looqlog-core --release --example mem_probe -- session 64`, see
`docs/devlog.md`): retained memory is about 3.5x the file size for plain text, 7.3x for
JSON Lines and 10.2x for logfmt. The limits follow from the worst of those:

- **Above 50 MB** a file is still opened, with a warning — from the command line it is
  printed on the terminal and shown on the page, and an upload from the page asks to
  continue or cancel first. Past this point parsing takes a while and memory use reaches
  about half a gigabyte.
- **Above 400 MB** looqlog refuses the file (up to ~4 GB of memory at the measured worst
  case) and explains why, rather than starting a parse that could fail partway through.
  Split the file (e.g. by time range) or filter it down first.

All sources of one process share a fixed **4 GB memory budget**, counted from what each
source actually retains (calibrated against the same measurement, see
[ADR-0011](docs/adr/0011-sources-owned-by-process-or-tabs.md)). When a new upload does not
fit, uploads that no tab is showing are unloaded oldest first; if it still does not fit,
the upload is refused with an error naming the budget and every other source stays as it
was. A file from the command line that does not fit stops being read and shows the same
error. Files from the command line and stdin are never unloaded to make room.

## Error states

Every failure mode has a specific, persistent message instead of a blank screen, an
infinite spinner, or a console-only error: an empty file, a file that looks binary
(NUL bytes near the start — offers a "proceed anyway" override, since a log with one
stray NUL byte is a real if rare case), a file `looqlog` or the browser couldn't read
(naming the path), an upload the backend rejected (with its reason), a lost connection
to the `looqlog` process, and a format that parsed to zero entries (read distinctly
from an empty file and from a filter matching nothing). Errors stay on screen until
you dismiss them or open another file successfully — never a toast that fades on its
own — and a failed second file never clears what the first one already loaded.

## Theme

Light and dark, following your OS preference by default. The toggle in the top bar
cycles Auto → Light → Dark → Auto; an explicit choice is remembered (`localStorage`)
across reloads on the same machine. No external fonts, stylesheets or images are used
anywhere, so both appearances render fully with the network disabled.

## Known limitations

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

## License

MIT
