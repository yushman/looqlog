## MODIFIED Requirements

### Requirement: A format is chosen by threshold, not by first match
A candidate format SHALL be selected only when at least 80% of the sampled non-empty lines
parse successfully under it. When no candidate reaches the threshold, plain text SHALL be
selected. Plain text SHALL be reported as a threshold match when the prefix scanner recognises
a timestamp in at least the same fraction of sampled lines, and as a fallback only when it does
not.

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

## ADDED Requirements

### Requirement: Detection fixes the prefix shape for the input
Detection SHALL record which timestamp shape and head offset matched the sample, and the
parser SHALL try that choice first for subsequent lines, falling back to the full set of
shapes when it misses and re-selecting after repeated misses. This SHALL NOT change which
entries are produced — only the order in which candidates are tried.

#### Scenario: Sticky choice does not change the result
- **WHEN** an input mixes two timestamp shapes so that the sticky choice misses on some lines
- **THEN** every line yields the same entry it would have yielded without a sticky choice

#### Scenario: Uniform input pays for one attempt
- **WHEN** every line of an input uses the same timestamp shape at the same offset
- **THEN** parsing does not attempt the other shapes on each line
