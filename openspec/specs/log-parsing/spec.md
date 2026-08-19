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
no `=` as part of the message rather than discarding it. A value beginning with `{` SHALL be
consumed to its matching `}` as one opaque block — tracking nesting depth and ignoring braces
inside quoted spans — and stored whole, so that a brace-delimited structure dumped into a log
line becomes one field rather than contributing each of its members as a top-level field.

#### Scenario: Quoted value with spaces
- **WHEN** the line is `level=error msg="connection refused" service=api`
- **THEN** `msg` is `connection refused` and `service` is `api`

#### Scenario: Bare tokens are kept
- **WHEN** the line is `starting up level=info`
- **THEN** an entry is produced whose message contains `starting up` and whose `level` field
  is `info`

#### Scenario: A braced value is one field, not many
- **WHEN** the line carries `time=18ms ret=204 headers={null=[HTTP/1.1 204], Alt-Svc=[h3], Content-Length=[0]}`
- **THEN** the fields are exactly `time`, `ret` and `headers`, and `Alt-Svc` and
  `Content-Length` do not appear as fields of their own

#### Scenario: Sibling pairs beside a braced value still parse
- **WHEN** a line carries genuine pairs both before and after a brace-delimited value
- **THEN** every sibling pair becomes its own field and the braced value stays intact

#### Scenario: Nested braces are balanced
- **WHEN** a braced value itself contains a brace-delimited value
- **THEN** the outer block is consumed to its own matching brace, not to the first `}`

### Requirement: Plain-text fallback
The plain-text parser SHALL always produce one entry per non-empty line and SHALL never
report a line as malformed. It SHALL extract a timestamp and a level when recognisable
patterns are present and leave them absent otherwise. The message SHALL be the line with
the recognised timestamp and level tokens removed; any text preceding them within the head
window SHALL be preserved in the message rather than discarded, and when nothing is
recognised the message SHALL be the full line. A line whose payload fails to parse under a
structured parser SHALL keep that payload as message text and SHALL NOT be reported as
malformed.

#### Scenario: Unstructured line still becomes an entry
- **WHEN** the line is `something happened that matches no pattern at all`
- **THEN** one entry is produced with that text as its message and no timestamp or level

#### Scenario: Blank lines are skipped silently
- **WHEN** input contains empty and whitespace-only lines
- **THEN** they produce neither entries nor diagnostics

#### Scenario: A payload that fails to parse is not malformed
- **WHEN** the line is `2026-08-08 17:42:01 INFO {"broken":` — a recognisable prefix
  followed by truncated JSON
- **THEN** an entry is produced with the prefix timestamp and level, the truncated text as
  its message, and no diagnostic is emitted

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

### Requirement: Prefix recognition beyond ISO 8601
The plain-text parser SHALL recognise a leading timestamp in the syslog RFC 3164
(`Aug  8 17:42:01`), klog (`0808 17:42:01.123456`), logcat (`04-21 13:07:53.198`),
Apache/CLF (`08/Aug/2026:17:42:01 +0000`), slash-date (`2026/08/08 17:42:01`) and
integer-epoch shapes in addition to the ISO 8601 form. A timestamp SHALL be searched for at
token starts within a bounded window at the head of the line, not only at offset 0, so that a
line opening with an address or hostname is still recognised. The entire pattern SHALL match
for a candidate to be accepted.

#### Scenario: Syslog line gets a timestamp
- **WHEN** the line is `Aug  8 17:42:01 host app[123]: connection refused`
- **THEN** an entry is produced with a timestamp, and its message is the remainder of the
  line

#### Scenario: Access-log timestamp is not at offset 0
- **WHEN** the line is `1.2.3.4 - - [08/Aug/2026:17:42:01 +0000] "GET /x HTTP/1.1" 500 12`
- **THEN** the bracketed timestamp is extracted, including its explicit offset

#### Scenario: Text before the timestamp survives
- **WHEN** the line is `1.2.3.4 - - [08/Aug/2026:17:42:01 +0000] "GET /x HTTP/1.1" 500 12`
- **THEN** the message still contains `1.2.3.4`, because dropping everything before the
  recognised timestamp would silently lose the client address on every access-log line

#### Scenario: A number late in the line is not a timestamp
- **WHEN** a line's first 64 bytes contain no timestamp but a date-like number appears
  further along
- **THEN** the entry has no timestamp

#### Scenario: ISO recognition is unchanged
- **WHEN** the line is `2026-08-08T17:42:01Z something happened`
- **THEN** the timestamp is extracted and the message is `something happened`, as before

