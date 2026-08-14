## Why

The viewer can show everything and narrow by time. What an incident responder actually does —
"errors only, from the api service, containing 'connection refused'" — is not possible yet, and
the filtered view they arrive at cannot be handed to a colleague.

This change adds field filters, full-text and regex search, and the URL hash that makes a view
reproducible. Days 18, 19, 22 and the day-20 golden-path checkpoint of `docs/mvp-plan.md` are one
change because they are one predicate: chips, search text and the time range from
`timeline-and-table` all narrow the same dataset, and the rule for how they combine has to be
decided once. That rule is currently written down nowhere — PRD Flow 3 shows chips being added
one after another and never says what two values of the same field mean together.

## What Changes

- Filter chips for `level` and for extracted fields, built from the field inventory the parser
  produces, with high-cardinality fields offering typed entry instead of a value list.
- A stated combination rule: values within one field are alternatives, different fields are
  conjunctions, and the time range and search text conjoin with the rest.
- Full-text search over message text and field values, case-insensitive substring by default,
  with match highlighting in the table.
- Regex search via a `re:` prefix, with compile errors shown inline and the previous result
  preserved rather than silently emptied.
- `field=value` typed into the search box becoming a chip, so the two ways a user expresses the
  same intent produce the same thing (PRD US-4).
- Escape clearing search, per PRD Flow 4.
- The timeline showing the filtered distribution against the unfiltered one, so what a filter
  excluded is visible rather than merely gone.
- URL hash state: an explicit grammar covering range, filters, query, format override and
  timezone, written on change and applied on load, so a view can be reproduced in a fresh tab.
- A stated caveat about what a shared URL contains, since a hash carrying a search string and
  field values carries fragments of the log itself.
- Filtering and search applying to entries arriving from a live stream, not only to a static
  dataset.

Not in this change: saved views, filter history, export of filtered results, query syntax beyond
`field=value` and `re:` (PRD §6 puts those at P2/P3), and the `#pattern=` custom-regex parsing
hook from TDR §9.

## Capabilities

### New Capabilities

- `filtering`: chips, the combination rule, interaction with the time range, and behaviour on
  live data.
- `search`: substring and regex search, highlighting, error handling, and the `field=value`
  shorthand.
- `url-state`: the hash grammar, when it is written, how it is applied on load, and what happens
  to a malformed or partially unknown hash.

### Modified Capabilities

- `timeline`: the histogram now shows the filtered distribution against the unfiltered one rather
  than a single series over everything.

### Ordering note

Assumes `timeline-and-table` is archived first — the range, the index and the shell state model
this change extends all come from there.

## Impact

- `web/`: filter bar and chip components, search component, a predicate evaluated over the entry
  index, hash serialisation and parsing.
- The predicate becomes the single place that decides what is visible; the timeline, the table
  and the counts all read from it, so a disagreement between them is a bug in one structure
  rather than three.
- Performance: this is the change TDR §11's <50ms filter latency on 10k lines is really about,
  and the first where a live stream must be evaluated against an active predicate per arriving
  entry.
- Answers a question none of the documents answer — how multiple filter values combine — and
  writes it into the specs rather than leaving it to whoever implements the chip component.
