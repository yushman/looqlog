## Purpose

The `error-states` capability covers how the application behaves and what it says when input or
environment is bad — unreadable file, empty file, binary file, a format that could not be parsed
at all, a WASM module that failed to load — including the file-size cap derived from measurement.

## Requirements

### Requirement: Every failure has a specific message
The application SHALL present a specific, user-facing message for each failure it can encounter —
unreadable file, empty file, binary file, a format that produced no entries, and a WASM module
that failed to load — rather than a blank view, a spinner that never ends, or a console-only
error.

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
- **WHEN** the browser cannot read the selected file, for example because permission was denied
- **THEN** the UI reports what failed and lets the user pick another file

#### Scenario: Module failure
- **WHEN** the WASM module fails to load or instantiate
- **THEN** the UI says so explicitly instead of remaining in a loading state

### Requirement: File size cap and warning
The application SHALL warn before parsing a file above a documented warning threshold, and SHALL
refuse files above a documented hard cap with a message explaining that browser memory cannot
hold the index for a file that size (TDR §14). Both numbers SHALL be derived from measurement and
recorded.

#### Scenario: Warning threshold
- **WHEN** the user opens a file above the warning threshold and below the cap
- **THEN** the UI warns about memory and time before parsing and lets the user continue or cancel

#### Scenario: Hard cap
- **WHEN** the user opens a file above the hard cap
- **THEN** parsing does not begin, the limit and the reason are stated, and the suggested
  alternatives are given

#### Scenario: Numbers are justified
- **WHEN** the cap and threshold are documented
- **THEN** each cites the measurement it came from rather than being asserted

### Requirement: Failure does not destroy existing state
An error while opening a new file SHALL leave any previously loaded data and filters intact,
rather than clearing the view.

#### Scenario: Bad second file
- **WHEN** a file is open and the user opens a second file that fails
- **THEN** the error is shown and the first file's entries and filters remain

### Requirement: Errors are reachable, not transient
Error messages SHALL remain visible until dismissed or superseded, and SHALL NOT rely on a
disappearing toast as their only presentation.

#### Scenario: User looks away
- **WHEN** an error occurs and the user returns to the tab a minute later
- **THEN** the error is still visible
