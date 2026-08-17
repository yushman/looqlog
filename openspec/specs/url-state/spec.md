# url-state Specification

## Purpose

The `url-state` capability covers encoding the view in the URL hash so it can be reproduced or
shared: a documented grammar for the time range, field filters, search query, format override and
timezone with percent-encoded values that round-trip, debounced writes that replace rather than grow
the history stack, the encoded state applied on load before the user sees an unfiltered view, a
malformed hash reported rather than ignored, and an explicit warning that the search text and field
values a link carries are themselves fragments of the log.

## Requirements
### Requirement: Hash grammar
The application SHALL encode view state in the URL hash using an explicit, documented grammar
covering the time range, active field filters, the search query, the format override and the
timezone. Values SHALL be percent-encoded so that field values containing separators round-trip
unchanged.

#### Scenario: Round trip through the hash
- **WHEN** a view with a range, two field filters and a query is encoded and then decoded
- **THEN** the resulting state equals the original

#### Scenario: Separator inside a value
- **WHEN** a field value contains a comma or an equals sign
- **THEN** it is encoded such that decoding yields the original value rather than two filters

### Requirement: The hash is written as state changes
The application SHALL update the hash when the range, filters or query change, debounced so that
typing does not produce an entry per keystroke, and SHALL replace rather than accumulate history
entries.

#### Scenario: Typing does not flood history
- **WHEN** the user types a fifteen-character query
- **THEN** the hash settles once after typing stops and the browser's back stack has not grown by
  fifteen entries

### Requirement: A hash is applied on load
On page load with a hash present, the application SHALL apply the encoded state before or
immediately after parsing, so the user sees the filtered view rather than the full one followed
by a jump.

#### Scenario: Fresh tab reproduces the view
- **WHEN** the URL of a filtered view is opened in a new tab and the same file is selected
- **THEN** the same filters, range and query are active and the view matches (PRD Flow 3 step 4)

#### Scenario: Format override from the hash
- **WHEN** the hash carries a format override
- **THEN** detection is skipped and the named format is used, as `log-parsing-core` specifies

### Requirement: A malformed hash is reported, not ignored
An unparsable or partially unknown hash SHALL produce a visible notice naming what could not be
applied, and SHALL apply the parts that were valid rather than silently discarding everything.

#### Scenario: Unknown key
- **WHEN** the hash contains a key the application does not recognise
- **THEN** the recognised keys are applied and the unknown one is reported

#### Scenario: Invalid range value
- **WHEN** the hash carries an unparsable timestamp in the range
- **THEN** the range is not applied, the rest is, and the user is told which part failed

### Requirement: The sharing caveat is stated
The UI SHALL make clear that a shared URL carries the search text and field values it encodes,
which are fragments of the log, so a link pasted outside the machine discloses them even though
the log itself never left the browser.

#### Scenario: Sharing affordance carries the warning
- **WHEN** the user copies the URL through an affordance the application provides
- **THEN** the caveat about what the hash contains is presented at that moment, not only in the
  README

