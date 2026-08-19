## Context

`prefix-and-payload-parsing` (archived 2026-08-19) established the shape of this work:
`Format` stays `{Json, Logfmt, Plain}`, timestamp shapes are recognised generically inside
plain text rather than as named grammars, every scanner is hand-rolled because `regex`
costs ~870KB of a ~300KB `core.wasm` budget, and a sticky shape choice keeps the per-line
cost near one attempt. All of that carries over unchanged; this change adds one shape to
that machinery and fixes two precision bugs the same measurement exposed.

The measurement, taken against the real release binary on a 13MB / 168,260-line Android
bugreport:

| | |
|---|---|
| entries parsed | 164,275 (0 skipped) |
| entries with no timestamp | 161,177 (98.1%) |
| detection | `fell back to plain text (12%)` |
| logcat lines present | 32,712 |
| distinct logcat tags | 334 |

## Goals / Non-Goals

**Goals:**
- logcat lines get a timestamp, a level, and their tag/pid/tid/uid as fields.
- A Java map dump inside a log line stops polluting the field inventory.
- A `key=value` token stops being read as the line's level.
- No new dependencies, no measurable throughput regression, no new `Format` variant.

**Non-Goals:**
- dmesg / kernel monotonic timestamps (`[ 1538269.814760]`). Turning "seconds since boot"
  into an instant requires a boot anchor that only exists in the bugreport preamble
  (`Uptime:` plus `dumpstate:`), which is metadata extraction, not line parsing. Stays in
  Known Limitations.
- Bugreport section awareness (`------ DUMPSYS … ------` switching format per section).
- Full logcat coverage of every `-v` output format Android can emit. The four layouts
  actually observed are the target; `brief`, `raw` and `long` are not.

## Decisions

### D1: The `Tag:` colon is the anchor, not the columns

A logcat line is accepted only when the *whole* shape matches: `MM-DD hh:mm:ss.mmm`, then
two or three uid/pid/tid columns, then a single letter from `VDIWEF`, then a tag ending in
`:`.

The tempting implementation is "match the timestamp, then skip numeric columns". That is
what makes it dangerous: skipping an unbounded run of number-like tokens will eventually
swallow the head of an ordinary line and read whatever follows as a level. Requiring the
trailing `Tag:` means the parser has to see the whole record before committing, so a line
that merely opens with a date and some numbers is rejected.

Columns accepted: all-digits (`1000`, `806`), a short lowercase word (`root`, `shell`,
`system`), or `u0_aNN`. Two or three of them, per the observed layouts:

```
04-18 19:21:16.151  1000   806   995 D ActivityManager: freezing …   uid pid tid   (31,297)
04-21 12:57:16.290  root   511   614 D vhdnativeservice: cmd: …      name pid tid   (1,855)
04-21 13:07:51.985   806 29149 W UsbDescriptorParser: …              pid tid          (127)
04-21 13:07:39.761 u0_a2  1906  1915 I d.process.medi: …             u0_aNN pid tid    (21)
```

Two columns are `pid tid`; three are `uid pid tid`. The distinction matters for field
naming, and is decidable from the count alone.

### D2: The severity letter is read here, not by the generic positional matcher

`prefix-and-payload-parsing` reads a level from the token immediately after the timestamp.
In logcat the letter sits *after* the columns, so the generic matcher cannot see it. The
logcat scanner consumes the letter itself as part of matching the shape and hands the level
back with the timestamp.

`V D I W E F` map through the existing letter table. `S` (silent) is not mapped: it means
"emit nothing", not a severity, and it does not appear in the measured file.

### D2a: A payload key wins over a column of the same name

When a logcat record's message is itself structured and carries a key named `tag`, `pid`,
`tid` or `uid`, the payload's value wins. This extends `prefix-and-payload-parsing`'s D7,
which stated the payload-wins rule only for timestamp, level and message, to the column
names this change introduces — same reasoning: the columns are stamped by the logging
framework around the application's own line, so the application's own key is the more
specific statement. Decided during implementation rather than up front; the opposite choice
is defensible.

### D3: Year inference is reused as-is

logcat's `MM-DD` carries no year, exactly like syslog 3164 and klog. It reuses
`ParseContext`'s caller-supplied reference instant and sets `timestamp_year_inferred`, which
the UI already surfaces as *"year inferred — this timestamp shape carries none"*. Nothing new
is needed; a bugreport from more than a year ago dates wrong and says so.

