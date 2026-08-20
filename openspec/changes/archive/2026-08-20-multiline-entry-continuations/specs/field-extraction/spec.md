## MODIFIED Requirements

### Requirement: Entry shape
Every parsed line SHALL become an `Entry` carrying an optional timestamp, an optional level,
a message, the line's ordinal position in the input, the arbitrary fields extracted from
it, and an optional link to the ordinal of the entry it continues. Entries SHALL be returned
in input order; the parser SHALL NOT sort or reorder them.

#### Scenario: Out-of-order timestamps are preserved
- **WHEN** input contains a line timestamped earlier than the line before it
- **THEN** both entries are returned in input order with their own timestamps, and no
  reordering or correction happens

#### Scenario: Ordinal identifies the source line
- **WHEN** an entry is produced from the 42nd line of the input
- **THEN** its ordinal is 42, so diagnostics and the UI can point at the original line

#### Scenario: A standalone line carries no continuation link
- **WHEN** a line is parsed that does not continue the entry above it
- **THEN** its continuation link is absent, which is the case for every entry produced by
  input containing no multi-line events

#### Scenario: A continuation entry keeps its own extracted values
- **WHEN** a logcat stack frame is linked to the chain root above it
- **THEN** it still carries its own timestamp, level and columns as extracted from its own
  line; the link records grouping and does not copy values from the root
