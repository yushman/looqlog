# entry-table Specification

## Purpose
TBD - created by archiving change timeline-and-table. Update Purpose after archive.
## Requirements
### Requirement: Virtual scrolling
The table SHALL render only the rows in and near the viewport, so the number of DOM nodes stays
bounded regardless of dataset size. Scrolling a 50,000-entry dataset SHALL stay smooth.

#### Scenario: Large dataset does not enter the DOM
- **WHEN** a 50,000-entry fixture is displayed
- **THEN** the rendered row count is bounded by the viewport, not by the dataset

#### Scenario: Jumping to the end is immediate
- **WHEN** the user drags the scrollbar from the top to the bottom of a large dataset
- **THEN** the last rows appear without a visible stall

### Requirement: Columns
The table SHALL show timestamp, level and message columns. Timestamps SHALL be rendered in the
timezone the parser applied and SHALL state which that is. Levels SHALL be visually
distinguishable, and an entry with no level or no timestamp SHALL render as explicitly absent
rather than as an empty cell that could be mistaken for a blank value.

#### Scenario: Timezone is stated, not implied
- **WHEN** entries are displayed after being parsed with the UTC default
- **THEN** the UI states that timestamps are shown in UTC

#### Scenario: Missing values are explicit
- **WHEN** an entry has no level
- **THEN** the level cell shows an explicit absence marker rather than looking like an empty
  string value

### Requirement: Long messages do not break the layout
The table SHALL truncate long messages to a single line per row and SHALL make the full text
reachable through the detail view. Row height SHALL be uniform.

#### Scenario: Very long line
- **WHEN** an entry's message is several thousand characters
- **THEN** its row is the same height as every other row and the message is truncated with an
  indication that it continues

### Requirement: Row selection and detail view
Selecting a row SHALL open a detail view showing the entry's full message and every extracted
field with its value, including fields not shown as columns.

#### Scenario: Fields are all reachable
- **WHEN** an entry parsed from JSON carries fields beyond timestamp, level and message
- **THEN** the detail view lists each of them with its value

#### Scenario: Nested JSON is readable
- **WHEN** an entry has a field holding a nested JSON object kept as text
- **THEN** the detail view shows that text in a readable form

### Requirement: The table follows the active range
The table SHALL show only entries matching the active time range, and SHALL state the shown
count against the total when a range is active.

#### Scenario: Range narrows the table
- **WHEN** a time range is selected on the timeline
- **THEN** the table shows only entries in that range and reports how many of the total are shown

### Requirement: Growth and eviction are handled
The table SHALL append entries arriving from a live stream without losing the user's scroll
position when the view is paused, and SHALL handle eviction of the oldest entries without
throwing or scrolling unexpectedly.

#### Scenario: Paused scroll position is stable
- **WHEN** the user has scrolled away from the tail and entries continue to arrive
- **THEN** the rows under the cursor stay where they are

#### Scenario: Eviction while scrolled into the evicted region
- **WHEN** the oldest entries are evicted while the user is viewing them
- **THEN** the table adjusts without an error and indicates that earlier entries are no longer
  retained

