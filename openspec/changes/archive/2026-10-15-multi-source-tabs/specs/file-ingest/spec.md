## MODIFIED Requirements

### Requirement: The argv path is read by the backend
Each positional path SHALL become its own source, read and parsed by the backend at startup, once,
without following later appends. Its tab SHALL show the log without requiring the user to pick it.
A path that cannot be read SHALL NOT prevent the server from starting; the failure SHALL be printed
on the terminal and shown as that source's error state.

#### Scenario: Zero-click open
- **WHEN** the user runs `looqlog tests/fixtures/sample.jsonl` and opens the printed source URL
- **THEN** the page shows the fixture's entries with no file picker step

#### Scenario: Unreadable path
- **WHEN** the user runs `looqlog does-not-exist.log`
- **THEN** the server starts, the terminal names the path and the error, and that source's tab
  shows the same error with an option to upload a file instead

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
SHALL create a new source and reply `201` with its id; it SHALL NOT replace or alter any existing
source. Uploads SHALL be accepted whether or not the process has a stdin source.

#### Scenario: Picked file becomes a source
- **WHEN** the user picks a `.jsonl` fixture on the landing
- **THEN** it is uploaded, a new source appears, and the tab navigates to that source's URL

#### Scenario: Drop is equivalent to pick
- **WHEN** the user drops a log file onto the page
- **THEN** it is handled identically to a picked file

#### Scenario: Upload next to stdin
- **WHEN** the process is reading stdin and the user uploads a file
- **THEN** the upload becomes a separate source and the stdin source is unaffected

### Requirement: A new ingest cancels the previous one
Ingests of different sources SHALL run independently. Aborting an upload — by the user leaving
mid-upload, a failed read, or the memory budget — SHALL drop only that upload's partial source.

#### Scenario: Two uploads at once
- **WHEN** two tabs upload different files at the same time
- **THEN** two sources are created and each holds only its own file's entries

#### Scenario: Aborted upload leaves nothing behind
- **WHEN** an upload is aborted midway
- **THEN** no source for it remains in the registry
