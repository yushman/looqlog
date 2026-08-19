## Context

`looq-core` parses three formats. JSON and logfmt are structured and work well; plain
text is the fallback and is where everything else ends up. Plain text today does exactly
two things: `timestamp::match_leading_timestamp` tries one hand-rolled pattern
(`\d{4}-\d{2}-\d{2}[T ]\d{2}:\d{2}:\d{2}(\.\d+)?(Z|[+-]\d{2}:?\d{2})?`) anchored at offset
0, and `level::scan_message` looks for a level word anywhere in the line. Anything else
becomes an entry with no timestamp, no level and no fields.

Two hard constraints shape every decision below:

- **No `regex`.** `log-parsing-core` measured the crate at ~870KB added to `core.wasm`
  (1,046,290 B → 158,016 B after removing it) against the ~300KB TDR §5 budget. The file
  is 194,350 B today. Every scanner here is hand-rolled byte matching, as
  `timestamp::match_leading_timestamp` already is.
- **Per-line cost.** Plain text is already the slowest of the three formats in
  `benches/parse.rs` precisely because it attempts a timestamp scan on every line. This
  change multiplies that attempt by several patterns and several offsets, so the naive
  implementation is a guaranteed regression against TDR §11's <200 ms/MB (currently
  ~80 ms/MB).

## Goals / Non-Goals

**Goals:**
- A line whose prefix carries a timestamp gets one, across the timestamp shapes real
  logging stacks emit — not just ISO 8601.
- A line whose payload is structured contributes filterable fields, whatever precedes it.
- Every inference the parser makes (year, timezone, prefix/payload disagreement) is
  reported, never silent.
- No new runtime dependency, no measurable throughput regression.

**Non-Goals:**
- Named format grammars as `Format` enum variants. Apache combined stays "plain text with
  a recognised timestamp"; its `status`/`method`/`path` are not broken out into fields.
- User-supplied patterns (grok-lite) and a format selector in the UI.
- Joining multi-line payloads (stack traces) into one entry — still one line, one entry.
- Named IANA timezones. Unchanged from `log-parsing-core`: needs `chrono-tz`'s embedded
  database, same budget problem as `regex`.

## Decisions

### D1: Widen the plain parser, do not add `Format` variants

`Format` stays `{Json, Logfmt, Plain}`. Syslog, klog, Apache and friends are recognised
*within* plain text rather than becoming their own enum members and their own detection
candidates.

Alternative considered: the TDR §8 P1 list as four new variants. Rejected on two counts.
Detection is an ordered chain with an 80% threshold, and going from 3 to 7 candidates
makes mis-detection both likelier and harder to reason about — syslog 5424's structured
data (`[exampleSDID@32473 iut="3"]`) reads as logfmt pairs, and Docker's lines *are* valid
JSON, so `Format::Docker` could never win against `Format::Json` sitting above it in the
chain. And a named variant only helps the format it names, while the same scanner work
spent inside plain text helps every log that merely resembles one of them.

Consequence to accept: `#format=` gains no new values, and someone whose log is genuinely
Apache combined gets a timeline and a message but not `status` as a filter chip.

### D2: A bounded head window, not offset 0 and not "anywhere"

The timestamp is searched for at token starts within the first **64 bytes** of the line,
where a token start is offset 0 or a position preceded by whitespace, `[`, `(` or `<`.
First full match wins.

Offset 0 alone cannot see `1.2.3.4 - - [08/Aug/2026:17:42:01 +0000]`, which is the entire
Apache/nginx access-log family. Scanning anywhere in the line is what design.md D9 of
`log-parsing-core` declined to guess at: a bare number deep in a message gets misread as a
timestamp, and the cost is unbounded in line length. 64 bytes covers the observed
prefixes (an IPv6 address plus `- -` is comfortably inside it) while keeping the work per
line constant regardless of how long the line is. Requiring the *whole* pattern to match,
not a prefix of it, is what keeps false positives low.

### D3: Sticky scanner selection

