## ADDED Requirements

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

## MODIFIED Requirements

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
