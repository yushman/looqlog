## MODIFIED Requirements

### Requirement: Count-per-bucket histogram
The timeline SHALL render entry counts per time bucket for the entries matching the active
predicate, over a background series showing the unfiltered counts, so that what a filter excluded
stays visible instead of simply disappearing. Bucket widths SHALL continue to come from a fixed
ladder of human-readable intervals so that axis labels stay round at every zoom level.

#### Scenario: Counts match the source
- **WHEN** a fixture with known timestamps is opened with no filters active
- **THEN** each bucket's height corresponds to the number of entries whose timestamps fall in it,
  verifiable against a manual count

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
