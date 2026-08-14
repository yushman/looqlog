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
and `t`, in that order, for formats that carry named fields, and by pattern match at the start
of the line for plain text. It SHALL accept RFC 3339 / ISO 8601 values and integer epoch values
in seconds, milliseconds and microseconds, disambiguated by magnitude.

#### Scenario: Named field precedence
- **WHEN** a JSON line carries both `time` and `@timestamp`
- **THEN** the value of `time` is used, per the stated order

#### Scenario: Epoch milliseconds
- **WHEN** a field holds `1786000000000`
- **THEN** it is interpreted as epoch milliseconds

#### Scenario: Leading timestamp in plain text
- **WHEN** the line is `2026-08-08T17:42:01Z something happened`
- **THEN** the timestamp is extracted and the message is the remainder of the line

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
the format provides one, and otherwise from the first case-insensitive match of
`TRACE|DEBUG|INFO|WARN|ERROR|FATAL` in the message. It SHALL normalise a fixed alias set —
`WARNING` to `WARN`, `ERR` to `ERROR`, `CRITICAL` to `FATAL` — and SHALL leave the level absent
when nothing matches rather than guessing a default.

#### Scenario: Dedicated field wins over message text
- **WHEN** a JSON line has `"level":"info"` and a message containing the word `error`
- **THEN** the entry's level is `INFO`

#### Scenario: Alias normalisation
- **WHEN** a line's level value is `warning`
- **THEN** the entry's level is `WARN`

#### Scenario: No level is absent, not INFO
- **WHEN** a line contains no recognisable level
- **THEN** the entry's level is absent and no default is substituted

### Requirement: Arbitrary field extraction
The parser SHALL turn top-level JSON object members other than the recognised timestamp, level
and message keys into fields. Nested objects and arrays SHALL be preserved as their JSON text
rather than flattened. For logfmt, every remaining `key=value` pair SHALL become a field. Plain text
SHALL contribute no fields.

#### Scenario: Custom field becomes filterable
- **WHEN** a fixture's lines carry `service=api`
- **THEN** `service` appears in the field inventory with `api` among its values

#### Scenario: Nested object is kept as text
- **WHEN** a JSON line contains `"http":{"status":500}`
- **THEN** the field `http` holds the JSON text of that object and is not split into
  `http.status`

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

