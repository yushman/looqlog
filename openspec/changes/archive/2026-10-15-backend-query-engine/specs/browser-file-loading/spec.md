## MODIFIED Requirements

### Requirement: The user selects the file in the browser
The page SHALL offer a file picker and a drag-and-drop target for opening a log file when none is
loaded, and to replace the current one. A file chosen this way SHALL be uploaded to the local
backend and parsed there (`file-ingest`). The page SHALL NOT read or parse the file's contents
itself.

#### Scenario: Picking a file parses it
- **WHEN** the user selects a local `.jsonl` file through the page's file picker
- **THEN** the file is uploaded to the backend and its parsed entries are shown

#### Scenario: Drag and drop is equivalent to picking
- **WHEN** the user drops a log file onto the page
- **THEN** it is handled identically to a picked file

## REMOVED Requirements

### Requirement: The CLI-supplied path is surfaced as a hint
**Reason**: ADR-0010 supersedes ADR-0007; the backend reads the argv path itself.
**Migration**: `looqlog app.log` now opens `app.log` directly (`file-ingest`, "The argv path is
read by the backend").

### Requirement: File contents stay in the browser
**Reason**: ADR-0010 supersedes ADR-0002; parsing runs on the backend, so the file crosses the
local process boundary.
**Migration**: The guarantee is now "never leaves your machine" for every mode; see `security`
and `live-tail-ui` for how this is stated.

### Requirement: The WASM module reports a parse result to the page
**Reason**: There is no WASM module; the page receives results through `query-api`.
**Migration**: Entry counts, detection, field inventory and diagnostics come from `query-api`
replies; throughput is measured natively (`file-ingest`, "Parse throughput is measured natively").
