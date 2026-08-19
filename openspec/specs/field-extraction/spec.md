# field-extraction Specification

## Purpose
The `field-extraction` capability covers turning a parsed line into a structured
`Entry` — timestamp, level, message, and the arbitrary field inventory that
`filtering-and-search` will build chips from. It defines the field-name precedence
used to recognise a timestamp/level/message among a line's fields, the timezone
policy applied to offset-less timestamps, the fixed level-alias table, and the
per-field cardinality cap that keeps a high-cardinality field (e.g. `request_id`)
from producing an unbounded value list.
## Requirements
### Requirement: Entry shape
Every parsed line SHALL become an `Entry` carrying an optional timestamp, an optional level,
a message, the line's ordinal position in the input, and the arbitrary fields extracted from
it. Entries SHALL be returned in input order; the parser SHALL NOT sort or reorder them.

#### Scenario: Out-of-order timestamps are preserved
- **WHEN** input contains a line timestamped earlier than the line before it
- **THEN** both entries are returned in input order with their own timestamps, and no
  reordering or correction happens

#### Scenario: Ordinal identifies the source line
- **WHEN** an entry is produced from the 42nd line of the input
- **THEN** its ordinal is 42, so diagnostics and the UI can point at the original line

### Requirement: Timestamp extraction
The parser SHALL look for a timestamp in the field names `timestamp`, `ts`, `time`, `@timestamp`
and `t`, in that order, for formats that carry named fields, and by pattern match at the head of
the line for plain text. It SHALL accept RFC 3339 / ISO 8601 values and integer epoch values
in seconds, milliseconds and microseconds, disambiguated by magnitude. For plain text it SHALL
additionally recognise the syslog RFC 3164, klog, Apache/CLF and slash-date shapes, and SHALL
search token starts within a bounded window at the head of the line rather than only at
offset 0.

#### Scenario: Named field precedence
- **WHEN** a JSON line carries both `time` and `@timestamp`
- **THEN** the value of `time` is used, per the stated order

#### Scenario: Epoch milliseconds
- **WHEN** a field holds `1786000000000`
- **THEN** it is interpreted as epoch milliseconds

#### Scenario: Leading timestamp in plain text
- **WHEN** the line is `2026-08-08T17:42:01Z something happened`
- **THEN** the timestamp is extracted and the message is the remainder of the line

#### Scenario: Non-ISO shape in plain text
- **WHEN** the line is `I0808 17:42:01.123456       1 main.go:10] starting`
- **THEN** the timestamp is extracted from the klog shape

#### Scenario: Timestamp inside the head window
- **WHEN** the line opens with an address before a bracketed CLF timestamp
- **THEN** the timestamp is found without being anchored at offset 0

### Requirement: Timezone policy for offset-less timestamps
A timestamp carrying an explicit offset SHALL be interpreted with that offset. A timestamp
carrying none SHALL be interpreted in a timezone supplied by the caller, defaulting to UTC
(PRD §12, TDR §16). The chosen interpretation SHALL be reported alongside the parse result so
a caller can tell the user which assumption was applied.

#### Scenario: Offset is respected
- **WHEN** the value is `2026-08-08T17:42:01+03:00`
- **THEN** it is stored as the corresponding instant, not shifted again

#### Scenario: Naive timestamp defaults to UTC
- **WHEN** the value is `2026-08-08 17:42:01` and the caller supplied no timezone
- **THEN** it is interpreted as UTC and the result reports that the UTC default was used

#### Scenario: Caller-supplied timezone
- **WHEN** the caller supplies `Europe/Belgrade` and the value carries no offset
- **THEN** it is interpreted in that zone

### Requirement: Entries without a usable timestamp are kept
An entry whose timestamp is missing or unparsable SHALL still be produced, with an absent
timestamp, and SHALL be counted so a caller can report how many entries cannot appear on a
time axis. Dropping such entries is a defect.

#### Scenario: Timestampless lines survive
- **WHEN** a plain-text file has no timestamps at all
- **THEN** every line becomes an entry with no timestamp, and the result reports that all
  entries lack one

