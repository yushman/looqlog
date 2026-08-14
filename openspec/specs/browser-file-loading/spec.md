## Purpose

The `browser-file-loading` capability covers how a log file reaches the browser and
gets into WASM without the backend ever reading its contents (the ADR-0002/ADR-0007
contract, and the surface US-6 is verified against).

## Requirements

### Requirement: The user selects the file in the browser
The page SHALL obtain the log file through a user gesture — a file picker or a drag-and-drop
target — and SHALL NOT depend on the backend supplying the file's contents or a readable
path. No browser API can open a server-supplied path without a user gesture, so the argv
path is a hint only (ADR-0007).

#### Scenario: Picking a file parses it
- **WHEN** the user selects a local `.jsonl` file through the page's file picker
- **THEN** the file is read in the browser and parsed by the WASM module

#### Scenario: Drag and drop is equivalent to picking
- **WHEN** the user drops a log file onto the page
- **THEN** it is handled identically to a picked file

### Requirement: The CLI-supplied path is surfaced as a hint
When the CLI was given a positional path, the page SHALL display that path so the user knows
which file to select, and SHALL state that the file is read by the browser rather than by
the `looq` process.

#### Scenario: Hint names the file
- **WHEN** the user runs `looq /var/log/app.log` and opens the page
- **THEN** the page prompts to open `/var/log/app.log` and explains that the file is read
  locally by the browser

#### Scenario: No path given
- **WHEN** the user runs `looq` in file mode with no path
- **THEN** the page shows a plain "open a log file" prompt with no hint text

### Requirement: File contents stay in the browser
The page SHALL NOT transmit file contents, file names, or derived parse results to the
backend over HTTP or WebSocket in file mode (ADR-0002, US-6).

#### Scenario: Empty network activity after load
- **WHEN** the user opens a file after the page has fully loaded, with the DevTools Network
  panel recording
- **THEN** no request is issued at all, and parsing completes regardless

#### Scenario: Works with the network disabled
- **WHEN** the machine's network is disabled after the page has loaded
- **THEN** selecting and parsing a file still works

### Requirement: The WASM module reports a parse result to the page
The page SHALL parse the selected file through the real multi-format parser running in the
worker, and SHALL receive typed entries, the detection result, the field inventory and the
diagnostics — not a bare count. The provisional single-format entry point from the skeleton is
removed; format detection and multi-format parsing are the parser's own responsibility, and
the page SHALL NOT assume any particular format.

#### Scenario: Entry count matches the fixture
- **WHEN** the user opens `tests/fixtures/sample.jsonl`, which has a known number of lines
- **THEN** the page reports an entry count equal to that number, now derived from the real
  parser rather than the hardcoded one

#### Scenario: Any of the three formats works without configuration
- **WHEN** the user opens a JSON Lines, a logfmt or a plain-text fixture
- **THEN** each is detected and parsed correctly with no format hint from the CLI or the page

#### Scenario: Parse throughput is measured, not assumed
- **WHEN** a ~1MB JSON Lines fixture is parsed through the worker
- **THEN** the wall-clock duration is measured in the browser and recorded in
  `docs/devlog.md` alongside the <200ms/MB target from TDR §11, superseding the skeleton's
  stub measurement and including the cost of the worker boundary
