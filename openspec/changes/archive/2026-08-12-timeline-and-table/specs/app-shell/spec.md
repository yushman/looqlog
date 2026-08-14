## MODIFIED Requirements

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
