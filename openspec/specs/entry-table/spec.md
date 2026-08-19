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
rather than as an empty cell that could be mistaken for a blank value. An absence marker SHALL
fit within its column's width; no cell's content SHALL render outside its own column. The level
column MAY render as a compact, color-coded abbreviation rather than the full level word,
provided the full level name remains available as accessible text (for assistive technology) and
as a hover tooltip — an abbreviation SHALL NOT be the only way the level is exposed.

#### Scenario: Timezone is stated, not implied
- **WHEN** entries are displayed after being parsed with the UTC default
- **THEN** the UI states that timestamps are shown in UTC

#### Scenario: Missing values are explicit
- **WHEN** an entry has no level
- **THEN** the level cell shows an explicit absence marker rather than looking like an empty
  string value

#### Scenario: An absence marker stays inside its column
- **WHEN** a file's entries mostly have neither a timestamp nor a level
- **THEN** each absence marker renders inside its own column and none of them overlaps the
  message text

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

### Requirement: Rows stay selectable while entries arrive
Re-rendering the table as entries arrive SHALL NOT replace the row elements a user is interacting
with. A row showing the same entry SHALL survive an update untouched, so that an ordinary click — a
press and a release separated by a human interval — selects it while a stream is running. Rows MAY be
rewritten when the entry they show, its selected state or the active search changes; they SHALL NOT
be rebuilt wholesale on every render.

#### Scenario: Selecting an entry during a live stream
- **WHEN** entries are arriving continuously, the view is not following the tail, and the user
  presses a row and releases it roughly a sixth of a second later
- **THEN** that entry becomes the selection and the detail view describes it

#### Scenario: Unchanged rows are left alone
- **WHEN** a live batch arrives and the visible rows still show the same entries
- **THEN** those rows are not re-created, and anything the pointer or keyboard focus is on survives

### Requirement: Columns can be resized
The table SHALL let the user change column widths by dragging a boundary in the header, and all
rows SHALL stay aligned to the same widths. The message column SHALL absorb the remaining width
so the row always fills the viewport horizontally. Drag handles SHALL exist only in the header,
never in data rows, so that a drag cannot begin on a row the user meant to select.

#### Scenario: Dragging a boundary resizes a column
- **WHEN** the user drags the boundary at the right edge of the timestamp column
- **THEN** that column's width follows the pointer and every row's timestamp column matches the
  header's

#### Scenario: The message column takes the remainder
- **WHEN** any other column is widened or narrowed
- **THEN** the message column shrinks or grows to match, and the row still exactly fills the
  viewport width

#### Scenario: Resizing does not select a row
- **WHEN** the user presses on a header boundary and drags across the table
- **THEN** no row becomes selected

### Requirement: Column widths have a floor and a reset
Each resizable column SHALL have a minimum width, and a drag below it SHALL clamp to that
minimum rather than refusing to move. The user SHALL be able to restore a single column to its
default width and to restore all columns at once.

#### Scenario: A column cannot be dragged away
- **WHEN** the user drags a boundary far past the left edge of its column
- **THEN** the column stops at its minimum width and remains visible and re-draggable

#### Scenario: Resetting one column
- **WHEN** the user double-clicks a boundary
- **THEN** the column to its left returns to its default width and the others are unchanged

#### Scenario: Resetting every column
- **WHEN** the user activates the reset control
- **THEN** all columns return to their default widths

### Requirement: Row positioning does not use inline style attributes
The virtual scroller SHALL position rows and size its spacer through stylesheet rules it
mutates via the CSS object model, not by writing `style` attributes on elements, so that the
served Content Security Policy does not need to permit inline styles. The mechanism SHALL work
in every browser PRD §11 lists as supported.

#### Scenario: No inline styles are written while scrolling
- **WHEN** the table is scrolled through a large dataset
- **THEN** no element in the table carries a `style` attribute written by the scroller

#### Scenario: The strict policy does not break the table
- **WHEN** the page is loaded under a policy that forbids inline styles
- **THEN** rows render and position correctly and no CSP violation is reported in the console

#### Scenario: Scrolling stays within its performance budget
- **WHEN** 50,000 entries are scrolled after the change
- **THEN** per-frame time is measured against the previously recorded figure and any regression
  is reported rather than accepted silently

