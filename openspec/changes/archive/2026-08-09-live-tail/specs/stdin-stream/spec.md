## REMOVED Requirements

### Requirement: Buffering is deferred and must not be faked
**Reason**: This change implements the ADR-0004 buffering it was holding the place for.
**Migration**: Replaced by the ring buffer, snapshot-on-connect and backpressure requirements
below; `--max-lines` stops being an inert parsed value and starts sizing the buffer.

## ADDED Requirements

### Requirement: Bounded ring buffer independent of clients
The backend SHALL retain stdin lines in a ring buffer sized by `--max-lines`, filled from the
moment the process starts and regardless of whether any client is connected. When the buffer is
full, the oldest lines SHALL be dropped (ADR-0004).

#### Scenario: Lines emitted before the browser opens survive
- **WHEN** a producer writes 500 lines to `looq --stdin` and a browser connects afterwards
- **THEN** the browser receives those 500 lines

#### Scenario: Memory stays bounded over a long run
- **WHEN** a producer writes ten times `--max-lines` worth of lines
- **THEN** the buffer holds at most `--max-lines` lines and process memory does not grow with
  the total number of lines written

### Requirement: Snapshot on connect
The backend SHALL send the current buffer contents to every newly connected client as a
snapshot before any subsequent live line, including on page reload and reconnect. The snapshot
SHALL carry the sequence number of its last line so the client can position itself in the
stream.

#### Scenario: Reload keeps history
- **WHEN** the user reloads the page during an active stream
- **THEN** the reloaded page shows the buffered history and continues live

#### Scenario: No line is delivered twice around the snapshot
- **WHEN** lines arrive while a snapshot is being sent
- **THEN** the client ends up with each line exactly once, in order

### Requirement: Backpressure drops oldest and says so
Each client SHALL have a bounded outbound channel; when it is full, the oldest undelivered
messages SHALL be dropped rather than blocking the stdin reader. Every drop SHALL produce a gap
event to that client stating how many lines were lost.

#### Scenario: Slow client does not stall the producer
- **WHEN** a connected client stops reading while a fast producer keeps writing
- **THEN** the stdin reader continues at full speed and the producer is never blocked

#### Scenario: Drops are reported, never silent
- **WHEN** messages are dropped for a client
- **THEN** that client receives a gap event whose count equals the number of lines it did not
  receive

### Requirement: Sequence numbers on every line
The backend SHALL assign a monotonically increasing sequence number to each stdin line and
include it in every delivered message, so a client can detect a gap by discontinuity even if a
gap event is itself lost.

#### Scenario: Client detects a discontinuity
- **WHEN** a client receives sequence numbers 100 then 137
- **THEN** the client can conclude that 36 lines are missing without relying on a gap event

### Requirement: Structured message envelope
The backend SHALL send WebSocket text messages in a structured envelope distinguishing at least
line, snapshot, gap and stream-ended messages, each carrying the fields that message type needs.
Raw unlabelled line text SHALL NOT be sent.

#### Scenario: Message types are distinguishable
- **WHEN** a client connects, receives history, experiences a drop, and then stdin closes
- **THEN** it receives a snapshot message, line messages, a gap message with a count, and a
  stream-ended message, each identifiable by its type

#### Scenario: A line containing envelope-like text is not misread
- **WHEN** a log line's own text looks like a serialised envelope
- **THEN** it is delivered as an ordinary line message and its content is not interpreted
