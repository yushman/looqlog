## MODIFIED Requirements

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
