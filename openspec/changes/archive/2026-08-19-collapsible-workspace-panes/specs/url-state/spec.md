## MODIFIED Requirements

### Requirement: Hash grammar
The application SHALL encode view state in the URL hash using an explicit, documented grammar
covering the time range, active field filters, the search query, the format override, the
timezone, the entry table's column widths and which workspace panes are collapsed. Values SHALL
be percent-encoded so that field values containing separators round-trip unchanged. Column widths
SHALL be encoded only for the directly resizable columns; a width that is derived from the others
SHALL NOT appear in the grammar. The collapsed-pane key SHALL name the panes that are collapsed,
so that the default state of everything expanded is absent from the hash entirely.

#### Scenario: Round trip through the hash
- **WHEN** a view with a range, two field filters and a query is encoded and then decoded
- **THEN** the resulting state equals the original

#### Scenario: Separator inside a value
- **WHEN** a field value contains a comma or an equals sign
- **THEN** it is encoded such that decoding yields the original value rather than two filters

#### Scenario: Column widths round-trip
- **WHEN** the user resizes two columns and the resulting link is opened again
- **THEN** the table renders with those widths

#### Scenario: Absent widths mean defaults
- **WHEN** a hash carries no column widths
- **THEN** the table uses its default widths

#### Scenario: Collapsed panes round-trip
- **WHEN** the user collapses the detail pane and the resulting link is opened again
- **THEN** the detail pane is collapsed

#### Scenario: Nothing collapsed means no key
- **WHEN** both panes are expanded
- **THEN** the hash carries no collapsed-pane key

## ADDED Requirements

### Requirement: An unknown pane name never blocks the log
A collapsed-pane value naming a pane that does not exist, or malformed in any other way, SHALL be
ignored for that entry and reported alongside the other unapplied parts of the hash. The remaining
pane names in the same value SHALL still apply, and the log SHALL still load — a collapsed pane is
a presentation preference, not data.

#### Scenario: An unknown name is dropped, the rest applies
- **WHEN** a hash names one real pane and one that does not exist
- **THEN** the real pane is collapsed, the log loads, and the unknown name is reported

#### Scenario: A malformed value does not stop loading
- **WHEN** a hash carries a collapsed-pane value that cannot be parsed at all
- **THEN** both panes render expanded, the log loads, and the problem is reported
