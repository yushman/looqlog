## ADDED Requirements

### Requirement: A match inside a chain surfaces its root
A search result SHALL present the chain root together with its members whenever the match
falls on an entry that continues another, and SHALL highlight the matched text on the member
that matched. A matching stack frame SHALL NOT be shown detached from the exception it
belongs to.

#### Scenario: A frame matches
- **WHEN** the user searches for text that appears only in a stack frame partway down a
  trace
- **THEN** the chain root is shown with the trace intact and the matching frame highlighted

#### Scenario: The root matches
- **WHEN** the query matches the chain root's message
- **THEN** the root is shown with its members and the root's match is highlighted

#### Scenario: Several members match
- **WHEN** the query matches more than one frame in the same chain
- **THEN** the chain appears once, with every matching frame highlighted, rather than once
  per match

#### Scenario: Regex search behaves the same way
- **WHEN** a regex query matches only a chain member
- **THEN** the chain root is surfaced with the member highlighted, exactly as for substring
  search
