## ADDED Requirements

### Requirement: Filter chips come from the field inventory
The UI SHALL offer filter chips for `level` and for the fields the parser reported, showing each
field's known values with their counts. A field marked high-cardinality SHALL offer typed value
entry instead of a value list.

#### Scenario: Values offered with counts
- **WHEN** a dataset has three distinct `service` values
- **THEN** the UI offers all three as selectable values with their occurrence counts

#### Scenario: High-cardinality field
- **WHEN** a field was marked high-cardinality by the parser
- **THEN** the UI accepts a typed value for it and does not attempt to list its values

### Requirement: Combination rule
The application SHALL combine active filters as follows: several selected values of the same
field match an entry when it matches any of them, different fields must all match, and the
active time range and search text must also match. The rule SHALL be stated in the UI or its
help rather than left to be inferred.

#### Scenario: Two values of one field widen
- **WHEN** both `level=ERROR` and `level=WARN` are active
- **THEN** entries at either level are shown

#### Scenario: Two fields narrow
- **WHEN** `level=ERROR` and `service=api` are both active
- **THEN** only entries matching both are shown

#### Scenario: Range and search conjoin
- **WHEN** a time range, a level chip and a search term are all active
- **THEN** only entries satisfying all three are shown

### Requirement: Filter state is visible and reversible
The UI SHALL show every active filter, the resulting entry count against the total, and SHALL
allow removing any single filter or clearing them all.

#### Scenario: Counts reflect the predicate
- **WHEN** filters reduce a 10,000-entry dataset to 42 entries
- **THEN** the UI shows 42 of 10,000

#### Scenario: A filter that matches nothing is distinguishable
- **WHEN** the active filters match no entries
- **THEN** the UI says that filters excluded everything, distinct from an empty or unparsed file

### Requirement: Filters apply to live entries
Entries arriving from a live stream SHALL be evaluated against the active predicate as they
arrive, and the counter SHALL distinguish entries matching the filter from total entries
received.

#### Scenario: Live stream under an active filter
- **WHEN** a filter is active and new lines arrive
- **THEN** only matching entries appear in the table, and the UI still reports the total received

#### Scenario: Filter changed mid-stream
- **WHEN** the user changes a filter during an active stream
- **THEN** the already-received entries are re-evaluated and the view updates without reloading

### Requirement: Filtering performance
Applying or changing a filter over 10,000 entries SHALL complete within the 50ms target in
TDR §11, measured and recorded rather than assumed.

#### Scenario: Measured filter latency
- **WHEN** a filter is toggled on a 10,000-entry dataset
- **THEN** the measured time to updated results is recorded against the target, and a miss is
  either fixed or explicitly documented
