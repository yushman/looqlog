## MODIFIED Requirements

### Requirement: logcat columns become fields
A recognised logcat record SHALL contribute its tag as a `tag` field and its numeric columns
as `pid` and `tid`, adding `uid` when a third column is present. These SHALL be ordinary
fields, filterable like any other, and SHALL NOT remain part of the message text. The `tag`
field's value SHALL be the tag alone, carrying none of the whitespace `adb logcat` pads it
with, so that the same tag filters identically whether or not the line it came from was
padded.

#### Scenario: The tag is filterable
- **WHEN** a bugreport's logcat records carry tags such as `ActivityManager` and
  `ProcessCpuTracker`
- **THEN** `tag` appears in the field inventory with those values and their counts

#### Scenario: A padded tag is recorded without its padding
- **WHEN** the record is `01-01 03:00:01.182   135   135 I vold    : Vold 3.0 firing up`
- **THEN** the `tag` field's value is `vold`, and a `tag=vold` filter matches the entry

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
