## Purpose

The `local-server` capability covers the HTTP listener: bind address and port
allocation, serving of compile-time-embedded assets, and graceful shutdown.
## Requirements
### Requirement: Loopback-by-default HTTP listener
The server SHALL bind to `127.0.0.1` on port `7891` unless overridden by `--host` or `--port`.
When `--port` is not given and 7891 is in use, the server SHALL try 7892 through 7900 in order and
bind the first free one. `--port 0` SHALL allocate a free port from the operating system. The CLI
SHALL report the port actually bound. An explicitly given port that is in use, or the whole default
range being in use, SHALL fail with a clear message naming the port or range and suggesting
`--port 0`, not a panic or a backtrace.

#### Scenario: Default bind
- **WHEN** `looqlog app.log` is running with no overrides and 7891 was free
- **THEN** `curl http://127.0.0.1:7891` returns HTTP 200

#### Scenario: Second process falls back
- **WHEN** one `looqlog` holds 7891 and the user starts another with no `--port`
- **THEN** the second binds 7892 and its banner says 7891 was busy

#### Scenario: Random port allocation
- **WHEN** the user runs `looqlog --port 0` twice in a row
- **THEN** each run reports a port that is open and, with high probability, different

#### Scenario: Explicit busy port fails cleanly
- **WHEN** port 7891 is bound by another process and the user runs `looqlog --port 7891 app.log`
- **THEN** the process exits non-zero with a message naming port 7891 and suggesting `--port 0`,
  with no panic output

#### Scenario: Whole default range busy
- **WHEN** ports 7891–7900 are all bound and the user runs `looqlog` with no `--port`
- **THEN** the process exits non-zero naming the range and suggesting `--port 0`

### Requirement: Static assets are served from the binary
All frontend assets — the HTML page, the JavaScript bundle and the stylesheet — SHALL be embedded
into the binary at compile time via `include_bytes!` and served from memory. The server SHALL NOT
read any asset from the filesystem at runtime. No WebAssembly module SHALL be served.

#### Scenario: Page loads with no working directory dependency
- **WHEN** the binary is copied to an empty directory and run from there
- **THEN** the page, the JS bundle and the stylesheet are all served successfully

#### Scenario: No WASM route
- **WHEN** the full set of registered routes is enumerated
- **THEN** none serves a `.wasm` file or a worker script

### Requirement: Graceful shutdown
On `SIGINT` (Ctrl+C) the server SHALL stop accepting connections, close open WebSocket
connections, and exit with status 0 within one second.

#### Scenario: Ctrl+C exits cleanly
- **WHEN** the user presses Ctrl+C while a browser is connected
- **THEN** the process exits 0, the port is released immediately, and no panic or task-abort
  message is printed

### Requirement: The page is served at the landing and at every source path
The server SHALL serve the same page at `/` and at `/l/<id>` for any id, including unknown ones,
so the client can render the landing, a source, or the closed state.

#### Scenario: Unknown id still loads the page
- **WHEN** the browser requests `/l/zzzzzzzz` for an id that never existed
- **THEN** the response is HTTP 200 with the page

