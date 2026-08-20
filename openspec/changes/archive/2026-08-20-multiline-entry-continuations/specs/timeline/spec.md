## MODIFIED Requirements

### Requirement: Count-per-bucket histogram
The timeline SHALL render **event** counts per time bucket for the entries matching the active
predicate, over a background series showing the unfiltered counts, so that what a filter excluded
stays visible instead of simply disappearing. An entry that continues the entry above it SHALL NOT
be counted, so a multi-line event contributes one count rather than one per physical line. Bucket
widths SHALL continue to come from a fixed ladder of human-readable intervals so that axis labels
stay round at every zoom level.

#### Scenario: Counts match the source
- **WHEN** a fixture with known timestamps and no multi-line events is opened with no filters active
- **THEN** each bucket's height corresponds to the number of entries whose timestamps fall in it,
  verifiable against a manual count

#### Scenario: A stack trace is one count, not one per frame
- **WHEN** a fixture containing a single exception with sixty stack frames in one bucket is opened
- **THEN** that bucket's height is one, not sixty-one

#### Scenario: Bucket width is readable
- **WHEN** the visible span is around ten minutes
- **THEN** buckets are a round interval such as five or ten seconds, not an arbitrary fraction

#### Scenario: Filtered distribution against the whole
- **WHEN** a level filter reduces the dataset
- **THEN** the histogram shows the filtered counts against a visually subordinate series of the
  unfiltered counts

#### Scenario: Filter that matches nothing
- **WHEN** the active predicate matches no entries
- **THEN** the background series still shows the dataset's shape, so the view reads as "filtered
  to nothing" rather than "no data"
