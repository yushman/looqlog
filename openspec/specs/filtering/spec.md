# filtering Specification

## Purpose

The `filtering` capability covers the field filter chips and the one predicate every view agrees on:
chips derived from the parser's field inventory with per-value counts (typed entry instead of a
value list for high-cardinality fields), the stated OR-within-a-field / AND-across-fields
combination with the active time range and search text, filter state that is always visible and
reversible, filters that keep applying as live entries arrive, and filtering fast enough to stay
interactive at the supported dataset sizes.

## Requirements
### Requirement: Filter chips come from the field inventory
The UI SHALL offer filter controls for `level` and for the fields the parser reported, showing each
field's known values with their counts. A field marked high-cardinality SHALL offer typed value
entry instead of a value list. The controls SHALL be grouped per field into sections the user can
collapse and expand, so a log with many fields does not bury the log itself; which sections start
open is a presentation choice, but every section SHALL be reachable and SHALL state what it contains
while collapsed.

#### Scenario: Values offered with counts
- **WHEN** a dataset has three distinct `service` values
- **THEN** the UI offers all three as selectable values with their occurrence counts

#### Scenario: High-cardinality field
- **WHEN** a field was marked high-cardinality by the parser
- **THEN** the UI accepts a typed value for it and does not attempt to list its values

#### Scenario: Many fields stay manageable
- **WHEN** a log produces more fields than fit on screen at once
- **THEN** each field's values live in a section that can be collapsed, and a collapsed section still
  names its field and says how many values it holds

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


### Requirement: Filter controls stay operable while entries arrive
Updating filter controls with new counts from a live stream SHALL NOT replace or discard the controls
themselves. A filter control SHALL remain the same element across such updates, so that an ordinary
click — a press and a release separated by a human interval — toggles the filter, and so that focus
and half-entered text survive. Controls SHALL only be added or removed when the set of fields or
values actually changes.

#### Scenario: A click during a live stream toggles the filter
- **WHEN** entries are arriving continuously and the user presses a filter control and releases it
  roughly a sixth of a second later
- **THEN** the filter toggles, exactly as it does with no stream running

#### Scenario: Typed input is not swallowed
- **WHEN** the user is part-way through typing a value into a high-cardinality field's input and
  several batches of live entries arrive
- **THEN** the typed text and the caret position are unchanged

#### Scenario: Counts still track the stream
- **WHEN** live entries change how many entries carry a value
- **THEN** the control's count updates without the control being rebuilt
