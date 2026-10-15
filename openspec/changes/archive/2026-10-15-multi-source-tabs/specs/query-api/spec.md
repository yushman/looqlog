## MODIFIED Requirements

### Requirement: One authenticated socket per tab carries all queries
The page SHALL open one WebSocket to `/ws` and authenticate with the per-process token and the id
of its source (or no id, for the landing) as its first frame, and send every query over that
socket. Each request SHALL carry a client-chosen `id` and a `type`, and each reply SHALL carry the
same `id`. A request sent before authentication SHALL close the connection without a reply. All
requests on a source connection SHALL apply to that source only.

#### Scenario: Reply matches request
- **WHEN** an authenticated client sends two `window` requests with ids 1 and 2
- **THEN** it receives exactly one reply for each, tagged 1 and 2

#### Scenario: Unauthenticated query is refused
- **WHEN** a client connects and sends a `query` request without first sending the token
- **THEN** the connection is closed and no entry data is sent

#### Scenario: Two tabs, two sources
- **WHEN** tab A is bound to `a.log` and tab B to `b.log`
- **THEN** each tab's windows contain only its own source's entries

### Requirement: Format override is a request
A `set_format(format | auto, tz_offset_minutes?)` request SHALL re-parse the retained raw lines of
the connection's source with the given format, or with detection when `auto`, and with the given
fixed UTC offset for offset-less timestamps when present, and reply with the new detection result.
The override SHALL apply to lines ingested afterwards as well, and SHALL NOT affect other sources.

#### Scenario: Override plain text to logfmt
- **WHEN** a plain-text-detected source is overridden to logfmt
- **THEN** the field inventory gains the logfmt keys and the detection result names the override

#### Scenario: Override applies to every tab of the source
- **WHEN** two tabs are open on the same source and one sends `set_format(logfmt)`
- **THEN** both receive `reset` and both see the logfmt parse

#### Scenario: Reset keeps the tab's filters
- **WHEN** a tab with filter `level=ERROR` receives `reset` because another tab overrode the format
- **THEN** it re-queries with `level=ERROR` still active

#### Scenario: Override does not leak to another source
- **WHEN** a tab on `a.log` sends `set_format(logfmt)` while a tab on `b.log` is open
- **THEN** the `b.log` tab receives no `reset` and its parse is unchanged

#### Scenario: Timezone override
- **WHEN** a client sends `set_format(auto, 120)` for a file with offset-less timestamps
- **THEN** the re-parsed entries' timestamps are interpreted at UTC+02:00

## ADDED Requirements

### Requirement: A gone source is reported, not empty
The backend SHALL send `closed` when a connection authenticates with an unknown id, or when its
source is closed or unloaded while connected, with a reason (`unknown`, `closed`, `unloaded`,
`budget`) and keep the connection open without answering data requests with entries. The page
SHALL show that the source is gone and why, with an upload option, never an empty table.

#### Scenario: Stale link after restart
- **WHEN** a tab opens `/l/<id>` from a previous run
- **THEN** it receives `closed` with reason `unknown` and shows the closed state

#### Scenario: Closed while viewing
- **WHEN** a source is closed from the landing while a tab shows it
- **THEN** that tab switches to the closed state with reason `closed`

### Requirement: The landing connection receives the source list
A connection authenticated without a source id SHALL receive the current source list right after
authentication and an updated list whenever a source is added, changes state, changes its tab
count, or is removed. A `close(id)` request on it SHALL close that source, or fail for stdin.

#### Scenario: List after auth
- **WHEN** a landing connection authenticates in a process with two sources
- **THEN** its first message lists both with name, kind, size, state and tab count
