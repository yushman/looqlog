## MODIFIED Requirements

### Requirement: Time-ordered index over entries
The backend SHALL maintain, in `looqlog-core`, an index of entries ordered by timestamp, separate
from the input-order entry list, so that range queries and bucket counts do not scan every entry.
Entries with no usable timestamp SHALL be tracked separately rather than assigned a substitute
time. The browser SHALL NOT hold an index or the full entry list.

#### Scenario: Out-of-order input is indexed correctly
- **WHEN** a log contains entries whose timestamps are not monotonically increasing
- **THEN** a range query returns every entry within the range regardless of its input position

#### Scenario: Timestampless entries are held apart
- **WHEN** a dataset contains entries with no usable timestamp
- **THEN** they are counted in a separate group and never appear in a time-range result

#### Scenario: Browser memory does not scale with the log
- **WHEN** a 1,000,000-line file is open and the user scrolls from top to bottom
- **THEN** the page holds at most a bounded window of rows at any time

### Requirement: Range queries
The index SHALL answer a query for a half-open time range with the entries inside it, in input
order, and with a count, fast enough that a range drag round trip (request to rendered result)
drives a redraw without visible lag on a 100,000-entry dataset.

#### Scenario: Range boundaries are unambiguous
- **WHEN** a range is selected whose start coincides exactly with an entry's timestamp
- **THEN** the boundary rule is applied consistently at both ends, and an entry cannot appear in
  two adjacent ranges

#### Scenario: Empty range
- **WHEN** a selected range contains no entries
- **THEN** the query returns an empty result and the UI can distinguish it from an error
