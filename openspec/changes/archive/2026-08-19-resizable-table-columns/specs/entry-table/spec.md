## MODIFIED Requirements

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

## ADDED Requirements

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