#### Scenario: Unparsable timestamp value
- **WHEN** a JSON line has `"ts": "yesterday"`
- **THEN** the entry is produced with no timestamp and a diagnostic records the unparsable
  value, while `ts` remains available as an ordinary field

### Requirement: Level extraction
The parser SHALL take the level from a dedicated field named `level`, `lvl` or `severity` when
the format provides one. For a line with a recognised timestamp prefix it SHALL then test the
token immediately following that prefix, accepting a bare word, a bracketed form (`[INFO]`,
`<INFO>`), a suffixed form (`INFO:`), a syslog priority (`<130>`) or a single-letter
klog/logcat code (`I`, `D`, `W`, `E`, `F`, `V`). Only when neither yields a level SHALL it fall
back to the first case-insensitive match of `TRACE|DEBUG|INFO|WARN|ERROR|FATAL` anywhere in the
message. That whole-message scan SHALL skip any token immediately followed by `=`, because such
a token is a key rather than a level. Single-letter codes SHALL be accepted only in the
positional slot, never by the whole-message scan. It SHALL normalise a fixed alias set —
`WARNING` to `WARN`, `ERR` to `ERROR`, `CRITICAL` to `FATAL` — SHALL map the eight syslog
severities onto that set (`emerg`/`alert`/`crit` to FATAL, `err` to ERROR, `warning` to WARN,
`notice`/`info` to INFO, `debug` to DEBUG), and SHALL leave the level absent when nothing
matches rather than guessing a default.

#### Scenario: Dedicated field wins over message text
- **WHEN** a JSON line has `"level":"info"` and a message containing the word `error`
- **THEN** the entry's level is `INFO`

#### Scenario: Positional token wins over a later word
- **WHEN** the line is `2026-08-08T17:42:01Z INFO retrying after ERROR response`
- **THEN** the entry's level is `INFO`

#### Scenario: A key is not a level
- **WHEN** a message contains `err=Success` and nothing else level-like
- **THEN** no level is extracted, rather than `ERROR` via the `ERR` alias

#### Scenario: A level as a field value still resolves
- **WHEN** a line carries the pair `level=err`
- **THEN** the entry's level is `ERROR`, because the token on the value side of `=` is
  unaffected by the key rule

#### Scenario: Whole-message scan still applies without a prefix
- **WHEN** the line is `app: ERROR something broke` with no recognisable timestamp
- **THEN** the entry's level is `ERROR`

#### Scenario: Syslog priority maps onto the level set
- **WHEN** a line's positional token is `<130>` (facility 16, severity 2 — `crit`)
- **THEN** the entry's level is `FATAL`

#### Scenario: Single letter only counts positionally
- **WHEN** a message with no timestamp prefix contains a standalone `E` mid-sentence
- **THEN** no level is extracted from it

#### Scenario: Alias normalisation
- **WHEN** a line's level value is `warning`
- **THEN** the entry's level is `WARN`

#### Scenario: No level is absent, not INFO
- **WHEN** a line contains no recognisable level
- **THEN** the entry's level is absent and no default is substituted

### Requirement: Arbitrary field extraction
The parser SHALL turn top-level JSON object members other than the recognised timestamp, level
and message keys into fields. Nested objects and arrays SHALL be preserved as their JSON text
rather than flattened, and a brace-delimited logfmt value SHALL likewise be kept whole rather
than contributing its members as top-level fields. For logfmt, every remaining `key=value` pair
SHALL become a field. Plain text SHALL contribute the fields of a structured payload parsed
from behind its prefix, the columns of a recognised logcat record, and no fields otherwise.

#### Scenario: Custom field becomes filterable
- **WHEN** a fixture's lines carry `service=api`
- **THEN** `service` appears in the field inventory with `api` among its values

#### Scenario: Nested object is kept as text
- **WHEN** a JSON line contains `"http":{"status":500}`
- **THEN** the field `http` holds the JSON text of that object and is not split into
  `http.status`

#### Scenario: A dumped map does not become many fields
- **WHEN** a line's logfmt payload carries `headers={Alt-Svc=[h3], Content-Length=[0]}`
- **THEN** `headers` is one field holding that text, and `Alt-Svc` is not a field

