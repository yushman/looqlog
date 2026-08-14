# log-parsing Specification

## Purpose
The `log-parsing` capability covers `looq-core`'s incremental byte-chunk-in,
entries-and-diagnostics-out parse API, and the three MVP format parsers (JSON
Lines, logfmt, plain-text fallback) that run underneath it. It defines how a
malformed line, a non-UTF-8 byte sequence, or a chunk boundary that splits a line
or a multi-byte character are all handled without losing data or silently dropping
a line — the parser is shared unmodified between file mode (large byte chunks) and
stdin mode (one line at a time), per ADR-0005 and design.md D1.

## Requirements
### Requirement: Incremental parse API
`looq-core` SHALL expose a parser that accepts input as successive byte chunks and returns
the entries completed by each chunk, so that a file read in pieces and a stdin stream
arriving line by line use the same code path. The parser SHALL hold back an incomplete
trailing line until the next chunk or an explicit finish call, and SHALL be target-agnostic:
no `wasm-bindgen`, no `web-sys`, no filesystem access (ADR-0005).

#### Scenario: Chunk boundary splits a line
- **WHEN** a log is fed in two chunks that split a line in the middle of its bytes
- **THEN** the parser produces exactly the same entries as feeding the whole input at once

#### Scenario: Trailing line without a newline
- **WHEN** input ends with a complete log line that has no trailing newline and the caller
  signals end of input
- **THEN** that line is emitted as an entry

#### Scenario: Line-at-a-time feeding
- **WHEN** the caller feeds one line per call, as stdin mode does
- **THEN** each call returns at most one entry and the result matches whole-input parsing

### Requirement: JSON Lines parsing
The parser SHALL treat each line as an independent JSON value. A line whose top level is a
JSON object SHALL yield an entry with that object's members available for field extraction.
A line that is valid JSON but not an object SHALL be treated as a malformed line rather than
producing an entry with no fields.

#### Scenario: Object line
- **WHEN** the line is `{"ts":"2026-08-08T17:42:01Z","level":"error","msg":"boom"}`
- **THEN** an entry is produced carrying all three members

#### Scenario: Non-object JSON
- **WHEN** the line is `[1,2,3]`
- **THEN** no entry is produced and a diagnostic records the line as malformed

### Requirement: logfmt parsing
The parser SHALL read `key=value` pairs separated by whitespace, SHALL support
double-quoted values containing spaces and escaped quotes, and SHALL treat a bare token with
no `=` as part of the message rather than discarding it.

#### Scenario: Quoted value with spaces
- **WHEN** the line is `level=error msg="connection refused" service=api`
- **THEN** `msg` is `connection refused` and `service` is `api`

#### Scenario: Bare tokens are kept
- **WHEN** the line is `starting up level=info`
- **THEN** an entry is produced whose message contains `starting up` and whose `level` field
  is `info`

### Requirement: Plain-text fallback
The plain-text parser SHALL always produce one entry per non-empty line and SHALL never
report a line as malformed. It SHALL extract a timestamp and a level when recognisable
patterns are present and leave them absent otherwise, with the full line as the message.

#### Scenario: Unstructured line still becomes an entry
- **WHEN** the line is `something happened that matches no pattern at all`
- **THEN** one entry is produced with that text as its message and no timestamp or level

#### Scenario: Blank lines are skipped silently
- **WHEN** input contains empty and whitespace-only lines
- **THEN** they produce neither entries nor diagnostics

### Requirement: One line is one entry
The parser SHALL NOT join consecutive lines into a single entry. Multi-line payloads such as
Java stack traces SHALL yield one entry per physical line, which is a deliberate MVP
limitation (PRD §14 Q3) rather than an oversight.

#### Scenario: Stack trace becomes several entries
- **WHEN** a five-line Java stack trace is parsed
- **THEN** five entries are produced and no diagnostic is emitted about them

### Requirement: Malformed lines are skipped and reported
A line that the active parser cannot parse SHALL be skipped without producing an entry,
SHALL NOT abort the parse, and SHALL produce a diagnostic recording the line number and the
reason (PRD §14 Q2). Silently dropping a line is a defect.

#### Scenario: Bad line does not stop the parse
- **WHEN** a 1000-line JSON fixture contains one truncated line at line 500
- **THEN** 999 entries are produced and one diagnostic names line 500 and the reason

#### Scenario: Every skipped line is accounted for
- **WHEN** any input is fully parsed
- **THEN** entries produced plus lines skipped plus blank lines equals the total line count

### Requirement: Diagnostics are bounded and aggregated
The parser SHALL retain at most a fixed number of individual diagnostics and SHALL keep exact
counts per reason beyond that limit, so that a badly broken input yields a usable summary
instead of unbounded memory growth.

#### Scenario: Pathologically broken input
- **WHEN** a file of one million unparsable lines is parsed with the JSON format active
- **THEN** the retained diagnostic list stays within the cap, the reported count is one
  million, and memory does not grow proportionally to the number of bad lines

#### Scenario: Reasons are distinguishable
- **WHEN** an input contains both truncated JSON and non-object JSON lines
- **THEN** the summary reports the two reasons separately with their own counts

### Requirement: Encoding fallback
Input SHALL be decoded as UTF-8. When a chunk contains byte sequences that are not valid
UTF-8, the parser SHALL fall back to latin-1 decoding for the affected lines rather than
failing or emitting replacement characters silently, and SHALL record that the fallback was
used.

#### Scenario: Latin-1 line among UTF-8 lines
- **WHEN** a fixture contains one latin-1 encoded line among valid UTF-8 lines
- **THEN** all lines produce entries, the latin-1 line's text is readable, and a diagnostic
  records the encoding fallback

#### Scenario: Multi-byte character split across chunks
- **WHEN** a chunk boundary falls inside a multi-byte UTF-8 character
- **THEN** the character is decoded correctly once the next chunk arrives, with no fallback
  recorded

