## MODIFIED Requirements

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

### Requirement: Stdin mode states its weaker privacy guarantee
The UI SHALL indicate, while in stdin mode, that log lines travel over a local WebSocket between
the `looqlog` process and the browser, which is a different guarantee from file mode's "never
leaves the browser" (TDR §12). The two modes SHALL NOT be described with the same wording.

#### Scenario: Mode indicator in stdin mode
- **WHEN** the page is showing a live stdin stream
- **THEN** the mode indicator says the data crossed a local process boundary

#### Scenario: Mode indicator in file mode
- **WHEN** the page is showing a file opened through the picker
- **THEN** the mode indicator says the file never left the browser