#### Scenario: Plain text with a structured payload contributes fields
- **WHEN** a plain-text line is `2026-08-08 17:42:01 INFO {"service":"api"}`
- **THEN** `service` appears in the field inventory

#### Scenario: Plain text without a structured payload contributes none
- **WHEN** a plain-text line carries only free text after its prefix
- **THEN** it contributes no fields

### Requirement: Field inventory with a cardinality cap
The parser SHALL maintain an inventory of the field names seen and, for each, up to a fixed
number of distinct values with their counts. A field exceeding that cap SHALL be marked
high-cardinality and SHALL stop accumulating values, so a `request_id` field cannot produce an
unbounded value list for the UI to render.

#### Scenario: Low-cardinality field lists its values
- **WHEN** a fixture has three distinct `service` values
- **THEN** the inventory lists all three with their occurrence counts

#### Scenario: High-cardinality field is capped and flagged
- **WHEN** a fixture has 50,000 distinct `request_id` values
- **THEN** the inventory marks `request_id` high-cardinality, retains no more than the cap,
  and memory does not grow with the number of distinct values

### Requirement: An inferred year is reported
When a recognised timestamp shape carries no year, the parser SHALL infer one from a
caller-supplied reference instant — that instant's year, stepped back by one when using it
would place the entry in the future — and SHALL mark the entry as having an inferred year, so
a caller can tell the user which entries are dated by assumption. The crate SHALL NOT read the
system clock itself, keeping it target-agnostic per ADR-0005.

#### Scenario: Year-less timestamp is dated and flagged
- **WHEN** the line is `Aug  8 17:42:01 host app: hello` with a reference instant in 2026
- **THEN** the entry's timestamp falls in 2026 and the entry is marked year-inferred

#### Scenario: A date that would be in the future steps back a year
- **WHEN** the line's month/day is December and the reference instant is in January 2026
- **THEN** the entry is dated in 2025 rather than in the future

#### Scenario: A timestamp carrying its own year is not flagged
- **WHEN** the line's timestamp includes a year
- **THEN** the entry is not marked year-inferred

### Requirement: Prefix and payload disagreements stay visible
The payload's value SHALL win when both a recognised prefix and a parsed payload supply a
timestamp, level or message. A prefix timestamp that differs from the payload's SHALL be
retained as an ordinary field rather than discarded, so the disagreement is inspectable and
filterable.

#### Scenario: Payload timestamp and level win
- **WHEN** the line is
  `2026-08-08 17:42:01 INFO {"ts":"2026-08-08T17:41:59Z","level":"error","msg":"boom"}`
- **THEN** the entry's timestamp is `17:41:59`, its level is `ERROR` and its message is `boom`

#### Scenario: The discarded prefix timestamp is kept as a field
- **WHEN** a prefix timestamp differs from the payload's
- **THEN** the prefix value is present as a field on the entry

#### Scenario: The prefix fills what the payload omits
- **WHEN** a payload carries a message but no timestamp or level
- **THEN** the prefix's timestamp and level are used

### Requirement: logcat columns become fields
A recognised logcat record SHALL contribute its tag as a `tag` field and its numeric columns
as `pid` and `tid`, adding `uid` when a third column is present. These SHALL be ordinary
fields, filterable like any other, and SHALL NOT remain part of the message text.

#### Scenario: The tag is filterable
- **WHEN** a bugreport's logcat records carry tags such as `ActivityManager` and
  `ProcessCpuTracker`
- **THEN** `tag` appears in the field inventory with those values and their counts

#### Scenario: Three columns yield uid, pid and tid
- **WHEN** a record's columns are `1000   806   995`
- **THEN** `uid` is `1000`, `pid` is `806` and `tid` is `995`

#### Scenario: Two columns yield pid and tid only
- **WHEN** a record's columns are `806 29149`
- **THEN** `pid` is `806`, `tid` is `29149`, and no `uid` field is produced

#### Scenario: Columns leave the message
- **WHEN** a logcat record is parsed
- **THEN** its message is the text after the tag's colon, carrying neither the columns nor
  the tag

