# entry-table Specification

## Purpose

The `entry-table` capability covers the virtual-scrolled table of parsed log entries: fixed-height
rows bounded by the viewport regardless of dataset size, timestamp/level/message columns with
explicit-absence handling for missing values, truncation with a reachable full-text detail view for
long messages, following the shell's active time range, and surviving live growth and eviction
without losing the user's place.

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
rather than as an empty cell that could be mistaken for a blank value. The level column MAY render
as a compact, color-coded abbreviation rather than the full level word, provided the full level
name remains available as accessible text (for assistive technology) and as a hover tooltip — an
abbreviation SHALL NOT be the only way the level is exposed.

#### Scenario: Timezone is stated, not implied
- **WHEN** entries are displayed after being parsed with the UTC default
- **THEN** the UI states that timestamps are shown in UTC

#### Scenario: Missing values are explicit
- **WHEN** an entry has no level
- **THEN** the level cell shows an explicit absence marker rather than looking like an empty
  string value

#### Scenario: Abbreviated level still exposes its full name
- **WHEN** the level column renders a compact abbreviation (for example, a single letter) instead
  of the full level word
- **THEN** the full level name is still available to a screen reader and as a tooltip on hover, so
  no user loses access to the actual value

### Requirement: Long messages do not break the layout
The table SHALL truncate long messages to a single line per row and SHALL make the full text
reachable through the detail view. Row height SHALL be uniform.

#### Scenario: Very long line
- **WHEN** an entry's message is several thousand characters
- **THEN** its row is the same height as every other row and the message is truncated with an
  indication that it continues

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

