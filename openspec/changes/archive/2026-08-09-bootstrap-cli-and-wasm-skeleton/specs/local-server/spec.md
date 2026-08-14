## ADDED Requirements

### Requirement: Loopback-by-default HTTP listener
The server SHALL bind to `127.0.0.1` on port `7891` unless overridden by `--host` or
`--port`. `--port 0` SHALL allocate a free port from the operating system and the CLI SHALL
report the allocated port. A port already in use SHALL fail with a clear message naming the
port, not a panic or a backtrace.

#### Scenario: Default bind
- **WHEN** `looq app.log` is running with no overrides
- **THEN** `curl http://127.0.0.1:7891` returns HTTP 200

#### Scenario: Random port allocation
- **WHEN** the user runs `looq --port 0` twice in a row
- **THEN** each run reports a port that is open and, with high probability, different

#### Scenario: Occupied port fails cleanly
- **WHEN** port 7891 is already bound by another process and the user runs `looq app.log`
- **THEN** the process exits non-zero with a message naming port 7891 and suggesting
  `--port 0`, with no panic output

### Requirement: Static assets are served from the binary
All frontend assets — the HTML page, the JavaScript bundle and `core.wasm` — SHALL be
embedded into the binary at compile time via `include_bytes!` and served from memory. The
server SHALL NOT read any asset from the filesystem at runtime, and SHALL serve
`core.wasm` with `Content-Type: application/wasm`.

#### Scenario: Page loads with no working directory dependency
- **WHEN** the binary is copied to an empty directory and run from there
- **THEN** the page, the JS bundle and `core.wasm` are all served successfully

#### Scenario: WASM content type
- **WHEN** the browser requests `core.wasm`
- **THEN** the response carries `Content-Type: application/wasm` so streaming compilation
  is not rejected

### Requirement: The server never reads the log file
In file mode the server SHALL NOT open, read, stat for content, or expose any HTTP route
that returns the contents of the log file named on the command line (ADR-0002).

#### Scenario: No route serves log content
- **WHEN** the full set of registered routes is enumerated
- **THEN** none of them reads from the filesystem outside the embedded asset set

#### Scenario: Network tab stays empty after load
- **WHEN** a user opens a log file in the browser after the page has finished loading
- **THEN** no further HTTP request is issued for the file's contents (US-6)

### Requirement: Graceful shutdown
On `SIGINT` (Ctrl+C) the server SHALL stop accepting connections, close open WebSocket
connections, and exit with status 0 within one second.

#### Scenario: Ctrl+C exits cleanly
- **WHEN** the user presses Ctrl+C while a browser is connected
- **THEN** the process exits 0, the port is released immediately, and no panic or task-abort
  message is printed
