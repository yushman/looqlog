## Purpose

The `live-tail-ui` capability covers the browser-side surfaces of a live stdin stream:
the connection-state indicator, the lines/sec counter, the eviction marker, autoscroll with
pause/resume, reconnect, the detected-format display, and the privacy indicator (`live-tail`
change; reworked for the thin client by `backend-query-engine`).
## Requirements
### Requirement: Live state is visible
The UI SHALL show a `LIVE` indicator while a stream is connected and receiving, together with a
lines-per-second counter updated at a fixed throttled interval. The indicator SHALL distinguish
connecting, live, stream ended and disconnected states rather than showing a single on/off dot.

#### Scenario: Stream running
- **WHEN** lines are arriving
- **THEN** the indicator shows live and the counter shows a non-zero rate

#### Scenario: Producer finished
- **WHEN** stdin closes and the stream-ended message arrives
- **THEN** the indicator says the stream ended and the already-received entries remain viewable

#### Scenario: Backend gone
- **WHEN** the `looqlog` process is killed while the page is open
- **THEN** the indicator shows disconnected rather than remaining on live with a frozen counter

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

### Requirement: Autoscroll pauses when the user reads
The UI SHALL follow the tail automatically while the user is at the bottom of the list, SHALL
pause following when the user scrolls away, and SHALL offer an explicit control to resume that
reports how many entries arrived while paused.

#### Scenario: Scrolling up pauses the tail
- **WHEN** the user scrolls up during an active stream
- **THEN** the view stops jumping to new entries and a paused state with a pending count is shown

#### Scenario: Resuming jumps to the newest entry
- **WHEN** the user activates the resume control
- **THEN** the view scrolls to the newest entry and following resumes

#### Scenario: Rendering is throttled
- **WHEN** lines arrive faster than the display can usefully update
- **THEN** rendering is batched at a fixed interval and the page remains responsive

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

