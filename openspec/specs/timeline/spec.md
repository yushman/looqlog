# timeline Specification

## Purpose
TBD - created by archiving change timeline-and-table. Update Purpose after archive.
## Requirements
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

### Requirement: Outliers do not destroy the axis
The timeline SHALL keep its default view usable when a small number of entries carry absurd
timestamps, by choosing the default span from the bulk of the data rather than from its extremes,
and SHALL make the excluded outliers discoverable rather than hiding them.

#### Scenario: One epoch-zero entry
- **WHEN** a dataset of entries from today contains one entry timestamped at the Unix epoch
- **THEN** the default view still shows today's distribution, and the outlier is reported as
  outside the displayed span

#### Scenario: Outliers remain reachable
- **WHEN** the user asks to see the full span
- **THEN** the timeline zooms out to include the outliers

### Requirement: Drag selects a time range
The timeline SHALL let the user drag horizontally to select a time range, SHALL show the
selection, and SHALL let it be adjusted and cleared. The selection SHALL immediately narrow the
entries shown elsewhere in the application.

#### Scenario: Dragging narrows the table
- **WHEN** the user drags a region covering part of the histogram
- **THEN** the table shows only entries within that time window and the selected span is
  displayed

#### Scenario: Clearing restores everything
- **WHEN** the user clears the selection
- **THEN** the table shows the full dataset again

#### Scenario: Selection survives new entries
- **WHEN** a range is selected while a live stream is running
- **THEN** the range stays where the user put it and does not follow the incoming edge

### Requirement: Timestampless entries are visible, not silently dropped
The timeline SHALL display how many entries have no usable timestamp, and the application SHALL
state that a time range excludes them whenever one is active.

#### Scenario: Plain-text log with no timestamps
- **WHEN** a dataset has no usable timestamps at all
- **THEN** the timeline says so instead of rendering an empty chart that reads as "no data"

#### Scenario: Mixed dataset under an active range
- **WHEN** a range is selected in a dataset where some entries lack timestamps
- **THEN** the UI reports how many entries are excluded for having no timestamp

### Requirement: Live updates are throttled
Under a live stream the timeline SHALL update its buckets on a throttled interval and SHALL
extend its span as time passes, without re-rendering per arriving entry.

#### Scenario: Fast stream
- **WHEN** entries arrive faster than the display refresh rate
- **THEN** the histogram updates at a bounded interval and the page stays responsive

