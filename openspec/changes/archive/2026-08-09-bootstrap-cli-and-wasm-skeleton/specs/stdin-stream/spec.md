## ADDED Requirements

### Requirement: Stdin is read off the request path
In stdin mode the CLI SHALL read stdin line by line in a task independent of the HTTP
server, starting before any client connects. Reading SHALL NOT block on the presence,
absence or slowness of a WebSocket client.

#### Scenario: Producer is not blocked by a missing client
- **WHEN** a producer writes lines to `looq --stdin` with no browser connected
- **THEN** the producer is never blocked and the process keeps consuming stdin

#### Scenario: Producer is not blocked by a slow client
- **WHEN** a connected client stops reading from its WebSocket while the producer keeps
  writing
- **THEN** the stdin reader continues to make progress

### Requirement: Lines are delivered to connected clients over `/ws`
The server SHALL expose a WebSocket endpoint at `/ws`. Each stdin line read while a client
is connected SHALL be delivered to that client as one text message, preserving line order
and excluding the trailing newline.

#### Scenario: Line reaches a connected client
- **WHEN** `echo hi | looq --stdin` is running and a `wscat` client connects to
  `ws://127.0.0.1:7891/ws`
- **THEN** the client receives a message whose payload is `hi`

#### Scenario: Order is preserved
- **WHEN** three lines are written in sequence to stdin with a client connected
- **THEN** the client receives all three in the same order

#### Scenario: Multiple clients each get the line
- **WHEN** two clients are connected and one line arrives on stdin
- **THEN** both clients receive it

### Requirement: End of stdin is signalled, not silent
When stdin reaches EOF, the server SHALL keep serving so already-received lines remain
viewable, and SHALL inform connected clients that the stream has ended rather than simply
going quiet.

#### Scenario: EOF does not kill the server
- **WHEN** `printf 'a\nb\n' | looq --stdin` finishes writing and stdin closes
- **THEN** the HTTP server is still reachable and the client has been told the stream ended

### Requirement: Buffering is deferred and must not be faked
The server SHALL NOT buffer stdin lines for later delivery in this change, and `--max-lines`
SHALL be parsed and reported but SHALL NOT govern any buffer yet. The bounded ring buffer,
snapshot-on-connect and drop-with-gap-indicator behaviour of ADR-0004 SHALL be specified and
implemented by the `live-tail` change, so lines emitted before the first client connects are
lost here by design.

#### Scenario: Late connection sees only new lines
- **WHEN** a client connects two seconds after lines have already been written to stdin
- **THEN** the client receives only lines arriving after connection, and this is the
  documented, temporary behaviour of this change rather than a defect
