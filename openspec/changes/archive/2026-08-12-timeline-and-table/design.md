## Context

Entries are parsed, typed and rendered as a provisional list. This change builds the two
surfaces the product is actually judged on, and the data structure underneath them. Every
decision here is constrained by three facts: datasets reach hundreds of thousands of entries, the
dataset can grow at one end and shrink at the other while being viewed, and the filters and
search arriving in the next change have to narrow the same structure without a second traversal.

TDR's M2 lists an `Index` alongside `Entry`; `log-parsing-core` deliberately did not build one,
because nothing needed it and its right shape depends on what queries it serves. Those queries
exist now.

## Goals / Non-Goals

**Goals:**
- Bucket counts and range queries that are cheap enough to run on every drag frame.
- A table that is indifferent to dataset size.
- Honest treatment of entries that cannot be placed on a time axis.
- Behaviour that survives live growth and front-eviction, since `live-tail` produces both.

**Non-Goals:**
- Field filters, search, URL state — `filtering-and-search` builds on the range this change
  produces.
- Visual design, themes, column resizing, sorting by column.
- Any change to parsing. If the timeline wants something the parser does not provide, that is a
  finding to record, not a place to add a second parser.

## Decisions

### D1 — The index lives in TypeScript, not in `looq-core`

It is a view structure: it changes when filters change, it must survive eviction, and it is
consulted on every drag frame from the main thread. Putting it in WASM means a boundary crossing
per query and a second copy of the entry data on the Rust side. The counter-argument is real —
Rust would be faster at building it — so the decision is written with its revisit condition: if
profiling shows index maintenance rather than rendering dominating at target sizes, move it.

### D2 — Sorted-ordinal index, not sorted entries

The index holds entry ordinals ordered by timestamp; entries themselves stay in input order.
Logs are usually near-sorted, so insertion stays close to appending, and nothing has to move the
entry data. Input order is what the table shows by default, and it is what makes an ordinal a
stable identity for selection, detail views and later URL state.

### D3 — Bucket widths from a fixed ladder

Bucket width comes from a ladder — 1s, 5s, 10s, 30s, 1m, 5m, … — picking the smallest that keeps
the bucket count under a target. Dividing the span into a fixed number of buckets instead
produces widths like 3.7 seconds, whose axis labels are unreadable and whose bucket boundaries
move whenever a new entry extends the span.

### D4 — Default span from the data's bulk, not its extremes

A single entry timestamped at the Unix epoch — from an unset field, a zero value, a clock at
boot — would otherwise compress fifty-six years into the axis and every real entry into one
pixel. So the default span comes from a robust interval over the timestamps rather than min to
max, with the excluded outliers reported and reachable by zooming out. This is a case where the
naive implementation fails silently: the chart renders, it just conveys nothing, and the user
concludes the log is empty.

### D5 — Timestampless entries are a first-class group

They are counted, displayed as a count, and excluded from any time range — with the exclusion
stated whenever a range is active. The alternatives are worse in a specific way: assigning them
the previous entry's timestamp invents data, and dropping them from the table makes a
plain-text log look empty. This is the same principle `log-parsing-core` applied when it refused
to drop them at parse time.

### D6 — Fixed row height, truncation, detail on demand

Uniform row height is what makes virtual scrolling arithmetic rather than measurement.
Variable-height rows with a measurement cache are the general solution and roughly triple the
complexity of the component; a log table's long messages are better served by truncation plus a
detail view anyway, since an eight-line wrapped row destroys the scannability that makes a table
useful during an incident.

### D7 — Range selection is shell state, not component state

The timeline produces a range; the table consumes it; both go through the shell. Filters, search
and URL state all arrive next and all need the same range. A direct timeline-to-table connection
would have to be untangled in the very next change.

### D8 — Live updates are throttled and do not move the user's view

The histogram re-buckets on an interval, not per entry. A selected range stays where the user put
it as new entries arrive — a range that follows the incoming edge is unusable for reading, which
is the only reason to select one.

## Risks / Trade-offs

- **Index maintenance dominates under a fast live stream** → Measure per-entry cost as the
  dataset grows; near-sorted insertion is the assumption, and a log that is badly out of order is
  the case to test.
- **Re-bucketing on every append is quadratic if done naively** → Buckets are maintained
  incrementally where possible and recomputed only on span or width change.
- **`uPlot` at 40KB against a 200KB gzipped budget** → Measure after integration; the budget has
  had no slack recorded since `browser-app-shell`.
- **Eviction while the user is scrolled into the evicted region** → An explicit scenario rather
  than something discovered in live use.
- **Boundary rule for range queries chosen by accident** → Half-open ranges, stated in the spec,
  so an entry cannot appear in two adjacent buckets or two adjacent selections.
- **Timezone confusion between axis and rows** → Both read the interpretation the parser applied,
  and the UI states which timezone is in use.

## Open Questions

- Does the table default to input order or timestamp order? Input order is honest about the file
  and cheap; timestamp order is what a user probably expects from a viewer with a timeline.
- Should the detail view be a panel, an inline expansion, or a modal? Affects how it behaves
  under a live stream that keeps appending.
- What is the target bucket count that drives the ladder choice — a fixed number, or one derived
  from the timeline's pixel width?
