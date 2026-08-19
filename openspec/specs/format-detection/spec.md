# format-detection Specification

## Purpose
The `format-detection` capability covers how `looq-core` chooses which of the
three MVP formats (JSON Lines, logfmt, plain text) parses a given input, by
sampling the first non-empty lines and requiring a candidate to cross a threshold
before it wins — rather than the first line deciding the whole file's format. It
also covers reporting that choice and its confidence back to the caller, and the
explicit override that lets a caller (eventually the `#format=` URL hash) skip
detection entirely.
## Requirements
### Requirement: Detection samples the head of the input
Format detection SHALL examine at most the first 100 non-empty lines and SHALL evaluate
candidate formats in the TDR §8 priority order: JSON Lines, then logfmt, then plain text.
Detection SHALL NOT require the whole input, so it can run on the first chunk of a large file
and on the opening lines of a live stdin stream.

#### Scenario: Detection on a partial read
- **WHEN** only the first 64KB of a 100MB file has been read
- **THEN** a format is already decided from that prefix

#### Scenario: Short input
- **WHEN** the input has three lines
- **THEN** detection uses those three lines rather than waiting for 100

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

### Requirement: The detection result is reported, not hidden
Detection SHALL return the chosen format, the fraction of sampled lines that parsed under it,
and whether it was a threshold match or the plain-text fallback, so a caller can display the
choice and a user can notice a wrong one (CLAUDE.md silent-failure list).

#### Scenario: Result carries its evidence
- **WHEN** detection selects logfmt from a sample of 100 lines with 88 parsing
- **THEN** the result reports logfmt, a 0.88 match fraction, and that a threshold was met

#### Scenario: Low-confidence choice is distinguishable
- **WHEN** detection selects a format at exactly the threshold
- **THEN** the reported fraction lets the caller warn the user rather than presenting the
  choice as certain

### Requirement: Explicit override wins over detection
The parser SHALL accept an explicit format from the caller, and when one is supplied SHALL
use it without running detection, even when the input parses poorly under it. This is the
parameter the `#format=` URL hash will later set.

#### Scenario: Forced format is honoured
- **WHEN** the caller forces logfmt on input that detection would classify as JSON
- **THEN** the input is parsed as logfmt

#### Scenario: Forced format that fits badly stays visible
- **WHEN** a forced format causes most lines to be skipped
- **THEN** the diagnostics report the skipped lines, so the mismatch is observable rather
  than looking like an empty log

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

