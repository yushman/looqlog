# 0007. The argv path is a hint, not an auto-loaded file

- **Status:** Accepted
- **Date:** 2026-08-09

## Context

PRD US-1 reads as if `looq app.log` opens the browser with the file already parsed, and
mvp-plan day 3 originally expected `looq tests/fixtures/sample.jsonl` to show an entry
count with no further user action. TDR §7 says the opposite: the UI opens the file
through `<input type="file">`. Only one of these can be true.

No browser API lets an `http://127.0.0.1:PORT` page open a path chosen by the server
process. `showOpenFilePicker` can at best suggest a starting directory, is
Chromium-only, and still requires a user gesture; `file://` paths are unreachable from
an `http://` origin under normal browser security policy. Under ADR-0002 — the backend
must never read the log file's contents in file mode — the file has to reach the
browser through something the user does, not something the server supplies.

## Decision

`looq app.log` starts the server and prints/shows `app.log` as a hint of which file to
open, but the browser page still requires the user to pick that file (via a file
picker or drag-and-drop) before anything is parsed. The CLI process never opens the
path itself in file mode.

## Alternatives considered

### Stream the file's bytes through the stdin transport

Make `looq app.log` behave like `cat app.log | looq`, delivering the zero-click golden
path PRD Flow 1 describes. Rejected: this moves file mode into the weaker stdin
privacy tier (TDR §12 — "does not leave the machine" rather than "does not leave the
browser") and makes US-6's empty-Network-tab test fail. That test is the single claim
that most separates looq from server-side log viewers; trading it away for one fewer
click is not worth it.

### Opt-in `--serve-file` flag

Keep the picker as default behavior, but let a user explicitly opt into
byte-streaming, mirroring the `--enable-server-side-parse` escape hatch TDR §12
already reserves. A real option, but rejected for this change as a flag with no
demand behind it yet — it can be added later if users actually complain about the
extra click, without breaking anything that exists today.

## Consequences

**Good:** the "file never leaves the browser" guarantee in file mode stays
unconditional, not "unconditional unless a convenience flag was used" — it matches
exactly what ADR-0002 promises and what US-6 tests.

**Bad / accepted cost:** the golden path is weaker than the PRD's original wording
implied. `looq app.log` differs from bare `looq` only in showing which file to open in
the browser — the pitch shrinks from "opens your log" to "opens a viewer, here's the
file to pick". Both READMEs and the page's own prompt text say this plainly instead of
letting users discover it by surprise.

**What would make us revisit:** sustained user complaints about the extra click would
justify building the opt-in `--serve-file` alternative above — a strict additive
change, not a reversal of this decision's default.
