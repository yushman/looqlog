## MODIFIED Requirements

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

## ADDED Requirements

### Requirement: Prefix recognition beyond ISO 8601
The plain-text parser SHALL recognise a leading timestamp in the syslog RFC 3164
(`Aug  8 17:42:01`), klog (`0808 17:42:01.123456`), Apache/CLF
(`08/Aug/2026:17:42:01 +0000`), slash-date (`2026/08/08 17:42:01`) and integer-epoch shapes
in addition to the ISO 8601 form. A timestamp SHALL be searched for at token starts within
a bounded window at the head of the line, not only at offset 0, so that a line opening with
an address or hostname is still recognised. The entire pattern SHALL match for a candidate
to be accepted.

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
