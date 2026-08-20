## ADDED Requirements

### Requirement: A continuation chain renders as one collapsible group
The table SHALL render a chain as a group: the root row SHALL carry an expand/collapse
control and the number of lines in the chain, and members SHALL render indented beneath the
root when expanded and be hidden when collapsed. Chains SHALL start collapsed, so that
opening a file full of stack traces shows events rather than frames.

#### Scenario: A trace collapses to one row
- **WHEN** a file containing an exception with sixty frames is opened
- **THEN** the table shows one row for the exception, marked as covering sixty-one lines,
  and the frames are not rendered until it is expanded

#### Scenario: Expanding shows the frames in order
- **WHEN** the user activates the expand control on a chain root
- **THEN** the members appear beneath it, indented, in input order

#### Scenario: Collapse state survives scrolling
- **WHEN** an expanded chain is scrolled out of view and back
- **THEN** it is still expanded, because virtual scrolling recycles rows and must not reset
  the group state

#### Scenario: Search expands the chain it matched
- **WHEN** a search matches a member of a collapsed chain
- **THEN** that chain is shown expanded so the highlighted frame is visible

#### Scenario: An orphaned member renders as an ordinary row
- **WHEN** live-tail eviction has removed a chain root while its members are still retained
- **THEN** each remaining member renders as an ordinary standalone row with no group control
  and no indentation

#### Scenario: Row height accounting stays correct
- **WHEN** a chain is expanded or collapsed
- **THEN** virtual scrolling positions remain correct and no rows are skipped or duplicated