### D4: The logfmt tokenizer consumes a braced value as one opaque block

When a value begins with `{`, the tokenizer consumes to its matching `}` (tracking depth,
honouring quotes) and stores the whole span as the field's value.

The defect this fixes, from the measured file:

```
2026-04-16T21:34:04.009 - PROBE_HTTP http://… time=18ms ret=204
  request={Connection=[close], User-Agent=[Mozilla/5.0 …]}
  headers={null=[HTTP/1.1 204 No Content], Alt-Svc=[…], Content-Length=[0], …}
```

`time` and `ret` are real fields. Everything inside the two braced blocks is a Java map
dump, and today the tokenizer walks straight into it, producing top-level chips named
`Alt-Svc`, `Content-Length`, `Cross-Origin-Resource-Policy`, `X-Android-Sent-Millis`.

Alternative considered and rejected: raising the "at least two `key=value` pairs" dispatch
threshold. It does not distinguish the two cases — this line has genuine pairs *and* junk —
so raising the bar loses `time`/`ret` while a three-member map dump still gets in.

Consuming the block as one value is also what makes this consistent with the nested-JSON
rule already in force (`prefix-and-payload-parsing` D8, `field-extraction`: nested objects
are kept as their text, not flattened). `request` and `headers` become one field each,
holding their text — the same treatment `{"http":{"status":500}}` already gets.

### D5: The level scan ignores a token followed by `=`

`scan_message` splits on non-alphabetic characters and resolves each token through the
alias table, so `err=Success` yields the token `err`, which the `ERR` → `ERROR` alias turns
into a reported ERROR on a line whose own severity letter is `I`.

The fix is positional, not vocabulary-based: a token whose next character is `=` is a key
and is skipped. Dropping `ERR` from the table instead would break `level=err`, where `err`
genuinely is the level — the difference between the two cases is exactly the `=`, and which
side of it the token sits on.

This requires `scan_message` to track token end offsets rather than using `split()`, which
discards position. Same word-boundary semantics otherwise.

### D6: Detection and the sticky choice absorb the new shape without structural change

`Logcat` joins `TimestampShape`, the detection sample counts it like any other, and the
sticky selection records it like any other. Because plain text with a recognised prefix is
already reported as a threshold match, a bugreport's logcat-heavy sections stop reading as
`fell back to plain text` with no further work.

Note the detection sample is the first 100 non-empty lines, and a bugreport opens with ~1,370
lines of `dumpstate` preamble and section banners. Detection will *not* select a prefix shape
from that head — which is correct and harmless: plain text is still chosen, the sticky hint is
simply absent, and every line falls back to the full sweep. The file's own numbers are the
argument that this is acceptable: it is a 74%-unstructured file either way.

## Risks / Trade-offs

- **Column skipping swallows an ordinary line** → D1's `Tag:` anchor: the whole shape must
  match before anything is consumed. Negative tests cover a line that opens with a date and
  numeric columns but has no tag.
- **Throughput** → one more shape in the sweep, and a bugreport-shaped file defeats the
  sticky hint (see D6). `cargo bench -p looq-core` against the recorded 76.0 / 104.4 / 121.3
  ms/MB and the `plain_mixed_shapes` 149.3 ms/MB, with the <200 ms/MB TDR §11 gate.
- **The brace fix changes existing field output** → a line whose logfmt payload contains a
  braced value used to contribute the block's members as fields and now contributes one
  field holding the text. That is the documented nested-value rule, but it is a visible
  change for anyone who was filtering on those inner names.
- **The `=` guard changes existing level output** → a line whose only level-looking token is
  a key now reports no level instead of a wrong one. Reporting nothing is the correct
  outcome per `field-extraction` ("leave the level absent rather than guessing a default"),
  but it is a behavior change.
- **`core.wasm` growth** → measured against the 206,629 B baseline; roughly 100KB of
  headroom remains under the ~300KB budget.

## Open Questions

- Whether `tag` deserves to be a *recognised* field name (like `level`/`msg`) rather than an
  ordinary one. It is the highest-signal filter in an Android log, and the UI has no notion
  of a preferred field. Left ordinary for now — the chip list already surfaces it by
  cardinality.
- dmesg's boot anchor. The data to compute it is in the bugreport preamble, but reading it
  means the parser would extract file-level metadata, which nothing in `looq-core` does
  today. Deliberately deferred rather than half-built.
