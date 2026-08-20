# entry-continuations Specification

## Purpose
TBD - created by archiving change multiline-entry-continuations. Update Purpose after archive.
## Requirements
### Requirement: Continuation lines are linked, not merged
A line recognised as the continuation of the entry above it SHALL still produce its own
`Entry`, carrying a link to the **root** of the chain it belongs to rather than to its
immediate predecessor. A line that starts its own entry SHALL carry no link. The parser
SHALL NOT merge, buffer or withhold any entry in order to decide this: the recognition
SHALL depend only on the current line and remembered state about lines already consumed,
so that feeding one line at a time (stdin mode, ADR-0004) returns the same entries at the
same moment as it does today.

#### Scenario: A frame links to the chain root
- **WHEN** a prefixed line is followed by three stack frames
- **THEN** four entries are produced, the first with no link and each of the other three
  linked to the first entry's ordinal

#### Scenario: Line-at-a-time feeding is unchanged
- **WHEN** the same input is fed one line per call rather than as one chunk
- **THEN** each call returns its entry immediately, and the entries and their links match
  whole-input parsing exactly

#### Scenario: No entry waits for a line that has not arrived
- **WHEN** a line that could be the root of a chain is fed and no further input follows
- **THEN** that entry is returned by the same call that fed it, without requiring a flush,
  a timeout, or an end-of-input signal

#### Scenario: Line accounting is unaffected
- **WHEN** an input containing chains is parsed to completion
- **THEN** entries produced plus lines skipped plus blank lines still equals the total line
  count

### Requirement: A prefix-less stack frame continues the entry above it
A line with no recognised timestamp prefix SHALL be treated as a continuation when it
matches one of the explicit frame markers — `at `, `Caused by:`, `... N more`,
`Suppressed:`, `Traceback (most recent call last):`, a Python `File "…", line N` frame, or
an exception header whose leading identifier's last dotted segment ends in `Exception`,
`Error` or `Throwable` — allowing for leading whitespace. Absence of a prefix alone SHALL
NOT make a line a continuation.

#### Scenario: Java trace written to stdout
- **WHEN** a timestamped `ERROR` line is followed by `java.lang.NullPointerException` and
  several `	at com.foo.Bar(Bar.java:12)` lines
- **THEN** every line after the first is linked to the first entry's ordinal

#### Scenario: The exception header links, so the frames below it can
- **WHEN** the line directly beneath a timestamped entry is a bare
  `java.lang.NullPointerException: …` header with no `at ` marker
- **THEN** the header is linked to the timestamped entry, and the frames beneath it link to
  the same root rather than being orphaned by the header breaking the chain

#### Scenario: Prose is not mistaken for an exception header
- **WHEN** a timestamped entry is followed by an unprefixed line of ordinary prose whose
  first word is not an identifier ending in `Exception`, `Error` or `Throwable`
- **THEN** it starts its own unlinked entry

#### Scenario: Python traceback
- **WHEN** a timestamped line is followed by `Traceback (most recent call last):`, a
  `  File "app.py", line 12, in handler` line and the frame body
- **THEN** those lines are linked to the timestamped entry

#### Scenario: Nested cause
- **WHEN** a chain contains `Caused by: java.io.IOException` followed by further frames and
  a `... 14 more` line
- **THEN** all of them link to the original chain root, not to the `Caused by:` line

#### Scenario: An unprefixed line that is not a frame starts its own entry
- **WHEN** a timestamped line is followed by an indented line of ordinary prose that
  matches no frame marker
- **THEN** that line becomes an unlinked entry of its own

### Requirement: A repeated logcat prefix continues the entry above it
A logcat record SHALL be treated as a continuation when its `pid`, `tid`, level and `tag`
all equal the chain root's **and** its message carries a continuation signal — leading
whitespace, a frame marker, or an unclosed brace left open by the root. The timestamp SHALL
NOT participate in the identity comparison.

#### Scenario: Trace frames share the root's identity
- **WHEN** a logcat `E App/WebSocketClientImpl` line reporting an exception is followed by
  frames carrying the same pid, tid, level and tag with tab-indented `at …` messages
- **THEN** each frame is linked to the exception line's ordinal

