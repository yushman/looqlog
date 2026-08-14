## 1. Predicate

- [x] 1.1 Single predicate combining time range, field filters and query, evaluated over the entry index
- [x] 1.2 Combination rule implemented: OR within a field, AND across fields, AND with range and query
- [x] 1.3 Table, timeline and every count read from the same predicate result
- [x] 1.4 Tests for both directions of the rule: two values of one field widen, two fields narrow

## 2. Filter chips

- [x] 2.1 Chips built from the field inventory, showing values with counts
- [x] 2.2 High-cardinality fields accept typed values instead of listing them
- [x] 2.3 Active filters displayed, individually removable, clearable as a set
- [x] 2.4 Result count shown against total; a predicate matching nothing reads as filtered-to-nothing, distinct from an empty file
- [x] 2.5 Combination rule stated in the UI or its help, not left to inference

## 3. Search

- [x] 3.1 Case-insensitive substring search over message text and field values
- [x] 3.2 Match highlighting within the truncated visible text, with the detail view showing full context
- [x] 3.3 `re:` prefix for regex applied as written; documented escape for a literal query starting with `re:`
- [x] 3.4 Invalid regex: inline error, previous results preserved, table not emptied
- [x] 3.5 `field=value` for a known field becomes a chip; an unknown field stays literal text
- [x] 3.6 Escape clears the query without clearing chips
- [x] 3.7 Compile the query once per change, not per entry

## 4. Timeline integration

- [x] 4.1 Filtered series in the foreground over an unfiltered background series
- [x] 4.2 Empty predicate result still shows the dataset's shape
- [x] 4.3 Range selection continues to conjoin with chips and query

## 5. URL state

- [x] 5.1 Hash grammar for range, filters, query, format override and timezone, with percent-encoding; document it
- [x] 5.2 Round-trip tests including values containing separators
- [x] 5.3 Debounced writes using `replaceState`; typing does not grow the back stack per keystroke
- [x] 5.4 Hash applied on load before the unfiltered view is shown
- [x] 5.5 Malformed or partially unknown hash: apply what is valid, report what is not
- [x] 5.6 Copy affordance that reads current state and presents the caveat about log fragments in the URL
- [x] 5.7 Measure a realistic worst-case hash length against browser limits

## 6. Live data

- [x] 6.1 Arriving entries evaluated against the active predicate on arrival
- [x] 6.2 Counter distinguishes matching entries from total received
- [x] 6.3 Changing a filter mid-stream re-evaluates retained entries without a reload
- [x] 6.4 Regex search measured at stream rates; cap searched text per entry if needed

## 7. Golden path checkpoint (mvp-plan day 20)

- [x] 7.1 PRD Flow 1, Flow 3 and Flow 4 run back to back on one real multi-thousand-line fixture
- [x] 7.2 Fix anything the combined run breaks; no fixture-specific handling anywhere in the path
- [x] 7.3 Filter latency on 10,000 entries measured against the <50ms target in TDR §11, recorded with its method
- [x] 7.4 Any missed target fixed or explicitly downgraded and documented here, not deferred

## 8. Wrap-up

- [x] 8.1 Resolve the design open questions: chip negation, raw-line matching, file name in the hash
- [x] 8.2 Both READMEs cover filters, search syntax and URL sharing including the caveat
- [x] 8.3 Devlog entries with the measured numbers
- [x] 8.4 `openspec validate filtering-and-search --strict` passes
- [x] 8.5 Archive the change
