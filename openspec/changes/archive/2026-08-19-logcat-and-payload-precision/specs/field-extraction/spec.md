## MODIFIED Requirements

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

## ADDED Requirements

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