Detection already samples up to 100 non-empty lines. That sample also decides which
timestamp shape and which head offset this input uses. Afterwards each line tries the
sticky choice first and only falls back to the full sweep when it misses; a configurable
number of consecutive misses re-runs selection.

Without this, the per-line cost is (number of patterns × number of candidate offsets), and
the pattern that wins is usually the last one tried, since ISO is checked first and
non-ISO logs are the whole point of the change. With it, the common case is one scanner
attempt at one offset — roughly what plain text costs today.

This is the decision most likely to be wrong in a way a benchmark catches, so task 6
measures both the sticky and the pathological (alternating shapes) case.

### D4: Inferred year is a first-class reported fact

Syslog 3164 and klog carry no year. The parser infers one — the current year, stepped back
by one if that would place the entry in the future relative to the caller-supplied "now" —
and sets `Entry::timestamp_year_inferred`, mirroring the existing
`timestamp_used_default_tz` flag all the way through the WASM DTO to the UI.

Alternatives: refusing to parse year-less timestamps (throws away all of syslog and all of
k8s klog, i.e. most of the point of this change) and inferring silently (a wrong point on
the timeline that the user cannot detect, which is exactly the shape of failure CLAUDE.md's
silent-failure list exists to prevent).

The December→January boundary is why the rule is "step back if it lands in the future"
rather than "always the current year". A log from more than a year ago still dates wrong;
that is a known limitation, reported via the flag rather than hidden. `looq-core` stays
target-agnostic, so "now" is a caller-supplied parameter, not a `SystemTime` call inside
the crate.

### D5: Level by position first, whole-line scan second

After the timestamp is consumed, the next token is tested as a level: bare (`INFO`),
bracketed (`[INFO]`, `<INFO>`), suffixed (`INFO:`), a syslog priority (`<130>` → severity
via the RFC 5424 table), or a single letter in klog/logcat style (`I`, `W`, `E`, `D`, `V`,
`F`). Only if that fails does the existing whole-message scan run.

This is a superset of today's behavior *except* where both find something and disagree,
which is the point: `2026-08-08T17:42:01Z INFO retrying after ERROR response` currently
reports ERROR. Keeping the scan as a fallback preserves levels on lines with no timestamp
prefix at all (`app: ERROR something`), which is why the scan is not simply deleted.

Syslog's 8 severities collapse into the existing 6-level table: `emerg`/`alert`/`crit` →
FATAL, `err` → ERROR, `warning` → WARN, `notice`/`info` → INFO, `debug` → DEBUG. Widening
`Level` to 8 variants would touch filtering, the timeline's colour mapping and the URL
grammar for two severities that no other format produces.

Single letters are deliberately only accepted in the positional slot, never by the
whole-line scan — a bare `E` elsewhere in a sentence means nothing.

### D6: Payload dispatch reuses the existing parsers

Whatever follows the prefix is offered to a parser when it looks structured: text starting
with `{` goes to `json::parse_line`; text with at least two `key=value` pairs goes to
`logfmt::parse_line`. Otherwise it stays the message. One level only — a payload's own
payload is not unwrapped.

The threshold of two pairs (rather than one) is a judgement call, for the same reason
detection uses 80% rather than "any match": English prose contains `foo=bar` far more often
than it contains two of them, and a false positive here silently turns message text into
filter chips.

A payload that fails to parse is not a malformed line. The prefix already produced a
usable entry; the payload simply stays message text. Reporting it as malformed would let a
plain-text file emit diagnostics, which the log-parsing spec forbids.

### D7: Payload wins conflicts; the prefix timestamp survives as a field

```
2026-08-08 17:42:01 INFO {"ts":"2026-08-08T17:41:59Z","level":"error","msg":"boom"}
        ↑ prefix                        ↑ payload — disagrees on both
```

Timestamp, level and message come from the payload when it supplies them. The prefix is
typically stamped by a collector, shell redirect or container runtime *around* the
application's own line, so the payload is the more authoritative record of what the
application said and when it said it.

