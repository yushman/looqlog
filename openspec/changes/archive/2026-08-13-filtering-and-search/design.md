## Context

Everything needed to narrow a dataset now exists separately: a field inventory from the parser, a
time-ordered index and a range from `timeline-and-table`, and a shell that owns view state. What
does not exist is the rule that combines them, and the encoding that makes the result
reproducible.

The rule is genuinely undecided in the documents. PRD Flow 3 shows a user clicking `[ERROR]`,
then `[service=api]`, and says the view "narrows" — which is true for two different fields and
false for two values of the same field, where narrowing would mean showing nothing. That
ambiguity resolves in code the moment someone writes the chip component, so it resolves here
instead, in a spec.

## Goals / Non-Goals

**Goals:**
- One predicate, stated explicitly, that the table, the timeline and every count agree on.
- Search that fails loudly on a bad regex.
- A view a user can reproduce in another tab from a URL.
- Filtering fast enough to feel instant at 10k entries, and correct on a live stream.

**Non-Goals:**
- A query language. `field=value` and `re:` are the whole surface; boolean expressions, negation
  and comparison operators are P2/P3 in PRD §6.
- Saved views, filter history, export.
- `#pattern=` custom regex parsing from TDR §9 — the hook is reserved, not built.

## Decisions

### D1 — OR within a field, AND across fields

Selecting `level=ERROR` and `level=WARN` shows both; selecting `level=ERROR` and `service=api`
shows their intersection. This matches how every faceted filter the audience already uses behaves
(Kibana, Grafana, GitHub's own issue filters), and it is the only reading under which clicking a
second value of the same field does something useful rather than emptying the view.

The rule is stated in the UI, not only in the spec. A filter model that the user has to infer
from experiment is a filter model they will misread during an incident, when they are reading
fast and trusting the counts.

### D2 — One predicate, evaluated over the index

Range, chips and query compose into a single predicate; the table renders the entries it selects,
the timeline buckets them, the counts count them. The alternative — the timeline filtering by
time while the table filters by everything — is how a viewer ends up showing a histogram and a
table that disagree, with no way for the user to tell which one is lying.

### D3 — Search covers message text and field values

Not field names. A user searching `api` means the value, not a field called `api`; including
names would make every entry with a `service` field match a search for `service`. Case-insensitive
by default because that is what a substring search in a log means; `re:` applied as written
because a user who types a regex expects the engine to do what the pattern says.

### D4 — `field=value` typed in the search box becomes a chip

PRD US-4 asks for `field=value` syntax in search, and the chips exist independently. Two input
paths producing two different internal states is how the URL hash ends up with two encodings for
the same view. So the search input parses `field=value` for known fields into chips; an unknown
field stays literal text, because silently matching nothing is the failure this project keeps
designing out.

### D5 — Invalid regex preserves the previous result

An uncompilable pattern shows an inline error and leaves the table as it was. Emptying the table
is the tempting implementation — the predicate matches nothing, so nothing renders — and it is
indistinguishable from a valid search with no hits. The user's next move differs completely
between those two cases.

### D6 — The timeline shows filtered counts over unfiltered ones

Two series: the filtered distribution in the foreground, the whole dataset behind it. A filtered
histogram alone loses the context that makes it useful — a spike of errors is meaningful relative
to total volume, and "filtered to nothing" and "no data at all" look identical without the
background series.

### D7 — Hash written with `replaceState`, debounced

Filters change constantly; pushing history for each would make the back button useless within
seconds of typing. Replace-in-place with a debounce keeps the URL current and the history clean.
Cost accepted: the back button does not undo a filter change. If users ask for that, it is a
contained change to a push-based model for discrete actions like chip toggles.

### D8 — The hash carries log content, and says so

The file never leaves the browser, but the hash encodes a search string and field values — real
fragments of the log — and a URL gets pasted into chat far more casually than a file gets
uploaded. PRD Flow 3 explicitly says "share the URL with a colleague". So the caveat appears at
the moment of copying, not only in the README. It costs one line of UI and closes a gap between
what the product guarantees and what a user might actually leak.

### D9 — Live entries are evaluated on arrival

Each arriving entry is tested against the active predicate as it lands, rather than re-running
the predicate over the whole dataset on each arrival. Changing a filter re-evaluates everything
retained; that is the expensive path, and it is the one the 50ms target measures.

## Risks / Trade-offs

- **Combination rule surprises someone anyway** → State it in the UI and cover both directions
  with scenarios; the alternative is that it is discovered by disagreement with a colleague's
  reading of the same screen.
- **Highlighting is expensive on long messages** → Highlight only within the truncated visible
  text; the detail view handles the full message.
- **Regex over field values on every entry of a live stream** → Compile once per query change,
  not per entry; measure at stream rates, and cap the searched text length per entry if needed.
- **Hash grows unwieldy with many filters** → Percent-encoded and compact by construction;
  measure a realistic worst case against browser URL length limits.
- **Debounce makes the URL briefly stale** → Acceptable, but the copy affordance must read
  current state rather than the last written hash.
- **`field=value` parsing steals a legitimate search** → Only known field names are parsed, and
  the documented escape produces a literal search.

## Open Questions

- Should a chip support negation (`level!=DEBUG`)? Excluding noise is at least as common as
  including signal, and adding it later means a grammar change to the hash.
- Does search match against the raw line as well as the parsed fields? Raw matching finds things
  the parser dropped; parsed matching is faster and more predictable.
- Should the hash encode the file name so a shared URL can tell the recipient which file to open?
  It is a hint, like the CLI path, but it also puts a path into a shareable string.
