## MODIFIED Requirements

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

## ADDED Requirements

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
