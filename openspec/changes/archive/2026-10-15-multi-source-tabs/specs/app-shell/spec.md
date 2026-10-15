## MODIFIED Requirements

### Requirement: Empty and unopened states are explicit
The UI SHALL present the landing page at `/` as the unopened state, a distinct state for a source
that produced zero entries, and a distinct closed-source state, rather than showing an empty
results area in any of these cases. A newly opened tab at `/` SHALL start on the landing and SHALL
NOT inherit a source, filters or a stream from other tabs.

#### Scenario: Fresh tab
- **WHEN** a new tab opens `/` while other tabs show logs
- **THEN** it shows the landing with the source list and an upload target, and no log

#### Scenario: Source produced no entries
- **WHEN** an empty file is opened
- **THEN** the UI says the file contained no log lines, distinct from the landing

#### Scenario: Source is gone
- **WHEN** a tab's source is unknown, closed or unloaded
- **THEN** the UI says which, distinct from the landing and from an empty file
