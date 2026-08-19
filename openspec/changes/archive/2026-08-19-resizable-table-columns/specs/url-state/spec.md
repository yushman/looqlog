## MODIFIED Requirements

### Requirement: Hash grammar
The application SHALL encode view state in the URL hash using an explicit, documented grammar
covering the time range, active field filters, the search query, the format override, the
timezone and the entry table's column widths. Values SHALL be percent-encoded so that field
values containing separators round-trip unchanged. Column widths SHALL be encoded only for the
directly resizable columns; a width that is derived from the others SHALL NOT appear in the
grammar.

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

## ADDED Requirements

### Requirement: A bad column width never blocks the log
A column width that is malformed, out of range, or names a column that does not exist SHALL be
replaced by that column's default and SHALL be reported alongside the other unapplied parts of
the hash. It SHALL NOT prevent the rest of the hash from applying, and SHALL NOT prevent the log
from loading — a width is a presentation preference, not data.

#### Scenario: Malformed width falls back to the default
- **WHEN** a hash carries a column width that is not a number
- **THEN** that column uses its default width, the log loads, and the problem is reported

#### Scenario: Out-of-range width is clamped, not rejected
- **WHEN** a hash carries a width below a column's minimum
- **THEN** the column renders at its minimum rather than at an unusable width