### Requirement: Structured payloads are parsed, not left as text
When the text remaining after a recognised prefix is itself structured, the parser SHALL
parse it with the format's own parser and merge the result into the entry: text starting
with `{` SHALL be offered to the JSON parser, and text carrying at least two `key=value`
pairs SHALL be offered to the logfmt parser. Nesting SHALL be resolved one level only.

#### Scenario: JSON payload behind a plain prefix
- **WHEN** the line is `2026-08-08 17:42:01 INFO {"status":500,"path":"/x"}`
- **THEN** the entry carries fields `status` and `path`, available to filters, rather than
  a message containing the raw JSON text

#### Scenario: logfmt payload behind a syslog prefix
- **WHEN** the line is `Aug  8 17:42:01 host app: level=error msg="boom" service=api`
- **THEN** the entry carries `service` as a field and `boom` as its message

#### Scenario: Prose is not mistaken for logfmt
- **WHEN** the message text after a prefix contains a single `foo=bar` inside a sentence
- **THEN** no fields are extracted and the text stays the message

#### Scenario: Payload nesting stops at one level
- **WHEN** a payload's own message value itself looks like a structured line
- **THEN** it is left as message text and not parsed again

### Requirement: Docker JSON wrapper is unwrapped
A JSON line whose top-level object members are exactly `log`, `stream` and `time` SHALL be
treated as a Docker json-file record: the `log` member SHALL be parsed as a log line in its
own right, `time` SHALL supply the timestamp when the inner line carries none, and `stream`
SHALL be kept as a field. An object that merely contains a `log` member among others SHALL
NOT be unwrapped.

#### Scenario: Container stdout is parsed, not shown as an escaped blob
- **WHEN** the line is
  `{"log":"2026-08-08T17:42:01Z ERROR boom\n","stream":"stdout","time":"2026-08-08T17:42:02Z"}`
- **THEN** the entry's level is ERROR, its message is `boom`, its timestamp comes from the
  inner line, and `stream` is a field

#### Scenario: Inner line without its own timestamp
- **WHEN** a Docker record's `log` member carries no recognisable timestamp
- **THEN** the wrapper's `time` value supplies the entry's timestamp

#### Scenario: An ordinary object with a log field is left alone
- **WHEN** a JSON line is `{"log":"hi","level":"info","service":"api"}`
- **THEN** it is parsed as an ordinary JSON entry and `log` stays a field

### Requirement: logcat records are recognised as a whole shape
The parser SHALL recognise a logcat record as the complete sequence of a `MM-DD hh:mm:ss.mmm`
timestamp, two or three uid/pid/tid columns, a single severity letter from `V D I W E F`, and
a tag terminated by `:`. A column SHALL be all digits, a short lowercase word, or the
`u0_aNN` form. The record SHALL be accepted only when the entire sequence including the
tag's colon matches; a partial match SHALL leave the line to the other shapes rather than
consuming its head. The severity letter SHALL supply the entry's level, since it does not sit
in the position the generic level matcher inspects; the letter `S` (silent) SHALL supply none,
after which the record follows the same fallback path as any other line whose prefix yielded no
level.

#### Scenario: Three-column layout with a numeric uid
- **WHEN** the line is `04-18 19:21:16.151  1000   806   995 D ActivityManager: freezing 2521 com.x`
- **THEN** the entry has a timestamp, level DEBUG, and its message is `freezing 2521 com.x`

#### Scenario: Two-column layout
- **WHEN** the line is `04-21 13:07:51.985   806 29149 W UsbDescriptorParser: Unrecognized len: 58`
- **THEN** the entry has a timestamp and level WARN

#### Scenario: Named and app-uid columns
- **WHEN** a record's first column is `root`, `shell` or `u0_a2` instead of digits
- **THEN** the record is recognised the same way

#### Scenario: A partial match is not consumed
- **WHEN** a line opens with `04-21 13:07:51.985   806 29149 W` but carries no `Tag:`
- **THEN** the line is not treated as logcat and its head is not consumed

#### Scenario: Silent severity supplies no level
- **WHEN** a record's severity letter is `S` and its message carries no level word
- **THEN** the entry has no level, because `S` is not mapped onto the level set

#### Scenario: The year is inferred and flagged
- **WHEN** a logcat record is parsed with a caller-supplied reference instant
- **THEN** its timestamp carries the inferred year and the entry is marked year-inferred

