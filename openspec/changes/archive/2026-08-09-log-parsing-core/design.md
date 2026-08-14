## Context

`bootstrap-cli-and-wasm-skeleton` leaves behind a workspace, an embedded page, a `/ws` line
pipe, and one hardcoded JSON Lines counter that exists only to prove the plumbing. This change
replaces that stub with the real thing inside `looq-core`, which ADR-0005 requires to stay
target-agnostic: no `wasm-bindgen`, no `web-sys`, no `std::fs`. Everything therefore arrives as
bytes from a caller, and every design decision below is constrained by having two very
different callers — a browser reading a file in chunks, and a stdin stream arriving one line at
a time, potentially for days.

Several questions the documents left open have to be answered here because the parser is where
they become code: invalid-line behaviour (TDR §16, PRD §14 Q2), timezone handling (TDR §16),
and the encoding fallback (PRD §12).

## Goals / Non-Goals

**Goals:**
- Three MVP formats parsed correctly, with tests per format.
- Detection that can be wrong out loud rather than wrong silently.
- Structured entries good enough for the timeline, table, filters and search to be built on
  without reopening the parser.
- Bad input — malformed lines, wrong encodings, missing timestamps — producing visible,
  bounded diagnostics.
- A measured throughput number for the real parser, replacing the skeleton's stub number.

**Non-Goals:**
- Any browser wiring. `serde-wasm-bindgen`, `comlink`, the worker boundary and the page are
  `browser-app-shell`.
- Any index or sorting. Entries come back in input order; whatever the timeline needs, it
  builds.
- Syslog, Apache/nginx and Docker/k8s formats — TDR §8 puts them at P1, after MVP.
- Multi-line aggregation of stack traces (PRD §14 Q3, P2).
- Any UI for diagnostics; this change produces them, `browser-app-shell` displays them.

## Decisions

### D1 — One incremental parser, two callers

The parser takes byte chunks and returns entries completed so far, holding an incomplete
trailing line until more bytes arrive. File mode feeds it 64KB chunks; stdin mode feeds it one
line. The alternative — a whole-string `parse(text)` for files plus a `parse_line(line)` for
live tail — was rejected because it is two implementations of format detection, field
extraction and diagnostics that will disagree exactly where a bug is hardest to see: an entry
looking different in live tail than in the same log opened as a file.

Consequence: the parser holds state (active format, decoder state, field inventory, diagnostic
counters), so callers own a parser instance rather than calling a free function.

### D2 — Decoding lives in core, not in JS

Bytes cross into the core crate and UTF-8 / latin-1 decoding happens there. The alternative is
letting the browser `TextDecoder` do it and passing strings in, which is faster to write and
would leave stdin mode with a separate decoder — a second implementation of the fallback rule,
and one that behaves differently at chunk boundaries. Keeping bytes as the boundary type also
keeps the door open for the future native MCP adapter (ADR-0005) without a third decoder.

### D3 — Detection by threshold, not by first match

80% of sampled lines must parse under a candidate format before it wins; otherwise plain text
takes it. First-match detection is what produces the classic failure this project's testing
rules call out: one JSON-looking line at the top of a plain-text file switching the whole parse
to JSON and emptying the view. The threshold is a number pulled from judgement, not from data,
so detection also returns the observed fraction — if 80% turns out wrong in practice, the
evidence to change it is already being reported.

### D4 — Malformed lines: skip, count, cap

Skip the line, keep parsing, record a diagnostic with line number and reason (PRD §14 Q2,
already the stated plan). The part not yet decided anywhere: what happens when everything is
malformed. Retaining a diagnostic per bad line means a wrong `#format=` override on a large file
allocates one diagnostic per line and can outweigh the entries themselves. So individual
diagnostics are capped and exact counts per reason continue past the cap. The user sees
"1,000,000 lines skipped: invalid JSON" plus the first N examples, which is both bounded and
more useful than a million individual warnings.

Alternative considered and rejected: fail the whole parse when the malformed fraction crosses
some threshold. It converts a partially-readable log into no log at all, and the honest signal —
a huge skip count next to a small entry count — is already available.

### D5 — Timezone: explicit offsets respected, naive values UTC by default, caller can override

