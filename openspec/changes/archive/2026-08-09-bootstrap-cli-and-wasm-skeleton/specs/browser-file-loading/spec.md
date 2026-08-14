## ADDED Requirements

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
The embedded WASM module SHALL expose an entry point that takes the file's text and returns
a count of parsed entries, so that end-to-end plumbing is observable before any real parser
exists. In this change the entry point MAY assume JSON Lines input; format detection and
multi-format parsing belong to the `log-parsing` capability.

#### Scenario: Entry count matches the fixture
- **WHEN** the user opens `tests/fixtures/sample.jsonl`, which has a known number of lines
- **THEN** the page reports an entry count equal to that number

#### Scenario: Parse throughput is measured, not assumed
- **WHEN** a ~1MB JSON Lines fixture is parsed through this entry point
- **THEN** the wall-clock duration is measured in the browser and recorded in
  `docs/devlog.md` alongside the <200ms/MB target from TDR §11 and the command that
  produced it
