## MODIFIED Requirements

### Requirement: Regex search via prefix
A query beginning with `re:` SHALL be treated as a regular expression over the same text,
applied as written, with no implicit case-insensitivity. The expression SHALL be compiled by the
backend with the Rust `regex` crate's syntax; the READMEs SHALL state that look-around and
backreferences are not supported.

#### Scenario: Regex query
- **WHEN** the user types `re:^ERROR.*timeout`
- **THEN** only entries matching that expression are shown

#### Scenario: Literal text that looks like a prefix
- **WHEN** the user searches for text that itself starts with `re:` as content
- **THEN** the documented escape for a literal query is available and behaves as a substring
  search

### Requirement: Invalid regex fails loudly
An uncompilable regular expression SHALL produce an inline error naming the problem, SHALL leave
the previous result visible, and SHALL NOT be presented as a search that matched nothing. This
includes an expression using a construct the Rust `regex` syntax does not support.

#### Scenario: Unbalanced bracket
- **WHEN** the user types `re:[unclosed`
- **THEN** an inline error is shown, the previous results remain, and the table does not go empty

#### Scenario: Unsupported look-around
- **WHEN** the user types `re:foo(?=bar)`
- **THEN** an inline error says look-around is not supported, and the previous results remain

#### Scenario: Error clears on correction
- **WHEN** the expression becomes valid again
- **THEN** the error disappears and results update
