## ADDED Requirements

### Requirement: A logcat-majority sample is a threshold match whatever its tag widths
Detection SHALL report a sample of logcat records as a threshold match on plain text with the
logcat timestamp shape whether or not those records' tags are padded to the column width. A
boot log, whose short system tags (`vold`, `init`, `netd`) are padded far more often than an
application log's are, SHALL NOT be reported as the plain-text fallback.

#### Scenario: A padded-tag sample is not a fallback
- **WHEN** detection samples 100 lines of which 90 are logcat records whose tags are padded
- **THEN** the format is plain text, the outcome is a threshold match, and the timestamp shape
  is logcat — not the fallback the UI renders as "no format matched at least 80%"

#### Scenario: Padded and unpadded records count towards one shape
- **WHEN** a sample mixes `vold    :` records with `ActivityManager:` records
- **THEN** both count as logcat prefixes and the modal shape chosen is logcat
