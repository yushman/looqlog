## Why

Entries exist and render as a provisional dump that cannot survive a real log: 50,000 rows in
the DOM, no time axis, no way to narrow to the minute where the incident happened. The two
surfaces that make looq a viewer rather than a parser — a timeline you can drag a range on and a
table you can scroll through without the tab dying — do not exist yet.

Days 15, 16 and 17 of `docs/mvp-plan.md` are one change because they share a data model. The
histogram counts entries per time bucket, the table renders a window of the same entries, and
dragging on the histogram narrows what the table shows. Built separately, that shared model gets
invented twice and reconciled afterwards.

## What Changes

- A time-ordered index over entries, built incrementally, so bucketing and range queries do not
  scan the whole dataset. This is the `Index` half of TDR's M2 that `log-parsing-core`
  deliberately left out until something needed it.
- Timeline: a count-per-bucket histogram rendered with `uPlot`, with bucket width chosen from a
  fixed ladder so the axis stays readable at any zoom.
- Drag-select on the timeline sets a time range; the range is adjustable, clearable, and drives
  the table.
- Explicit handling of entries with no usable timestamp: counted, visible, and excluded from
  time-range results in a way the user can see rather than silently missing.
- Outlier handling so a single entry at epoch zero cannot compress every real entry into one
  pixel.
- Virtual-scrolled table with timestamp, level and message columns, rendering only the visible
  window regardless of dataset size.
- Row selection with a detail view showing the entry's full message and all extracted fields.
- Both surfaces tolerate a dataset that grows at the end and is evicted at the front, as live
  tail produces.
- Timestamp display honouring the timezone policy the parser applied, so the axis and the rows
  agree with each other and with the log.

Not in this change: field filter chips, full-text search, URL hash state (all
`filtering-and-search`), themes and styling polish (`release-hardening`). The range filter built
here is the substrate those filters combine with.

## Capabilities

### New Capabilities

- `entry-index`: the time-ordered index over entries and the range queries it serves, including
  its behaviour under append and front-eviction.
- `timeline`: bucketing, histogram rendering, range selection, and how entries that cannot be
  placed on a time axis are represented.
- `entry-table`: virtual scrolling, columns, selection and the detail view.

### Modified Capabilities

- `app-shell`: the provisional entry rendering from `browser-app-shell` is replaced by the
  virtual table, and the shell now owns the active time range as part of its state.

### Ordering note

Assumes `browser-app-shell` is archived first, and that `live-tail` has landed or is understood,
since front-eviction is a requirement here rather than an afterthought.

## Impact

- `web/`: timeline and table components, the shared index and view model, `uPlot` integration.
- Dependency added: `uPlot` (~40KB), counted against the <200KB gzipped bundle budget in TDR §5.
- The index lives on the JS side rather than in `looq-core`, since it is a view concern that
  changes with filters; if profiling says otherwise, moving it into the core crate is a
  contained change.
- Performance targets touched: 50k-line scroll smoothness, and the <50ms filter latency on 10k
  lines from TDR §11 that the range filter is the first real exercise of.
- Constrains later changes: filters and search narrow the same index the timeline buckets from,
  so their combination is an intersection over one structure rather than three parallel passes.
