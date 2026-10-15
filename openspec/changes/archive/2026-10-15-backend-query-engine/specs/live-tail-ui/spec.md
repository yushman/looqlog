## MODIFIED Requirements

### Requirement: Gaps are marked in the entry list
The UI SHALL render a visible marker at the top of the entry list when the backend has evicted
entries, stating how many were evicted. Eviction SHALL NOT be represented only as a change in the
counter or only in a log message.

#### Scenario: Eviction produces a visible marker
- **WHEN** a stream exceeds `--max-lines` while the page is open
- **THEN** a marker above the first retained row names the number of evicted lines

#### Scenario: Marker on a late connection
- **WHEN** a browser connects after eviction has already happened
- **THEN** the marker is shown with the cumulative evicted count

### Requirement: Client-side retention mirrors the backend
The UI SHALL NOT retain entries of its own beyond the rows it renders; retention is the backend's
(`stdin-stream`). The entry count SHALL show the retained total, and eviction SHALL be visible as
such rather than silently changing what the entry count means.

#### Scenario: Long-running stream stays bounded
- **WHEN** a stream delivers several times `--max-lines` entries to an open page
- **THEN** browser memory stays flat and the retained total stops at `--max-lines`

#### Scenario: Evicted history is not presented as complete
- **WHEN** eviction has occurred
- **THEN** the UI indicates that earlier entries are no longer retained

### Requirement: Reconnect does not duplicate history
On losing the connection the UI SHALL retry with backoff, and on reconnecting SHALL re-issue its
current query and window so the view shows each retained entry exactly once.

#### Scenario: Brief disconnect
- **WHEN** the connection drops and is re-established while the producer keeps writing
- **THEN** the entry list contains each retained line exactly once and the eviction marker
  reflects anything evicted meanwhile

### Requirement: Stream format detection holds the opening lines
The backend SHALL hold the opening lines of a stream until either a sample sufficient for detection
has arrived or a short timeout expires, then run detection once and parse the held lines with the
detected format. The UI SHALL make the detected format visible and overridable in the same way as
file mode.

#### Scenario: Detection from the first lines
- **WHEN** a stream begins with JSON Lines output
- **THEN** the format is detected as JSON Lines and the held lines are parsed as such

#### Scenario: Slow producer still shows output
- **WHEN** a producer emits only two lines and then goes quiet
- **THEN** the hold times out, those lines are parsed and become queryable rather than waiting
  for a full sample

### Requirement: Stdin mode states its weaker privacy guarantee
The UI SHALL indicate, in every mode, that log data travels from the `looqlog` process to the
browser over a local connection and never leaves the machine on the default bind. It SHALL NOT
claim, in any mode, that data stays inside the browser.

#### Scenario: Mode indicator in stdin mode
- **WHEN** the page is showing a live stdin stream
- **THEN** the mode indicator says the data is read by the local `looqlog` process and never
  leaves the machine

#### Scenario: Mode indicator in file mode
- **WHEN** the page is showing a file
- **THEN** the mode indicator says the file is read by the local `looqlog` process and never
  leaves the machine
