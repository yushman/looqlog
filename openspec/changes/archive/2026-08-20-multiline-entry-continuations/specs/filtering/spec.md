## ADDED Requirements

### Requirement: A continuation chain filters as one unit
The active predicate SHALL be evaluated against a chain's root entry, and the whole chain
SHALL be shown or hidden together. A chain member SHALL NOT be shown without its root, and a
root SHALL NOT be shown with its members omitted, because a stack frame without the
exception that produced it is not readable as a filter result.

#### Scenario: A level filter shows the whole trace
- **WHEN** `level=ERROR` is active and an `ERROR` entry has sixty stack frames beneath it,
  none of which extracted a level of their own
- **THEN** the root and all sixty frames are shown

#### Scenario: A field on the root selects the chain
- **WHEN** a `tag` chip matching only the chain root is active
- **THEN** the root and all its members are shown

#### Scenario: A chain the root does not match is hidden entirely
- **WHEN** a filter excludes the chain root
- **THEN** none of its members are shown, even if a member's own extracted fields would have
  matched

#### Scenario: An orphaned member behaves as a standalone entry
- **WHEN** live-tail eviction has removed a chain root while its members are still retained
- **THEN** each remaining member is filtered on its own values and rendered as an ordinary
  standalone row, rather than being dropped or causing a failed lookup
