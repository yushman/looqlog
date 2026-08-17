## MODIFIED Requirements

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
