## Purpose

The `stdin-stream` capability covers reading stdin off the main request path and
transporting lines to connected browsers over `/ws`, with a bounded ring buffer,
snapshot-on-connect, backpressure and a structured message envelope (ADR-0004).
## Requirements
### Requirement: Stdin is read off the request path
In stdin mode the CLI SHALL read stdin line by line in a task independent of the HTTP
server, starting before any client connects. Reading SHALL NOT block on the presence,
absence or slowness of a WebSocket client.

#### Scenario: Producer is not blocked by a missing client
- **WHEN** a producer writes lines to `looqlog --stdin` with no browser connected
- **THEN** the producer is never blocked and the process keeps consuming stdin

#### Scenario: Producer is not blocked by a slow client
- **WHEN** a connected client stops reading from its WebSocket while the producer keeps
  writing
- **THEN** the stdin reader continues to make progress

### Requirement: Lines are delivered to connected clients over `/ws`
The server SHALL expose a WebSocket endpoint at `/ws`. Each stdin line read while a client
is connected SHALL be delivered to that client as a `line` message in the structured
envelope (see "Structured message envelope"), preserving line order and excluding the
trailing newline from the carried text.

#### Scenario: Line reaches a connected client
- **WHEN** `echo hi | looqlog --stdin` is running and a `wscat` client connects to
  `ws://127.0.0.1:7891/ws`
- **THEN** the client receives a `line` message whose `text` field is `hi`

#### Scenario: Order is preserved
- **WHEN** three lines are written in sequence to stdin with a client connected
- **THEN** the client receives all three in the same order

#### Scenario: Multiple clients each get the line
- **WHEN** two clients are connected and one line arrives on stdin
- **THEN** both clients receive it

### Requirement: End of stdin is signalled, not silent
When stdin reaches EOF, the server SHALL keep serving so already-received lines remain
viewable, and SHALL inform connected clients that the stream has ended rather than simply
going quiet. A client that connects *after* stdin has already closed SHALL still be told
the stream has ended, not left waiting indefinitely for a signal that was already sent to
nobody.

#### Scenario: EOF does not kill the server
- **WHEN** `printf 'a\nb\n' | looqlog --stdin` finishes writing and stdin closes
- **THEN** the HTTP server is still reachable and the client has been told the stream ended

#### Scenario: Late connection after stdin already closed
- **WHEN** stdin closes before any browser has ever connected, and a browser connects
  afterward
- **THEN** that client receives the buffered history and is still told the stream has
  ended, rather than waiting forever

### Requirement: Bounded ring buffer independent of clients
The backend SHALL retain stdin lines in a ring buffer sized by `--max-lines`, filled from
the moment the process starts and regardless of whether any client is connected. When the
buffer is full, the oldest lines SHALL be dropped (ADR-0004).

#### Scenario: Lines emitted before the browser opens survive
- **WHEN** a producer writes 500 lines to `looqlog --stdin` and a browser connects afterwards
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

