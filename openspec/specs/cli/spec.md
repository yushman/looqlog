## Purpose

The `cli` capability covers looqlog's argv surface: flag semantics, startup output, the
`--host` exposure warning, browser auto-open, and file-vs-stdin mode selection.
## Requirements
### Requirement: Argument surface
The CLI SHALL accept exactly the flags listed in TDR §6 — `--port <u16>`, `--host <ip>`,
`--open`, `--no-browser`, `--stdin`, `--max-lines <usize>`, `--version`, `--help` — plus an
optional positional log-file path. Unknown flags SHALL exit non-zero with an error naming
the offending flag.

#### Scenario: Help lists every documented flag
- **WHEN** the user runs `looqlog --help`
- **THEN** the output lists all eight flags with their defaults (`--port 7891`,
  `--host 127.0.0.1`, `--max-lines 100000`, `--open` off, `--no-browser` off)

#### Scenario: Unknown flag is rejected loudly
- **WHEN** the user runs `looqlog --colour`
- **THEN** the process exits with a non-zero status and prints an error naming `--colour`,
  and no server is started

### Requirement: Mode selection between file and stdin
The CLI SHALL run in stdin mode when `--stdin` is passed or when stdin is not a TTY, and in
file mode otherwise. A positional path SHALL be accepted in file mode and SHALL be read and parsed
by the backend at startup (`file-ingest`).

#### Scenario: Piped input selects stdin mode
- **WHEN** the user runs `echo hi | looqlog`
- **THEN** the CLI starts in stdin mode without requiring `--stdin`

#### Scenario: Path argument is opened
- **WHEN** the user runs `looqlog app.log` where `app.log` is readable
- **THEN** the CLI starts in file mode and the backend parses `app.log` without any action in
  the browser

#### Scenario: Nonexistent path still starts
- **WHEN** the user runs `looqlog does-not-exist.log`
- **THEN** the CLI prints an error naming the path and the reason and keeps serving, so the user
  can upload a file from the page instead

### Requirement: Startup output
On start the CLI SHALL print the version, the full URL the server is listening on, and how
to quit. When a positional path was given, the output SHALL name the file being opened.

#### Scenario: Banner shows the actual bound URL
- **WHEN** the user runs `looqlog --port 0 app.log`
- **THEN** the printed URL contains the port that was actually allocated, not `0`

#### Scenario: Banner names the opened file
- **WHEN** the user runs `looqlog app.log`
- **THEN** the output says `app.log` is being opened, not that the user should pick it

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
The CLI SHALL open the user's default browser at the server URL only when `--open` is
passed. `--no-browser` SHALL suppress opening even when `--open` is present. A failure to
launch a browser SHALL NOT be fatal.

#### Scenario: Opt-in open
- **WHEN** the user runs `looqlog --open app.log`
- **THEN** the default browser is launched at the printed URL

#### Scenario: Default is no browser
- **WHEN** the user runs `looqlog app.log`
- **THEN** no browser is launched and the URL is printed for manual use

#### Scenario: Headless machine degrades gracefully
- **WHEN** `--open` is passed on a machine with no browser or no display
- **THEN** the CLI prints that it could not open a browser, prints the URL and an
  `ssh -L <port>:127.0.0.1:<port>` hint, and keeps serving

