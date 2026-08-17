## MODIFIED Requirements

### Requirement: Row selection and detail view
Selecting a row SHALL open a detail view showing the entry's full message and every extracted
field with its value, including fields not shown as columns. The detail view SHALL occupy a
dedicated area that exists whether or not an entry is selected, stating explicitly when nothing is
selected, so that inspecting an entry does not move the rows around it. The selection SHALL follow
the entry rather than a screen position: entries arriving or being evicted SHALL NOT silently change
which entry the detail view is describing, and an evicted selection SHALL be reported as such.

#### Scenario: Fields are all reachable
- **WHEN** an entry parsed from JSON carries fields beyond timestamp, level and message
- **THEN** the detail view lists each of them with its value

#### Scenario: Nested JSON is readable
- **WHEN** an entry has a field holding a nested JSON object kept as text
- **THEN** the detail view shows that text in a readable form

#### Scenario: Nothing selected is an explicit state
- **WHEN** no row has been selected
- **THEN** the detail area says so rather than rendering as an empty region

#### Scenario: Inspecting does not move the table
- **WHEN** a row is selected
- **THEN** the rows keep their positions and the table does not reflow to make room

#### Scenario: Selection survives live growth
- **WHEN** an entry is selected and new entries arrive
- **THEN** the detail view still describes the same entry; if that entry is evicted, the detail view
  says the entry is no longer retained rather than showing a different entry
