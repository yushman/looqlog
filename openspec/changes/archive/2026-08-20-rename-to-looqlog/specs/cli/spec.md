## MODIFIED Requirements

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
file mode otherwise. A positional path SHALL be accepted in file mode and SHALL NOT be
opened or read by the CLI process.

#### Scenario: Piped input selects stdin mode
- **WHEN** the user runs `echo hi | looqlog`
- **THEN** the CLI starts in stdin mode without requiring `--stdin`

#### Scenario: Path argument does not touch the file
- **WHEN** the user runs `looqlog app.log` where `app.log` is readable
- **THEN** the CLI starts in file mode and never opens `app.log` (verifiable with `strace`
  or an equivalent syscall trace containing no `open` of that path)

#### Scenario: Nonexistent path still starts
- **WHEN** the user runs `looqlog does-not-exist.log`
- **THEN** the CLI prints a warning that the path could not be verified and starts anyway,
  because path validity is the browser's concern, not the backend's

### Requirement: Startup output
On start the CLI SHALL print the version, the full URL the server is listening on, and how
to quit. When a positional path was given, the output SHALL name the file the user is
expected to open in the browser.

#### Scenario: Banner shows the actual bound URL
- **WHEN** the user runs `looqlog --port 0 app.log`
- **THEN** the printed URL contains the port that was actually allocated, not `0`

### Requirement: Exposure warning for non-loopback binds
When `--host` resolves to any address other than `127.0.0.1`, the CLI SHALL print a warning
to stdout at startup stating that live stdin logs become reachable from the network without
authentication (ADR-0003). The warning SHALL be unconditional and SHALL NOT be suppressible
by any flag in this change.

#### Scenario: Binding to all interfaces warns
- **WHEN** the user runs `looqlog --host 0.0.0.0`
- **THEN** stdout contains a warning naming the unauthenticated WebSocket exposure before
  the ready banner

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
