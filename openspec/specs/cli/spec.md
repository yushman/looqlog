## Purpose

The `cli` capability covers looqlog's argv surface: flag semantics, startup output, the
`--host` exposure warning, browser auto-open, and which sources (files, stdin) a run creates.
## Requirements
### Requirement: Argument surface
The CLI SHALL accept exactly the flags listed in TDR §6 — `--port <u16>`, `--host <ip>`,
`--open`, `--no-browser`, `--stdin`, `--max-lines <usize>`, `--version`, `--help` — plus zero or
more positional log-file paths. Unknown flags SHALL exit non-zero with an error naming the
offending flag.

#### Scenario: Help lists every documented flag
- **WHEN** the user runs `looqlog --help`
- **THEN** the output lists all eight flags with their defaults (`--port 7891`,
  `--host 127.0.0.1`, `--max-lines 100000`, `--open` off, `--no-browser` off)

#### Scenario: Unknown flag is rejected loudly
- **WHEN** the user runs `looqlog --colour`
- **THEN** the process exits with a non-zero status and prints an error naming `--colour`,
  and no server is started

#### Scenario: Several paths
- **WHEN** the user runs `looqlog a.log b.log`
- **THEN** both files are opened as separate sources

### Requirement: Mode selection between file and stdin
The CLI SHALL add a stdin source when `--stdin` is passed or when stdin is not a TTY, and SHALL
add one file source per positional path, read and parsed by the backend at startup
(`file-ingest`). Stdin and file sources SHALL coexist in one process.

#### Scenario: Piped input adds a stdin source
- **WHEN** the user runs `echo hi | looqlog`
- **THEN** the process has one stdin source without requiring `--stdin`

#### Scenario: Stdin and files together
- **WHEN** the user runs `myapp | looqlog app.log`
- **THEN** the process has a stdin source and a file source for `app.log`

#### Scenario: Nonexistent path still starts
- **WHEN** the user runs `looqlog does-not-exist.log`
- **THEN** the CLI prints an error naming the path and the reason, keeps serving, and the source
  is listed in an error state

#### Scenario: No sources at all
- **WHEN** the user runs `looqlog` from a terminal with no paths
- **THEN** the server starts with an empty registry and the landing offers an upload

### Requirement: Startup output
On start the CLI SHALL print the version, the landing URL, one line per source naming it and its
`/l/<id>` URL, and how to quit. When the default port was busy, it SHALL say which port it fell back
to.

#### Scenario: Banner shows the actual bound URL
- **WHEN** the user runs `looqlog --port 0 app.log`
- **THEN** the printed URLs contain the port that was actually allocated, not `0`

#### Scenario: Banner lists every source
- **WHEN** the user runs `cat x | looqlog a.log b.log`
- **THEN** the output has the landing URL and three source lines: `a.log`, `b.log` and stdin, each
  with its own URL

### Requirement: Exposure warning for non-loopback binds
When `--host` resolves to any address other than `127.0.0.1`, the CLI SHALL print a warning
to stdout at startup stating that log contents — the opened file, uploaded files and stdin —
become reachable from the network without authentication beyond the page token (ADR-0003). The
warning SHALL be unconditional and SHALL NOT be suppressible by any flag.

#### Scenario: Binding to all interfaces warns
- **WHEN** the user runs `looqlog --host 0.0.0.0 app.log`
- **THEN** stdout contains a warning naming file and stdin exposure before the ready banner

#### Scenario: Loopback bind stays quiet
- **WHEN** the user runs `looqlog` with no `--host`
- **THEN** no exposure warning is printed

### Requirement: Browser auto-open is opt-in
The CLI SHALL open the user's default browser only when `--open` is passed: one tab per source at
its `/l/<id>` URL, or the landing when there are no sources. `--no-browser` SHALL suppress opening
even when `--open` is present. A failure to launch a browser SHALL NOT be fatal.

#### Scenario: Opt-in open
- **WHEN** the user runs `looqlog --open a.log b.log`
- **THEN** two browser tabs open, one on each source's URL

#### Scenario: Open with no sources
- **WHEN** the user runs `looqlog --open` with no paths and stdin a TTY
- **THEN** one tab opens on the landing

#### Scenario: Default is no browser
- **WHEN** the user runs `looqlog app.log`
- **THEN** no browser is launched and the URLs are printed for manual use

#### Scenario: Headless machine degrades gracefully
- **WHEN** `--open` is passed on a machine with no browser or no display
- **THEN** the CLI prints that it could not open a browser, prints the URLs and an
  `ssh -L <port>:127.0.0.1:<port>` hint, and keeps serving

