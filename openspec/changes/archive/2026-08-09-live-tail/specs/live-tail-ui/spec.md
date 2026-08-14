## ADDED Requirements

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
- **WHEN** the `looq` process is killed while the page is open
- **THEN** the indicator shows disconnected rather than remaining on live with a frozen counter

### Requirement: Gaps are marked in the entry list
The UI SHALL render a visible marker at the position where lines were dropped, stating how many
were lost. A gap SHALL NOT be represented only as a change in the counter or only in a log
message.

#### Scenario: Backpressure produces a visible marker
- **WHEN** a synthetic fast-producer/slow-consumer test causes drops
- **THEN** a gap marker appears in the entry list naming the number of lost lines

#### Scenario: Gap inferred from sequence numbers
- **WHEN** sequence numbers jump without a gap event arriving
- **THEN** the UI still shows a gap marker for the missing range

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
The UI SHALL retain at most `--max-lines` entries and SHALL evict the oldest beyond that, so a
page left open on a busy stream does not grow without bound. Eviction SHALL be visible as such
rather than silently changing what the entry count means.

#### Scenario: Long-running stream stays bounded
- **WHEN** a stream delivers several times `--max-lines` entries to an open page
- **THEN** browser memory stabilises and the oldest entries are evicted

#### Scenario: Evicted history is not presented as complete
- **WHEN** eviction has occurred
- **THEN** the UI indicates that earlier entries are no longer retained

### Requirement: Reconnect does not duplicate history
On losing the connection the UI SHALL retry with backoff, and on reconnecting SHALL use the
snapshot's sequence numbers to merge history without duplicating entries it already holds.

#### Scenario: Brief disconnect
- **WHEN** the connection drops and is re-established while the producer keeps writing
- **THEN** the entry list contains each line exactly once and a gap marker covers what was
  missed

### Requirement: Stream format detection holds the opening lines
The UI SHALL hold the opening lines of a stream until either a sample sufficient for detection
has arrived or a short timeout expires, then run detection once and parse the held lines with
the detected format. It SHALL make the detected format visible and overridable in the same way
as file mode.

#### Scenario: Detection from the first lines
- **WHEN** a stream begins with JSON Lines output
- **THEN** the format is detected as JSON Lines and the held lines are parsed as such

#### Scenario: Slow producer still shows output
- **WHEN** a producer emits only two lines and then goes quiet
- **THEN** the hold times out, those lines are parsed and displayed rather than waiting for a
  full sample

### Requirement: Stdin mode states its weaker privacy guarantee
The UI SHALL indicate, while in stdin mode, that log lines travel over a local WebSocket between
the `looq` process and the browser, which is a different guarantee from file mode's "never
leaves the browser" (TDR §12). The two modes SHALL NOT be described with the same wording.

#### Scenario: Mode indicator in stdin mode
- **WHEN** the page is showing a live stdin stream
- **THEN** the mode indicator says the data crossed a local process boundary

#### Scenario: Mode indicator in file mode
- **WHEN** the page is showing a file opened through the picker
- **THEN** the mode indicator says the file never left the browser
