## MODIFIED Requirements

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

## REMOVED Requirements

### Requirement: The server never reads the log file
**Reason**: ADR-0010 supersedes ADR-0002; the backend reads the argv file and receives uploads.
**Migration**: `file-ingest` defines what the backend reads; no HTTP route returns raw file bytes —
log content is returned only through authenticated `query-api` replies.
