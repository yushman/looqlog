## Purpose

The `browser-file-loading` capability covers the page's file picker and drop target, which
upload a chosen file to the local backend for parsing (ADR-0010; see `file-ingest`).
## Requirements
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