Answers TDR §16. The core takes a timezone parameter, defaults to UTC, and reports which
interpretation it applied. Reporting matters more than the default: a log written in local time
and read as UTC shows every timestamp shifted by hours, and nothing about the display reveals
it. The `#tz=` hash that sets this is `filtering-and-search`'s problem; the parameter exists
now so the UI has something to set.

Entries with unparsable or missing timestamps are kept with an absent timestamp and counted.
Dropping them is the worse failure: an entire plain-text log would silently render as an empty
timeline, and a file whose timestamp field is misnamed would look empty rather than
misconfigured.

### D6 — Levels: dedicated field first, fixed alias table, no default

A dedicated `level` / `lvl` / `severity` field beats scanning the message, because a message
containing the word "error" in a line with `"level":"info"` is common and mis-levelling it
poisons the level filter, which is the first thing an incident responder clicks. Aliases are a
fixed small table (`WARNING`→`WARN`, `ERR`→`ERROR`, `CRITICAL`→`FATAL`); syslog numeric
severities belong with the syslog format at P1. No default level: substituting `INFO` for an
absent one invents data and makes the level filter lie about coverage.

### D7 — Field inventory capped per field

Filters (`filtering-and-search`) need to know which fields exist and which values they take.
Tracking every distinct value is unbounded on any field carrying a request or trace id, in a
WASM heap that TDR §14 already flags as the binding constraint. So: up to N distinct values per
field with counts, then the field is flagged high-cardinality and stops accumulating. Such a
field remains usable for free-text search and for equality filters typed by hand; it just does
not offer a value list. The cap number is a judgement call and gets recorded in the devlog with
the memory measurement that justifies it.

### D8 — Nested JSON kept as text, not flattened

`{"http":{"status":500}}` yields a `http` field holding the object's JSON text. Flattening to
`http.status` is the friendlier behaviour and is where this will probably end up eventually, but
it needs decisions on separators, array indices, depth caps and collisions with literal dotted
keys — all of which are scope this change does not have, and none of which are cheaper to
undo later than to defer now. Recorded as a known limitation so nobody implements half of it
by accident.

### D9 — The benchmark from the skeleton gets re-run here

The day-4 number measured a hardcoded stub. Once real parsers, `serde_json`, `regex` and
`chrono` are in the WASM bundle, both the throughput and the bundle size change materially.
CLAUDE.md requires benchmarking `looq-core` hot paths before merging, so this change ships a
native `criterion` bench for the parse path plus a re-measured browser number, and records both
against TDR §11 with the same discipline as day 4: number, target, fixture, machine, command.

## Risks / Trade-offs

- **Detection threshold picked by judgement** → The observed fraction is reported with every
  result, so a wrong threshold is visible in real use rather than inferred.
- **`serde_json` + `regex` + `chrono` inflate `core.wasm` past the ~300KB budget in TDR §5** →
  Measure the bundle after each dependency lands; `regex` in particular is the usual offender
  and a hand-written level/timestamp scanner is the fallback if the budget breaks.
- **Stateful parser instances leak state across files** → Opening a second file in the same page
  must construct a new parser; covered by a test that parses two different-format inputs through
  fresh instances and one that asserts a reused instance is rejected or reset explicitly.
- **Per-entry allocation dominates the hot path** → Benchmark before optimising; the columnar
  layout that ADR-0002 mentions as the escape hatch is a real option but not a starting point.
- **Chunk-boundary bugs are invisible in normal use** → Every parser gets a test that feeds the
  same input in one chunk and in adversarial splits (mid-line, mid-multi-byte-character) and
  asserts identical output.
- **Live tail cannot re-detect a format** → Detection runs on the opening lines of a stream; a
  service that starts with a plain-text banner and switches to JSON gets classified by the
  banner. Mitigated by the override parameter and by reporting the match fraction; a real fix
  belongs to `live-tail` if it turns out to matter.

## Open Questions

- The exact caps: how many individual diagnostics retained, how many distinct values per field.
  Both are memory-versus-usefulness trades that should be set from a measurement, not guessed.
- Should the plain-text parser attempt timestamp extraction anywhere in the line, or only at the
  start? Anywhere is more forgiving and more likely to match a number that is not a timestamp.
- Does the field inventory need to survive a live-tail ring-buffer eviction, where a field's
  counts should arguably decrease as old lines are dropped? Cheaper to answer in `live-tail`,
  but the inventory's shape is fixed here.
