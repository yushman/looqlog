## MODIFIED Requirements

### Requirement: Three-pane workspace layout
The application SHALL arrange its surfaces as a workspace bound to the viewport: a full-width
timeline across the top, a filter rail on one side, the entry table filling the remaining width, and
a detail pane for the selected entry. The panes SHALL scroll independently and the document itself
SHALL NOT scroll, so that log entries are visible when the view first renders rather than below a
screenful of other surfaces. The same layout SHALL serve both file mode and live-stream mode, defined
once rather than per mode. Each side pane SHALL be collapsible, giving its width to the entry table,
and SHALL be collapsed and reopened from a control belonging to the pane itself. A collapsed pane
SHALL leave a narrow strip carrying that control and the pane's name written vertically, so that
what is hidden and how to restore it are both visible without the user having to learn a control
elsewhere.

#### Scenario: Entries are visible without scrolling the page
- **WHEN** a log is opened in a desktop-sized window
- **THEN** entry rows are visible in the initial view, and the document has no scrollbar of its own

#### Scenario: The table uses the height it is given
- **WHEN** the window is made taller
- **THEN** the table shows more rows, rather than keeping a fixed-height viewport inside a taller
  page

#### Scenario: Both modes share the layout
- **WHEN** the same window size is used to open a file and to view a live stdin stream
- **THEN** the two views present the same pane arrangement, differing only in the mode-specific
  surfaces each one adds

#### Scenario: A collapsed pane gives its width to the table
- **WHEN** the filter rail is collapsed
- **THEN** the entry table occupies the width the rail had apart from the strip, and the document
  still does not scroll

#### Scenario: A collapsed pane says what it is
- **WHEN** a pane is collapsed
- **THEN** its strip shows the pane's name as vertical text and a control that reopens it

#### Scenario: A collapsed pane can always be brought back
- **WHEN** both side panes are collapsed
- **THEN** both strips remain visible and their controls reopen the panes

#### Scenario: Only the strip is reachable by keyboard
- **WHEN** a pane is collapsed and the user tabs through the page
- **THEN** focus reaches that pane's reopen control and never lands on any other control inside
  the pane

### Requirement: Secondary surfaces are collapsible without hiding warnings
Surfaces other than the log itself SHALL be allowed to start collapsed — loading another file,
format detection, parser diagnostics, the privacy note and the shareable-link action — and a
collapsed surface SHALL state its status on the part that stays visible. A collapsed surface SHALL
open by itself when it has something the user must not miss, so that collapsing is a way to save
space and never a way to hide a warning. This SHALL hold when the surface is hidden by collapsing
the pane that contains it: the pane's own control SHALL indicate that a surface inside it needs
attention, and a condition that opens a surface by itself SHALL also expand the pane containing it.
When the pane is collapsed to a strip, the indicator SHALL appear on that strip.

#### Scenario: A clean parse stays out of the way
- **WHEN** a file parses with no skipped lines and a confident format detection
- **THEN** those surfaces are collapsed and their visible summaries say so

#### Scenario: Skipped lines are visible while collapsed
- **WHEN** a file produces skipped lines
- **THEN** the skip count and its severity are readable without expanding anything, and a severe
  skip ratio expands the diagnostics surface on its own

#### Scenario: Fallback detection is not buried
- **WHEN** format detection falls back to plain text or lands below the confidence threshold
- **THEN** that fact is visible on the collapsed surface, not only inside it

#### Scenario: A collapsed pane still signals a warning inside it
- **WHEN** the rail is collapsed and the file produces skipped lines
- **THEN** the indicator on the rail's strip shows that something inside needs attention

#### Scenario: A severe condition reopens the pane, not just the surface
- **WHEN** the rail is collapsed and a skip ratio severe enough to auto-expand the diagnostics
  surface occurs
- **THEN** the rail expands as well, so the auto-expanded surface is actually visible

## ADDED Requirements

### Requirement: Selecting an entry reveals the detail pane
Selecting a row while the detail pane is collapsed SHALL expand that pane, so that a selection
never appears to do nothing. Collapsing the detail pane SHALL NOT clear the current selection.

#### Scenario: Selection expands the pane
- **WHEN** the detail pane is collapsed and the user selects a row
- **THEN** the pane expands and shows that entry

#### Scenario: Collapsing keeps the selection
- **WHEN** an entry is selected and the detail pane is collapsed and expanded again
- **THEN** the same entry is still selected and shown

### Requirement: Collapsing works in the narrow stacked layout
Below the width at which the workspace stacks into a single column, the collapse controls SHALL
keep working by hiding the corresponding stacked block, and the collapsed state SHALL serialise
identically in both layouts so that a link shared from one applies in the other. The width at which
the layout stacks SHALL be defined in one place and consumed by the stylesheet and the script,
rather than repeated.

#### Scenario: Collapse works when panes are stacked
- **WHEN** the window is narrow enough that the panes stack and the user collapses the rail
- **THEN** the rail's block is hidden and the control still reopens it

#### Scenario: A link crosses layouts
- **WHEN** a link with a collapsed pane is opened in a window of the other layout
- **THEN** the same pane is collapsed
