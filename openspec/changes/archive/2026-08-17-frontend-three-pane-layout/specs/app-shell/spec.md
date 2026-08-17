## ADDED Requirements

### Requirement: Three-pane workspace layout
The application SHALL arrange its surfaces as a workspace bound to the viewport: a full-width
timeline across the top, a filter rail on one side, the entry table filling the remaining width, and
a detail pane for the selected entry. The panes SHALL scroll independently and the document itself
SHALL NOT scroll, so that log entries are visible when the view first renders rather than below a
screenful of other surfaces. The same layout SHALL serve both file mode and live-stream mode, defined
once rather than per mode.

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

### Requirement: Secondary surfaces are collapsible without hiding warnings
Surfaces other than the log itself SHALL be allowed to start collapsed — loading another file,
format detection, parser diagnostics, the privacy note and the shareable-link action — and a
collapsed surface SHALL state its status on the part that stays visible. A collapsed surface SHALL
open by itself when it has something the user must not miss, so that collapsing is a way to save
space and never a way to hide a warning.

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

## MODIFIED Requirements

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
