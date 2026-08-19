## Purpose

The `app-shell` capability covers the TypeScript/Web Components application structure,
its Vite build, the workspace layout the panes are arranged in, and the surfaces that
display parse results, detection and diagnostics.
## Requirements
### Requirement: The UI is built from Web Components in a typed build
The application SHALL be built with Vite from TypeScript in strict mode and composed of custom
elements; CI SHALL run `tsc --noEmit` and fail on any type error. No inline application script
SHALL remain in the served HTML.

#### Scenario: Strict type errors block the build
- **WHEN** a type error is introduced in the frontend sources
- **THEN** CI fails before any bundle is produced

#### Scenario: The skeleton script is gone
- **WHEN** the served page is inspected
- **THEN** it loads the bundle and contains no application logic of its own

### Requirement: The detected format is visible and overridable in principle
The UI SHALL display which format was detected and the match fraction that produced the
decision, and SHALL make a low-confidence or fallback detection visually distinct from a
confident one, so a wrong classification is noticed rather than silently accepted.

#### Scenario: Confident detection
- **WHEN** a JSON fixture is detected at a 1.0 match fraction
- **THEN** the UI shows the format as JSON Lines without a warning

#### Scenario: Fallback detection is flagged
- **WHEN** no candidate reaches the threshold and plain text is selected as fallback
- **THEN** the UI says the format fell back to plain text and shows the observed fractions

### Requirement: Parser diagnostics reach the user
The UI SHALL display the parser's diagnostics — skipped lines with their reasons and counts,
encoding fallbacks, and the number of entries with no usable timestamp — with retained examples
available on demand. Diagnostics SHALL NOT be console-only. The diagnostics surface MAY be collapsed
by default provided the collapsed state still reports the skip count and its severity, per the
"Secondary surfaces are collapsible without hiding warnings" requirement.

#### Scenario: Malformed lines are reported on screen
- **WHEN** a fixture with one malformed line is opened
- **THEN** the UI reports one skipped line, its reason and its line number

#### Scenario: A wholesale mismatch is obvious
- **WHEN** a format override causes most lines to be skipped
- **THEN** the UI shows the large skip count next to the small entry count rather than
  presenting an almost-empty log as normal

#### Scenario: Collapsed diagnostics still carry the count
- **WHEN** the diagnostics surface is collapsed and the parse skipped lines
- **THEN** the skipped-line count is readable in the collapsed state, and expanding it reveals the
  per-reason breakdown and examples

### Requirement: Parsed entries are displayed
The application SHALL display parsed entries through the virtual-scrolled table and the
timeline, which together replace the provisional rendering introduced by `browser-app-shell`.
The shell SHALL own the active time range alongside the parse result, detection and diagnostics,
and SHALL pass it to the components that consume it rather than letting them hold it
independently.

#### Scenario: Three formats each render
- **WHEN** a JSON, a logfmt and a plain-text fixture are opened one at a time
- **THEN** each renders in the timeline and the table with no fixture-specific handling and no
  console errors

#### Scenario: Entry count matches the source
- **WHEN** a fixture with a known line count is opened
- **THEN** displayed entries plus reported skipped lines account for every non-empty line

#### Scenario: The provisional renderer is gone
- **WHEN** the frontend sources are inspected
- **THEN** the provisional entry dump from `browser-app-shell` no longer exists

#### Scenario: Range is owned by the shell
- **WHEN** a time range is selected on the timeline
- **THEN** the table reflects it through shell state rather than through a direct component-to-
  component connection

### Requirement: Empty and unopened states are explicit
The UI SHALL present a distinct state before any file is opened, and a distinct state for a
file that produced zero entries, rather than showing an empty results area in both cases.

#### Scenario: Nothing opened yet
- **WHEN** the page has loaded and no file has been selected
- **THEN** the UI prompts for a file, including the CLI-supplied path hint when there is one

#### Scenario: File produced no entries
- **WHEN** an empty file is opened
- **THEN** the UI says the file contained no log lines, distinct from the initial prompt

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

