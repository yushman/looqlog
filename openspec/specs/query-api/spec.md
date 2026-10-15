# query-api Specification

## Purpose
The `query-api` capability covers the browser↔backend protocol over `/ws`: per-connection views,
window/entry/bucket/stats requests, chain expansion, format override and coalesced change
notifications (ADR-0010).
## Requirements
### Requirement: One authenticated socket per tab carries all queries
The page SHALL open one WebSocket to `/ws`, authenticate with the per-process token as its first
frame, and send every query over that socket. Each request SHALL carry a client-chosen `id` and a
`type`, and each reply SHALL carry the same `id`. A request sent before authentication SHALL close
the connection without a reply.

#### Scenario: Reply matches request
- **WHEN** an authenticated client sends two `window` requests with ids 1 and 2
- **THEN** it receives exactly one reply for each, tagged 1 and 2

#### Scenario: Unauthenticated query is refused
- **WHEN** a client connects and sends a `query` request without first sending the token
- **THEN** the connection is closed and no entry data is sent

### Requirement: The query is the whole view state
A `query` request SHALL carry the active field filters, the raw search text and the time range.
The backend SHALL compile the search text — including `field=value` token extraction against the
field inventory — evaluate the chain-aware predicate over the retained entries, and reply with the
total entry count, the matched count, the count of matched entries excluded for lacking a timestamp
when a range is active, the extracted filter tokens, and a compile error if any. Each `query`
SHALL start a new generation; replies to `window` and `buckets` SHALL carry the generation they
were computed for.

#### Scenario: Filter and search combine
- **WHEN** a client queries with filter `level=ERROR` and search `timeout` over a fixture with
  known counts
- **THEN** the reply's matched count equals the number of entries satisfying both

#### Scenario: Compile error keeps the previous result
- **WHEN** a client queries with search `re:[unclosed`
- **THEN** the reply carries a compile error and the backend keeps the previous generation's match
  set as current

#### Scenario: Stale reply is recognisable
- **WHEN** a client sends a `window` request and then a new `query` before the window reply
  arrives
- **THEN** the window reply carries the older generation, so the client can discard it

### Requirement: Windows of the current result set
A `window(offset, limit)` request SHALL return up to `limit` rows of the current match set in
input order starting at `offset`. Each row SHALL carry its ordinal, timestamp, level, message,
fields, continuation link (number or null), chain membership needed to render a collapsible group,
and search match ranges in the message for highlighting. `limit` SHALL be capped by the backend.
Offsets SHALL be counted over the collapsed view (see "Chain expansion is view state"), and the
reply SHALL carry the current row count of that view.

#### Scenario: Window past the end
- **WHEN** a client requests a window whose offset is beyond the matched count
- **THEN** the reply is an empty row list with the current matched count, not an error

#### Scenario: Chain rows arrive with their structure
- **WHEN** a window covers a stack-trace chain
- **THEN** the root row has a null continuation link and each member row names the root's ordinal

### Requirement: Query state is per connection
Each authenticated connection SHALL have its own query, match set, generation and chain toggles.
A `query` on one connection SHALL NOT change what any other connection's `window`, `buckets` or
`matched` counts return.

#### Scenario: Two tabs, two filters
- **WHEN** tab A queries `level=ERROR` and tab B then queries with no filter
- **THEN** tab A's next window still contains only ERROR entries

### Requirement: Chain expansion is view state
A connection SHALL be able to toggle a chain by its root ordinal. A chain SHALL be shown expanded
when exactly one of these holds: the search matched one of its members, or its root is toggled.
Toggling SHALL change the row count and windows without re-evaluating the predicate.

#### Scenario: Collapsed chain counts as one row
- **WHEN** a match set contains one chain root with five members and no search matches a member
- **THEN** the view's row count counts the chain as one row and a window returns the root only

#### Scenario: Search expands, toggle collapses
- **WHEN** a search matches a member of a chain and the user then toggles that chain
- **THEN** the chain is shown collapsed

### Requirement: Source description is a request
A `stats` request SHALL return what the page needs about the source that no notification carries:
the source's name and any read error or size warning, the active format and detection result,
the diagnostics summary, the field inventory, retained entries per level, retained entry count,
total and blank line counts, the cumulative evicted line count, bytes ingested, and whether
ingest has finished.

#### Scenario: Page learns the source on load
- **WHEN** a client authenticates against `looqlog app.log` and sends `stats`
- **THEN** the reply names `app.log`, its detected format and its field inventory

#### Scenario: Read error is visible through stats
- **WHEN** a client sends `stats` to a process started with an unreadable path
- **THEN** the reply carries the read error naming the path

### Requirement: A window can be anchored to an entry
A `window` request MAY carry an `anchor` ordinal instead of an `offset`; the backend SHALL then
start the window at that entry's current position in the collapsed view, so a paused live view
keeps showing the same rows while older entries are evicted.

#### Scenario: Paused view survives eviction
- **WHEN** a paused client requests a window anchored at ordinal 5000 after 1000 older entries
  were evicted
- **THEN** the window starts at ordinal 5000

### Requirement: Single entry fetch
An `entry(ordinal)` request SHALL return the full entry, including its raw line(s), for the detail
pane, or a not-found reply if the ordinal has been evicted.

#### Scenario: Evicted entry
- **WHEN** a client requests an ordinal that the ring buffer has evicted
- **THEN** the reply says the entry is no longer retained

### Requirement: Timeline buckets come from the backend
A `buckets` request SHALL return the bucket width, the bucket start times and per-bucket counts
for the current match set over the robust span (or the requested span), plus the timestampless
count, computed by the backend.

#### Scenario: Buckets reflect filters
- **WHEN** a filter excludes every ERROR entry and buckets are requested
- **THEN** no bucket counts any ERROR entry

### Requirement: Change notifications are pushed and coalesced
The backend SHALL push untagged notifications: `progress` (bytes ingested, total and whether ingest
is done, file only),
`grown` (new total and matched counts), `evicted` (number of lines dropped from retention),
`ended` (stdin closed), and `reset` (the source was replaced or re-parsed; the client must
re-query). `progress`, `grown` and `evicted` SHALL be coalesced to at most one of each per 100 ms
per client, carrying cumulative values so that a coalesced or lost notification never makes a
count wrong.

#### Scenario: Fast producer, bounded messages
- **WHEN** stdin delivers 10,000 lines in one second
- **THEN** the client receives no more than about ten `grown` notifications in that second, and
  the last one carries the exact totals

#### Scenario: Reset after a format override
- **WHEN** a client sends `set_format` with a different format
- **THEN** every connected client receives `reset` and a subsequent `query` reflects the new parse

### Requirement: Format override is a request
A `set_format(format | auto, tz_offset_minutes?)` request SHALL re-parse the retained raw lines with
the given format, or with detection when `auto`, and with the given fixed UTC offset for
offset-less timestamps when present, and reply with the new detection result. The override SHALL apply to
lines ingested afterwards as well.

#### Scenario: Override plain text to logfmt
- **WHEN** a plain-text-detected source is overridden to logfmt
- **THEN** the field inventory gains the logfmt keys and the detection result names the override

#### Scenario: Override applies to every tab of the process
- **WHEN** two tabs are open on the same process and one sends `set_format(logfmt)`
- **THEN** both receive `reset` and both see the logfmt parse, because the format belongs to the
  source rather than to a tab's view

#### Scenario: Timezone override
- **WHEN** a client sends `set_format(auto, 120)` for a file with offset-less timestamps
- **THEN** the re-parsed entries' timestamps are interpreted at UTC+02:00

