## ADDED Requirements

### Requirement: The argv path is read by the backend
When the CLI is given a positional path, the backend SHALL read and parse that file at startup,
once, without following later appends. The page SHALL show the log without requiring the user to
pick it. A path that cannot be read SHALL NOT prevent the server from starting; the failure SHALL
be reported on the page and printed on the terminal.

#### Scenario: Zero-click open
- **WHEN** the user runs `looqlog tests/fixtures/sample.jsonl` and opens the printed URL
- **THEN** the page shows the fixture's entries with no file picker step

#### Scenario: Unreadable path
- **WHEN** the user runs `looqlog does-not-exist.log`
- **THEN** the server starts, the terminal names the path and the error, and the page shows the
  same error with an option to upload a file instead

#### Scenario: Large argv file
- **WHEN** the argv file is above the warning threshold and below the hard cap
- **THEN** it is read without a confirmation step, and the terminal and the page both show the
  size warning

#### Scenario: Argv file above the hard cap
- **WHEN** the argv file is above the hard cap
- **THEN** it is not read, and the terminal and the page both state the limit and the reason

#### Scenario: Appends after startup are not followed
- **WHEN** lines are appended to the file after it has been fully read
- **THEN** the page's entry count does not change

### Requirement: Upload from the browser
The page SHALL let the user pick or drop a file, and SHALL send its contents to the backend with
`POST /upload`, streamed, with the token in the `X-Looqlog-Token` header. The backend SHALL parse
the body as it arrives and SHALL NOT buffer the whole body before parsing. A successful upload
SHALL replace the current file source and notify connected clients with `reset`.

#### Scenario: Picked file is shown
- **WHEN** the user picks a `.jsonl` fixture in the page
- **THEN** it is uploaded, parsed by the backend, and its entries are shown

#### Scenario: Drop is equivalent to pick
- **WHEN** the user drops a log file onto the page
- **THEN** it is handled identically to a picked file

#### Scenario: Upload is refused in stdin mode
- **WHEN** the process is in stdin mode and an upload is attempted
- **THEN** the backend rejects it with a message that this process is showing a stream

### Requirement: Ingest reports progress
While a file is being parsed, the backend SHALL report progress as bytes ingested out of the total,
and entries parsed so far SHALL be queryable before the file is finished.

#### Scenario: Entries before the end
- **WHEN** a 50 MB file is being ingested
- **THEN** the page shows progress advancing and can scroll the entries parsed so far

### Requirement: A new ingest cancels the previous one
Starting an upload while a previous file is still being ingested SHALL stop the previous ingest;
none of its entries SHALL remain once the new file's ingest has begun successfully.

#### Scenario: Second upload mid-parse
- **WHEN** the user uploads a second file while the first is still parsing
- **THEN** only the second file's entries are shown and the field inventory contains nothing from
  the first

### Requirement: Parse throughput is measured natively
Parse throughput and index-build cost SHALL be measured with `cargo bench` on `looqlog-core` and
recorded in `docs/devlog.md` against the <200 ms/MB target (TDR §11), together with the retained
memory per MiB of input.

#### Scenario: Benchmark recorded
- **WHEN** the change is verified
- **THEN** `docs/devlog.md` records MB/s for JSON Lines, logfmt and plain text, and bytes of
  retained memory per MiB of input
