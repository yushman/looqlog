## MODIFIED Requirements

### Requirement: Stdin is read off the request path
In stdin mode the CLI SHALL read stdin line by line in a task independent of the HTTP
server, starting before any client connects, and SHALL parse each line on the backend as it is
read. Reading SHALL NOT block on the presence, absence or slowness of a WebSocket client.

#### Scenario: Producer is not blocked by a missing client
- **WHEN** a producer writes lines to `looqlog --stdin` with no browser connected
- **THEN** the producer is never blocked and the process keeps consuming stdin

#### Scenario: Producer is not blocked by a slow client
- **WHEN** a connected client stops reading from its WebSocket while the producer keeps
  writing
- **THEN** the stdin reader continues to make progress

### Requirement: Lines are delivered to connected clients over `/ws`
The server SHALL expose a WebSocket endpoint at `/ws`. Each stdin line SHALL become a parsed entry
on the backend, and connected clients SHALL be notified with `grown` (`query-api`) carrying the new
total and matched counts. Clients obtain the entries themselves through `window` requests, in input
order, with the trailing newline excluded from the message text.

#### Scenario: Line reaches a connected client
- **WHEN** `echo hi | looqlog --stdin` is running and an authenticated client is connected
- **THEN** the client receives a `grown` notification and a following `window(0, 1)` returns an
  entry whose message is `hi`

#### Scenario: Order is preserved
- **WHEN** three lines are written in sequence to stdin with a client connected
- **THEN** a window over them returns all three in the same order

#### Scenario: Multiple clients each see the line
- **WHEN** two clients are connected and one line arrives on stdin
- **THEN** both clients receive `grown` and can fetch it

### Requirement: End of stdin is signalled, not silent
When stdin reaches EOF, the server SHALL keep serving so already-received entries remain
queryable, and SHALL inform connected clients with `ended` rather than simply going quiet. A
client that connects *after* stdin has already closed SHALL receive `ended` right after
authentication.

#### Scenario: EOF does not kill the server
- **WHEN** `printf 'a\nb\n' | looqlog --stdin` finishes writing and stdin closes
- **THEN** the HTTP server is still reachable and the client has been told the stream ended

#### Scenario: Late connection after stdin already closed
- **WHEN** stdin closes before any browser has ever connected, and a browser connects
  afterward
- **THEN** that client can query the retained entries and is told the stream has ended

### Requirement: Bounded ring buffer independent of clients
The backend SHALL retain stdin entries (with their raw lines) up to `--max-lines`, filled from the
moment the process starts and regardless of whether any client is connected. When the limit is
reached, the oldest entries SHALL be evicted from the index (ADR-0004), and connected clients
SHALL be notified with `evicted` carrying the cumulative evicted count.

#### Scenario: Lines emitted before the browser opens survive
- **WHEN** a producer writes 500 lines to `looqlog --stdin` and a browser connects afterwards
- **THEN** the browser can query those 500 entries

#### Scenario: Memory stays bounded over a long run
- **WHEN** a producer writes ten times `--max-lines` worth of lines
- **THEN** at most `--max-lines` entries are retained and process memory does not grow with
  the total number of lines written

#### Scenario: Eviction is announced
- **WHEN** retention evicts entries while a client is connected
- **THEN** the client receives `evicted` with the cumulative count

### Requirement: Snapshot on connect
A newly connected or reconnected client SHALL be able to query every retained entry immediately
after authentication; the first notification after authentication SHALL carry the current total,
matched and cumulative evicted counts so the client positions itself without a separate snapshot
transfer.

#### Scenario: Reload keeps history
- **WHEN** the user reloads the page during an active stream
- **THEN** the reloaded page shows the retained history and continues live

#### Scenario: No entry appears twice
- **WHEN** lines arrive while a client is reconnecting
- **THEN** the client's windows contain each entry exactly once, in order

### Requirement: Sequence numbers on every line
The backend SHALL assign a monotonically increasing ordinal to each stdin line and include it in
every row it returns, so a client can tell which entries are retained and where eviction cut the
history.

#### Scenario: First retained ordinal reveals eviction
- **WHEN** 150 lines have been written with `--max-lines 100`
- **THEN** the first row returned has ordinal 51 (line numbers start at 1) and the cumulative
  evicted count is 50

### Requirement: Structured message envelope
The backend SHALL send WebSocket text messages in a structured JSON envelope distinguishing at
least replies (tagged with the request id), `progress`, `grown`, `evicted`, `ended` and `reset`
notifications, and errors, each carrying the fields that message type needs. Log text SHALL only
appear inside a row's fields and SHALL never be interpreted.

#### Scenario: Message types are distinguishable
- **WHEN** a client connects, queries, receives growth, sees eviction, and then stdin closes
- **THEN** it receives a reply, `grown`, `evicted` and `ended` messages, each identifiable by its
  type

#### Scenario: A line containing envelope-like text is not misread
- **WHEN** a log line's own text looks like a serialised envelope
- **THEN** it is returned as an ordinary row's message and its content is not interpreted

## REMOVED Requirements

### Requirement: Backpressure drops oldest and says so
**Reason**: Clients pull rows; the only pushed messages are cumulative, coalesced notifications,
so a slow client loses nothing it cannot re-query. Per-client gap events have no meaning.
**Migration**: The only loss left is retention eviction, reported by `evicted` ("Bounded ring
buffer independent of clients") and shown by `live-tail-ui`.
