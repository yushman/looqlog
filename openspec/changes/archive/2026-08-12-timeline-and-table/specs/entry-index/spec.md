## ADDED Requirements

### Requirement: Time-ordered index over entries
The application SHALL maintain an index of entries ordered by timestamp, separate from the
input-order entry list, so that range queries and bucket counts do not scan every entry. Entries
with no usable timestamp SHALL be tracked separately rather than assigned a substitute time.

#### Scenario: Out-of-order input is indexed correctly
- **WHEN** a log contains entries whose timestamps are not monotonically increasing
- **THEN** a range query returns every entry within the range regardless of its input position

#### Scenario: Timestampless entries are held apart
- **WHEN** a dataset contains entries with no usable timestamp
- **THEN** they are counted in a separate group and never appear in a time-range result

### Requirement: Incremental maintenance under append and eviction
The index SHALL be updated incrementally as entries are appended and as the oldest entries are
evicted, without rebuilding from scratch. Its cost per appended entry SHALL NOT grow with the
size of the dataset.

#### Scenario: Live append stays cheap
- **WHEN** entries arrive continuously at a high rate
- **THEN** per-entry index maintenance time stays flat as the dataset grows

#### Scenario: Front eviction keeps the index consistent
- **WHEN** the oldest entries are evicted at the retention limit
- **THEN** they disappear from range queries and bucket counts, and no stale references remain

### Requirement: Range queries
The index SHALL answer a query for a half-open time range with the entries inside it, in input
order, and with a count, both fast enough to drive a redraw on drag.

#### Scenario: Range boundaries are unambiguous
- **WHEN** a range is selected whose start coincides exactly with an entry's timestamp
- **THEN** the boundary rule is applied consistently at both ends, and an entry cannot appear in
  two adjacent ranges

#### Scenario: Empty range
- **WHEN** a selected range contains no entries
- **THEN** the query returns an empty result and the UI can distinguish it from an error