#### Scenario: A frame whose millisecond drifted still continues the chain
- **WHEN** a `W System.err` root at `13:07:51.983` is followed by a frame at `13:07:51.984`
  with the same pid, tid, level and tag
- **THEN** the frame is linked to the root

#### Scenario: Identical identity without a continuation signal starts a new entry
- **WHEN** several consecutive logcat lines share pid, tid, level and tag but each carries
  an ordinary, unindented message
- **THEN** each is an unlinked entry of its own

#### Scenario: A different tag breaks the chain
- **WHEN** a frame-shaped logcat message arrives under a different tag from the open chain's
  root
- **THEN** it starts its own entry and the previous chain is closed

### Requirement: An unclosed brace continues the entry above it
When a chain root's message ends with a positive `{` nesting depth, subsequent lines SHALL
be treated as continuations while that depth remains positive, and the chain SHALL close
when the depth returns to zero. Only `{` and `}` SHALL be counted; `[` and `]` SHALL NOT,
because ANSI escape sequences embedded in log messages make bracket counting unreliable.

#### Scenario: Pretty-printed JSON payload
- **WHEN** a prefixed line ending in `response body = {` is followed by indented JSON lines
  and a closing `}`
- **THEN** all lines through the closing brace are linked to the first entry, and the next
  line starts a new entry

#### Scenario: ANSI escapes do not open a chain
- **WHEN** a message contains an ANSI escape sequence such as `ESC[7m` and no unclosed `{`
- **THEN** no chain is opened

#### Scenario: A payload chain is grouped but not field-extracted
- **WHEN** a multi-line JSON payload is recognised as a chain
- **THEN** the root entry's fields are exactly what single-line parsing would have produced,
  and no keys from the continuation lines appear as fields

### Requirement: A chain requires a prefixed root
A chain SHALL only be opened beneath an entry whose source line carried a recognised
timestamp prefix, and a blank line SHALL close any open chain. This prevents a run of
unprefixed text — a thread dump, a `dumpsys` section, `/proc` output — from collapsing into
a single entry.

#### Scenario: Bare frames with no log entry above them stay separate
- **WHEN** a thread-dump section contains `at java.lang.Object.wait(…)` lines preceded only
  by `| held mutexes=` and `native: #03 pc …` lines, none of which carry a timestamp
- **THEN** every one of those lines is an unlinked entry of its own

#### Scenario: A blank line closes the chain
- **WHEN** a chain is open and a blank line arrives, followed by another frame-shaped line
- **THEN** the line after the blank starts its own entry rather than extending the chain

#### Scenario: A long run of unprefixed dump text produces no chain
- **WHEN** a section of several thousand consecutive unprefixed, non-blank lines is parsed
- **THEN** no entry in that section is linked to any other

### Requirement: Chain length is capped and truncation is reported
A chain SHALL be limited to a fixed maximum number of continuation lines. When the limit is
reached the chain SHALL close and the next line SHALL start a fresh unlinked entry, and the
truncation SHALL be recorded as a diagnostic naming the chain root's line number. Truncating
a chain silently is a defect: a chain that quietly stops is indistinguishable from a trace
that was genuinely short.

#### Scenario: Over-long chain is closed and reported
- **WHEN** an input contains more consecutive continuation lines than the cap allows
- **THEN** the entries beyond the cap are unlinked, and a diagnostic identifying the
  truncated chain's root line is available from the parser

#### Scenario: A chain at exactly the cap is not reported
- **WHEN** a chain contains exactly the maximum number of continuation lines
- **THEN** all of them are linked and no truncation diagnostic is recorded

### Requirement: Structured formats never chain
Continuation recognition SHALL apply only to the plain-text format. Under JSON Lines and
logfmt every line is a complete record or a malformed one, and a malformed line SHALL
continue to be skipped with a diagnostic rather than absorbed into the entry above it.

#### Scenario: A truncated JSON record does not swallow the next one
- **WHEN** a JSON Lines input contains a line that is cut off mid-object, followed by a
  valid record
- **THEN** the cut-off line is reported as malformed and the valid record becomes its own
  unlinked entry

#### Scenario: Forcing plain text on a JSON file enables chaining
- **WHEN** the same input is parsed with the plain-text format forced
- **THEN** continuation recognition applies, because the active format — not the file's
  content — decides

