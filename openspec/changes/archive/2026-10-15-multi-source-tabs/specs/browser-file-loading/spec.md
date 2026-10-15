## MODIFIED Requirements

### Requirement: The user selects the file in the browser
The landing page and the closed-source state SHALL offer a file picker and a drag-and-drop target.
A file chosen this way SHALL be uploaded to the local backend and parsed there as a new source
(`file-ingest`), and the tab SHALL navigate to that source's URL. A tab already showing a source
SHALL NOT replace it in place. The page SHALL NOT read or parse the file's contents itself.

#### Scenario: Picking a file parses it
- **WHEN** the user selects a local `.jsonl` file through the landing's file picker
- **THEN** the file is uploaded, the tab navigates to `/l/<id>`, and its parsed entries are shown

#### Scenario: Drag and drop is equivalent to picking
- **WHEN** the user drops a log file onto the landing
- **THEN** it is handled identically to a picked file

#### Scenario: Viewing tab stays bound
- **WHEN** a tab shows `a.log` and the user wants another file
- **THEN** opening another file goes through the landing or a new tab, and `a.log`'s tab keeps
  showing `a.log`
