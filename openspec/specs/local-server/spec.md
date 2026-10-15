## Purpose

The `local-server` capability covers the HTTP listener: bind address and port
allocation, serving of compile-time-embedded assets, and graceful shutdown.
## Requirements
### Requirement: Loopback-by-default HTTP listener
The server SHALL bind to `127.0.0.1` on port `7891` unless overridden by `--host` or
`--port`. `--port 0` SHALL allocate a free port from the operating system and the CLI SHALL
report the allocated port. A port already in use SHALL fail with a clear message naming the
port, not a panic or a backtrace.

#### Scenario: Default bind
- **WHEN** `looqlog app.log` is running with no overrides
- **THEN** `curl http://127.0.0.1:7891` returns HTTP 200

#### Scenario: Random port allocation
- **WHEN** the user runs `looqlog --port 0` twice in a row
- **THEN** each run reports a port that is open and, with high probability, different

#### Scenario: Occupied port fails cleanly
- **WHEN** port 7891 is already bound by another process and the user runs `looqlog app.log`
- **THEN** the process exits non-zero with a message naming port 7891 and suggesting
  `--port 0`, with no panic output

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

