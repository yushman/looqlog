## MODIFIED Requirements

### Requirement: A format is chosen by threshold, not by first match
A candidate format SHALL be selected only when at least 80% of the sampled non-empty lines
parse successfully under it. When no candidate reaches the threshold, plain text SHALL be
selected. When both logfmt and the prefix scanner cross the threshold, plain text SHALL be
selected in preference to logfmt, because the plain-text path extracts the leading timestamp and
the positional level and then hands the remainder to the same logfmt parser, so no field is lost
and two are gained. Plain text SHALL be reported as a threshold match when the prefix scanner
recognises a timestamp in at least the same fraction of sampled lines, and as a fallback only
when it does not.

#### Scenario: Mostly JSON with some noise
- **WHEN** 95 of 100 sampled lines are JSON objects and 5 are startup banners
- **THEN** JSON Lines is selected

#### Scenario: Ambiguous input falls back
- **WHEN** half the sampled lines look like logfmt and half like free text
- **THEN** plain text is selected rather than logfmt

#### Scenario: Plain text is never rejected
- **WHEN** no candidate reaches the threshold
- **THEN** plain text is selected and detection reports fallback rather than failure

#### Scenario: Recognised prefixes are a match, not a fallback
- **WHEN** 95 of 100 sampled lines are syslog lines whose timestamps the prefix scanner
  recognises
- **THEN** plain text is selected and reported as a threshold match, not as a fallback

#### Scenario: Genuinely unstructured input still reports fallback
- **WHEN** the sampled lines carry no recognisable timestamps at all
- **THEN** plain text is selected and reported as a fallback

#### Scenario: A timestamp prefix outranks logfmt pairs
- **WHEN** the sampled lines are shaped `<ISO timestamp> <LEVEL> key=value key=value`, so that both
  logfmt and the prefix scanner cross the threshold
- **THEN** plain text is selected, and each entry carries the leading timestamp, the positional level
  and every field the logfmt payload contained

#### Scenario: Real logfmt is still logfmt
- **WHEN** the sampled lines carry their timestamp as a value (`ts=…`, `time=…`) or carry none at all
- **THEN** logfmt is selected, because no prefix is recognised in a `key=` position
