## Purpose

The `app-shell` capability covers the TypeScript/Web Components application structure,
its Vite build, and the surfaces that display parse results, detection and diagnostics
(`browser-app-shell` change).
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
available on demand. Diagnostics SHALL NOT be console-only.

#### Scenario: Malformed lines are reported on screen
- **WHEN** a fixture with one malformed line is opened
- **THEN** the UI reports one skipped line, its reason and its line number

#### Scenario: A wholesale mismatch is obvious
- **WHEN** a format override causes most lines to be skipped
- **THEN** the UI shows the large skip count next to the small entry count rather than
  presenting an almost-empty log as normal

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

