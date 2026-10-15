## Purpose

The `error-states` capability covers how the application behaves and what it says when input or
environment is bad — unreadable file, empty file, binary file, a format that could not be parsed
at all, a rejected upload, a lost connection to the backend — including the file-size cap derived from measurement.
## Requirements
### Requirement: Every failure has a specific message
The application SHALL present a specific, user-facing message for each failure it can encounter —
unreadable file, empty file, binary file, a format that produced no entries, an upload that failed
or was rejected, and a lost connection to the backend — rather than a blank view, a spinner that
never ends, or a console-only error.

#### Scenario: Empty file
- **WHEN** the user opens a zero-byte file
- **THEN** the UI says the file is empty, distinct from the initial prompt and from a filtered
  empty result

#### Scenario: Binary file
- **WHEN** the user opens a file whose first chunk contains NUL bytes or is otherwise
  overwhelmingly non-textual
- **THEN** the UI says it does not look like a log file and offers to proceed anyway rather than
  rendering unreadable rows

#### Scenario: Unreadable file
- **WHEN** the backend cannot read the argv path, or the browser cannot read a picked file
- **THEN** the UI reports what failed, naming the path when there is one, and lets the user
  upload another file

#### Scenario: Upload rejected
- **WHEN** an upload fails or is rejected by the backend
- **THEN** the UI shows the backend's reason rather than a generic network error

#### Scenario: Backend gone
- **WHEN** the `looqlog` process exits while the page is open
- **THEN** the UI says the connection to `looqlog` was lost instead of remaining in a loading
  state

### Requirement: File size cap and warning
The application SHALL warn about a file above a documented warning threshold — before ingesting
an upload, and while reading an argv file, whose path on the command line is the confirmation
(`file-ingest`) — and SHALL
refuse files above a documented hard cap with a message explaining that the `looqlog` process
cannot hold the index for a file that size. Both numbers SHALL be derived from the measured
retained memory per MiB of input (`file-ingest`) and recorded. The check SHALL apply to argv files
(using the file's size) and uploads (using the declared size, and enforced again while streaming).

#### Scenario: Warning threshold
- **WHEN** the user uploads a file above the warning threshold and below the cap
- **THEN** the UI warns about memory and time before parsing and lets the user continue or cancel

#### Scenario: Hard cap
- **WHEN** the user opens a file above the hard cap
- **THEN** parsing does not begin, the limit and the reason are stated, and the suggested
  alternatives are given

#### Scenario: Numbers are justified
- **WHEN** the cap and threshold are documented
- **THEN** each cites the measurement it came from rather than being asserted

### Requirement: Failure does not destroy existing state
An error while opening a new file SHALL leave every existing source, and every tab's data and
filters, intact.

#### Scenario: Bad second file
- **WHEN** a source is open and the user uploads a second file that fails
- **THEN** the error is shown on the uploading tab and the first source's tabs keep their entries
  and filters

#### Scenario: Budget refusal
- **WHEN** an upload is refused by the memory budget
- **THEN** the uploader sees an error naming the budget, and no other source changes

### Requirement: Errors are reachable, not transient
Error messages SHALL remain visible until dismissed or superseded, and SHALL NOT rely on a
disappearing toast as their only presentation.

#### Scenario: User looks away
- **WHEN** an error occurs and the user returns to the tab a minute later
- **THEN** the error is still visible

