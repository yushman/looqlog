## ADDED Requirements

### Requirement: Parsing runs off the main thread
The parser SHALL run inside a Web Worker reached through `comlink`, so that parsing a large
file leaves the page able to paint and respond to input. The main thread SHALL NOT hold a
synchronous reference to the WASM module.

#### Scenario: Page stays responsive during a large parse
- **WHEN** a 50MB file is being parsed
- **THEN** the page continues to respond to scrolling and clicks, and the progress indicator
  keeps updating

#### Scenario: Worker failure is reported, not silent
- **WHEN** the worker throws or the WASM module fails to instantiate
- **THEN** the UI shows an error naming what failed, rather than an empty result

### Requirement: Typed values cross the boundary
Entries, the detection result, the field inventory and the diagnostics SHALL cross the
JS↔WASM boundary as typed structures via `serde-wasm-bindgen`, and the TypeScript types
describing them SHALL be checked by `tsc --noEmit` in CI so a shape change in Rust cannot land
as a silent runtime mismatch.

#### Scenario: Type check catches a shape change
- **WHEN** a field is renamed in the Rust structure without updating the TypeScript type
- **THEN** CI fails on the type check

#### Scenario: Entries arrive with their fields intact
- **WHEN** a logfmt fixture carrying `service=api` is parsed
- **THEN** the JS side receives entries whose field map contains `service` with value `api`

### Requirement: Files are fed in chunks with progress
The bridge SHALL read the selected file in chunks and feed them to the parser incrementally,
reporting progress as a fraction of bytes consumed. It SHALL NOT read the whole file into a
single JavaScript string before parsing.

#### Scenario: Progress advances
- **WHEN** a multi-megabyte file is parsed
- **THEN** progress updates arrive during the parse rather than only at the end

#### Scenario: Entries appear before the file is finished
- **WHEN** the first chunks have been parsed
- **THEN** the entries from them are available to the UI without waiting for the last chunk

### Requirement: A parse can be cancelled
The bridge SHALL let the caller cancel an in-flight parse, after which no further entries or
progress from that parse SHALL reach the UI and the worker SHALL be reusable or replaced.

#### Scenario: Opening a second file cancels the first parse
- **WHEN** the user opens a new file while a parse is running
- **THEN** the first parse stops, its partial results are discarded, and only the new file's
  entries are displayed

### Requirement: Each file gets a fresh parser instance
The bridge SHALL construct a new parser instance per file so that no format decision, decoder
state, field inventory or diagnostic count carries over from a previous file.

#### Scenario: Second file of a different format
- **WHEN** a JSON fixture is opened and then a logfmt fixture is opened in the same page
- **THEN** the second file is detected as logfmt and its field inventory contains nothing from
  the first
