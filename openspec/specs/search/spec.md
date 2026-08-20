# search Specification

## Purpose

The `search` capability covers the search box over message text and field values: case-insensitive
substring matching with the match highlighted, regular expressions behind the `re:` prefix applied
as written, an invalid expression reported loudly instead of silently matching nothing, `field=value`
typed into the box becoming a filter chip, clearing that restores the unsearched view, and search
that keeps applying as live entries arrive.
## Requirements
### Requirement: Substring search
The search input SHALL match entries whose message text or any field value contains the query as
a case-insensitive substring, and SHALL highlight the matched text in the table.

#### Scenario: Message match
- **WHEN** the user types `connection refused`
- **THEN** entries whose message contains that text in any letter case are shown with the match
  highlighted

#### Scenario: Field value match
- **WHEN** the query matches the value of a field that is not displayed as a column
- **THEN** the entry is shown and the match is visible in its detail view

### Requirement: Regex search via prefix
A query beginning with `re:` SHALL be treated as a regular expression over the same text,
applied as written, with no implicit case-insensitivity.

#### Scenario: Regex query
- **WHEN** the user types `re:^ERROR.*timeout`
- **THEN** only entries matching that expression are shown

#### Scenario: Literal text that looks like a prefix
- **WHEN** the user searches for text that itself starts with `re:` as content
- **THEN** the documented escape for a literal query is available and behaves as a substring
  search

### Requirement: Invalid regex fails loudly
An uncompilable regular expression SHALL produce an inline error naming the problem, SHALL leave
the previous result visible, and SHALL NOT be presented as a search that matched nothing.

#### Scenario: Unbalanced bracket
- **WHEN** the user types `re:[unclosed`
- **THEN** an inline error is shown, the previous results remain, and the table does not go empty

#### Scenario: Error clears on correction
- **WHEN** the expression becomes valid again
- **THEN** the error disappears and results update

### Requirement: `field=value` in the search box becomes a filter
The search input SHALL recognise a `field=value` token for a known field and turn it into a
filter chip rather than searching for the literal text, so that typing and clicking produce the
same state (PRD US-4).

#### Scenario: Typed field filter
- **WHEN** the user types `service=api` in the search box
- **THEN** a `service=api` chip appears and the remaining text, if any, is used as full-text
  search

#### Scenario: Unknown field stays text
- **WHEN** the typed token names a field that does not exist in the dataset
- **THEN** it is treated as ordinary search text rather than silently producing an empty result

### Requirement: Clearing search
Pressing Escape in the search input SHALL clear the query and restore the results that the other
active filters produce, without clearing those filters.

#### Scenario: Escape clears only the query
- **WHEN** a level chip and a search query are both active and the user presses Escape
- **THEN** the query is cleared, the chip remains active, and results update accordingly

### Requirement: Search applies to live entries
Entries arriving from a live stream SHALL be matched against the active query as they arrive, on
the same terms as static entries.

#### Scenario: Search during a stream
- **WHEN** a query is active and new lines arrive
- **THEN** only matching lines appear, highlighted, without the view being rebuilt

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

