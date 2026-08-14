## 1. Entry index

- [x] 1.1 Sorted-ordinal index by timestamp, with entries kept in input order
- [x] 1.2 Timestampless entries tracked as a separate counted group, never given a substitute time
- [x] 1.3 Incremental append with flat per-entry cost as the dataset grows; test with a badly out-of-order fixture
- [x] 1.4 Front-eviction support leaving no stale references in queries or bucket counts
- [x] 1.5 Half-open range queries returning entries in input order plus a count, with the boundary rule tested at exact-match timestamps
- [x] 1.6 Measure index maintenance cost against rendering cost at target sizes; record the numbers and the revisit condition for moving the index into `looq-core`

## 2. Timeline

- [x] 2.1 `uPlot` integration rendering a count-per-bucket histogram; measure the bundle delta against the TDR §5 budget
- [x] 2.2 Bucket width ladder (1s, 5s, 10s, 30s, 1m, …) selecting the smallest width under a target bucket count
- [x] 2.3 Default span from a robust interval over timestamps, with outliers reported and reachable by zooming out
- [x] 2.4 Verify bucket counts against a manual count from a fixture
- [x] 2.5 Drag-select producing a range, with adjust and clear
- [x] 2.6 Timestampless count displayed; a dataset with no usable timestamps says so instead of rendering an empty chart
- [x] 2.7 Throttled re-bucketing under a live stream; a selected range does not follow the incoming edge

## 3. Entry table

- [x] 3.1 Virtual scroll with uniform row height and a viewport-bounded DOM node count
- [x] 3.2 Timestamp, level and message columns; timezone stated; absent level or timestamp rendered as explicitly absent
- [x] 3.3 Long-message truncation with a continuation indicator
- [x] 3.4 Row selection and a detail view listing the full message and every extracted field, including nested JSON kept as text
- [x] 3.5 Table reflects the active range and reports shown count against total
- [x] 3.6 Append under a live stream without moving a paused user's scroll position
- [x] 3.7 Eviction while scrolled into the evicted region adjusts cleanly and indicates lost history
- [x] 3.8 Delete the provisional entry renderer from `browser-app-shell`

## 4. Shell integration

- [x] 4.1 Active time range owned by the shell and passed to timeline and table
- [x] 4.2 Three fixtures render end to end in both surfaces with no fixture-specific handling and no console errors
- [x] 4.3 Resolve the design open questions: default table ordering, detail view placement, target bucket count

## 5. Performance

- [x] 5.1 50,000-entry fixture scrolls smoothly; record the measurement method and the number
- [x] 5.2 Range-filter latency on 10,000 entries measured against the <50ms target in TDR §11
- [x] 5.3 Drag responsiveness measured as frames during a continuous drag over a large dataset
- [x] 5.4 If any target is missed, fix or explicitly downgrade and document it in this change, not later

## 6. Wrap-up

- [x] 6.1 Devlog entries with the measured numbers and their commands
- [x] 6.2 `openspec validate timeline-and-table --strict` passes
- [x] 6.3 Archive the change