The prefix timestamp is not discarded: it is kept as a field (`prefix_ts`) whenever it
differs from the payload's, so the disagreement is inspectable and filterable rather than
being quietly resolved. Where the payload supplies nothing, the prefix value stands.

### D8: Docker's wrapper is unwrapped in the JSON parser

A JSON object whose members are exactly `log`, `stream` and `time` is a Docker
json-file-driver record. The `log` member is re-parsed as a line in its own right (prefix
scan and payload dispatch included), `time` supplies the timestamp when the inner line has
none, and `stream` is kept as a field.

The exact-member-set test is what keeps this from firing on an application log that
happens to have a `log` key. The unwrap lives in `json.rs` rather than becoming its own
format for the reason given in D1: Docker lines are valid JSON, so any Docker candidate
placed below JSON in the detection chain could never win, and one placed above it would
have to be tried against every JSON log in existence.

k8s CRI's other shape (`2026-08-08T17:42:01Z stdout F <line>`) needs no special case — it
is a prefixed plain-text line and D2 plus D6 already handle it.

### D9: Prefixed plain text is a match, not a fallback

Plain text whose prefix scanner found a timestamp in at least the threshold fraction of
sampled lines is reported as `Threshold`, the same outcome a JSON or logfmt match produces;
only "nothing matched" stays `Fallback`. Implementation note: this reuses the existing
`DetectionOutcome` variants rather than adding a third. An earlier draft of this decision
called for a new variant, but the outcome the spec scenarios require is exactly "reported as
a threshold match", and reusing `Threshold` keeps the WASM DTO's string mapping and the UI's
`outcome === "fallback"` check working unchanged. The extra information detection now
carries — which timestamp shape and head offset won — travels as its own fields.

The UI currently renders a fallback as a warning — *"Format fell back to plain
text — no format matched at least 80% of the sampled lines"* — which after this change
would fire on input that parsed perfectly well.

## Risks / Trade-offs

- **Per-line throughput regression** → D3's sticky selection, plus the D2 window being a
  fixed byte count rather than a fraction of the line. Gated by `cargo bench -p looq-core`
  against the current ~80 ms/MB before the change is considered done, with the pathological
  alternating-shape input measured too, not just the happy path.
- **False-positive timestamps inside the head window** → the whole pattern must match, and
  only token starts are tried. Residual risk accepted: a line beginning with a version
  string or an IP-like number in a date-ish shape could be misread. This is visible in the
  UI (an entry lands at an absurd point on the timeline) rather than silent.
- **The level change is a behavior change on existing input** → it is called out as
  breaking in the proposal, and both READMEs describe the new precedence. Anyone relying on
  the old whole-line-first behavior for a line that also has a positional level was getting
  the wrong answer.
- **Payload dispatch turns message text into fields** → a filter chip list that used to be
  empty for plain text becomes populated, which changes the UI's shape for existing files.
  Bounded by the two-pair threshold (D6) and by the existing per-field cardinality cap.
- **Inferred years are wrong for old archives** → flagged per entry (D4) and surfaced in
  the UI. A caller-supplied override (a year, or a full timezone/epoch policy) is the
  natural next change and is deliberately not built here.
- **`core.wasm` growth** → several hundred lines of scanner code, measured in `tasks.md`
  against the 194,350 B baseline. Expected to be tens of KB against ~113KB of headroom; if
  it somehow is not, the packaging spec requires the new number and its justification to be
  recorded rather than discovered.

## Open Questions

- **A format selector in the UI.** Override is still `#format=` in the URL hash only, and
  `looq-detection.ts` can only display. This change does not add detection candidates
  (D1), so the pressure is lower than it would have been — but a user whose file is
  mis-detected still has to hand-edit a URL. Raised during exploration, left unresolved,
  deliberately not scoped here.
- **Whether an inferred year deserves more than a flag** — e.g. a caller-supplied "assume
  this year" input for old archives. Flag first, decide once it is visible in real use.
