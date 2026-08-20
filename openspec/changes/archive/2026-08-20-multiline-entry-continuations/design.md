## Context

`looq-core`'s parser is line-oriented end to end: `Parser::feed` splits on `\n` and
`consume_line` turns each line into exactly one `Entry` (`crates/looq-core/src/parser.rs`).
That was correct for JSON Lines and logfmt, where a line *is* a record, and it survived
`prefix-and-payload-parsing` and `logcat-and-payload-precision` because both of those
changes made a single line parse *better* without ever questioning the one-line-one-entry
assumption.

Stack traces break it. The measurements below come from the 13 MB / 168,260-line Android
bugreport used throughout those two changes:

```
168 260  lines total
 33 300  logcat-prefixed          (20%)
134 960  dumpsys / /proc / VM TRACES  (80%)
```

Continuations show up in that corpus in a shape the original framing of PRD §14 Q3 did
not anticipate. There are two distinct phenomena, plus one trap:

**Prefixed continuations (831 lines).** logcat re-stamps every physical line, so a Java
trace arrives with a full prefix on each frame:

```
04-21 12:56:57.372 10044 2075 6108 E JAZZ/WebSocketClientImpl: Error websocket
04-21 12:56:57.372 10044 2075 6108 E JAZZ/WebSocketClientImpl: java.net.SocketTimeoutException: …
04-21 12:56:57.372 10044 2075 6108 E JAZZ/WebSocketClientImpl: 	at libcore.io.IoBridge.connectErrno(…)
```

These entries already carry a timestamp, a level, a tag and columns — the parser reads
them correctly today. They are one event only because the prefix *repeats*.

**Prefix-less continuations (0 instances).** The classic Java/Python-to-stdout shape,
which is what PRD §14 Q3 describes, does not occur in this corpus at all. There are 3,344
bare `at …` lines, but every single one sits inside the VM TRACES thread-dump section
with no log entry above it — preceded by `native: #03 pc …` or `| held mutexes=`.

**The trap.** 92,151 lines (55% of the file) begin with whitespace, and the longest
unbroken run of non-blank prefix-less lines is 6,206 (`VMALLOC INFO`). Any rule shaped
like "indented line continues the previous one" swallows most of the file into a handful
of enormous entries.

## Goals / Non-Goals

**Goals:**
- A multi-line event is identifiable as one event: collapsible in the table, one point on
  the timeline, one unit for filtering and search.
- The recognition rule never fires on the 134,960 lines of dump text in the measured
  corpus.
- Incremental parsing is preserved exactly as it is: `feed()` with one line still returns
  what it returns today, at the same moment, with no new flush obligation on the caller.
- ADR-0005 holds: no clock, no `wasm-bindgen`, no `std::fs` in `looq-core`.
- TDR §5's `core.wasm` budget holds (210,165 of ~300,000 bytes today), with no `regex`
  dependency — byte scanners only, as everywhere else in this crate.

**Non-Goals:**
- Sectioning a bugreport into its `------ SECTION ------` blocks. That is what would
  actually tame the other 134,960 lines, and it is a different change. This one must not
  make those lines worse; it does not try to make them better.
- Extracting fields from a multi-line JSON payload (see D7 — this is forced by D1, not a
  scope preference).
- Merging continuation lines into one `Entry` inside `looq-core`.

## Decisions

### D1 — Mark, don't merge

`Entry` gains `continuation_of: Option<usize>`. One `Entry` per physical line is kept;
grouping is a rendering concern.

Three models were compared:

| | Latency in live mode | Lookahead | Cost |
|---|---|---|---|
| **Hold-and-flush** — buffer the entry, append continuations, emit on the next starter | the last entry is invisible until the *next* line arrives — in a quiet stream, forever | 1 entry | needs `Parser::flush()` plus a timer in `live-tail.ts`, because ADR-0005 forbids a clock in `looq-core` |
| **Emit-then-amend** — emit immediately, patch the entry when a continuation arrives | none | none | a second message shape on the wire; `EntryIndex.matchingOrdinals`/`sorted` need manual reconciliation because the message grew; a patch aimed at an evicted ordinal must be a no-op |
| **Mark-don't-merge** (chosen) | none | **none** | one nullable field on the wire; grouping logic moves to the UI |

The deciding property: **merging needs lookahead, marking needs only lookbehind.** "Is
this line a continuation?" is answerable from the current line plus remembered state about
the previous one, never from a line that has not arrived. The hardest-looking question in
this problem — what an entry whose continuation has not arrived yet should look like — is
an artifact of the hold-and-flush model, not a property of the task. Choosing D1 deletes
it.

Marking also filters and searches better than merging: a merged entry highlights as one
blob when the user searches for `IoBridge`, whereas a marked chain keeps the matching
frame addressable and shows it inside its root's context.

### D2 — `continuation_of` points at the chain root, not the predecessor

The UI needs "which group is this row in", not "what came immediately before". Storing the
root means the table, the filter predicate and the timeline each do one lookup instead of
walking a chain that can be hundreds of lines long, and it makes chain membership a
constant-time test.

### D3 — Three recognizers, one shared guard set

```
GUARDS (apply to every recognizer)
  · chain root must be an entry whose line had a recognised timestamp prefix
  · a blank line closes any open chain
  · a chain that would exceed the cap is closed; the truncation is reported (D8)
  · only Format::Plain participates (D6)

R1  prefix-less frame
    extract_leading() == None
    AND the line matches an explicit marker, after optional leading whitespace:
      `at `  `Caused by:`  `... N more`  `Suppressed:`
      `Traceback (most recent call last):`  `File "…", line N`
      an exception header: a dotted or bare identifier whose last segment ends
      in `Exception`, `Error` or `Throwable`, optionally followed by `: message`
      (`java.lang.NullPointerException: …`, `ValueError: …`)
    OR the line is indented AND the previous chain member was a Python
      `File "…", line N` frame, whose source line follows it carrying no
      marker of its own

R2  logcat identity
    the line is a logcat record
    AND (pid, tid, level, tag) equal the chain root's          (D4)
    AND the message carries a continuation signal:
      leading whitespace/tab, OR an R1 marker, OR the root left brace depth > 0

R3  open brace
    the chain root's message ends with unclosed `{` depth       (D5)
    the chain continues, tracking depth, until depth returns to 0
```

An explicit marker list was chosen over "no prefix" or "no prefix and indented":

| Rule | Fires on, in the measured corpus |
|---|---|
| `extract_leading() == None` | 134,960 lines; longest chain 6,206 |
| …and the line is indented | 92,151 lines; longest chain still 6,206 |
| explicit markers **and** a prefixed root | **0** false positives out of 134,960 |

This is the same discipline the rest of the crate follows: the claim "this line belongs to
the one above it" is made on positive evidence, not on the absence of evidence.

The exception-header marker is not optional, and the existing
`crates/looq-core/tests/fixtures/stack-trace.log` is why:

```
2026-08-08T17:42:11Z ERROR unhandled exception in request handler   ← root
java.lang.NullPointerException: Cannot invoke "String.length()" …   ← no `at `
	at com.example.app.Handler.process(Handler.java:42)              ← frame
```

Without it the header is not a continuation, so it becomes an unlinked entry with no
prefix — and the frames below it then have no prefixed root to attach to, so the entire
chain fails. Since the decision is lookbehind-only (D1), the header cannot be linked
retroactively once the frames prove what it was; it has to be recognised on its own.
Requiring the last identifier segment to end in `Exception`, `Error` or `Throwable` keeps
the rule narrow enough that ordinary prose does not match it.

R2 needs no equivalent: a logcat exception header carries the full prefix, so it simply
becomes the chain root itself and the frames attach to it.

The Python frame-body clause is there for the same reason. A traceback frame is two lines,
and the second carries nothing recognisable at all:

```
  File "app.py", line 12, in handler        ← marker
    return int(request.args["count"])       ← the frame's source line, no marker
```

It is the one clause that keys off *what the previous member was* rather than off the
current line alone. That does not weaken D1 — it is still strictly lookbehind, decided from
state already accumulated — but it is narrower than it looks: it fires only on the single
line directly after a `File "…", line N` member, and only if that line is indented.

Notably, the root-must-have-a-prefix guard is what neutralises the VM TRACES section
without any special-casing: those 3,344 bare `at …` lines follow `| held mutexes=` and
`native: #03 pc …`, which carry no timestamp, so no chain is ever open above them.

### D4 — logcat identity excludes the timestamp

Measured: 830 of 831 frames share the root's millisecond, and one does not.

```
13:07:51.983  1000  806 29149 W System.err: java.lang.IndexOutOfBoundsException
13:07:51.984  1000  806 29149 W System.err: 	at com.android.server.usb.descriptors.ByteStream…
                ↑ pid/tid/level/tag identical, timestamp drifted 1 ms
```

`(pid, tid, level, tag)` alone catches 831 of 831. Including the timestamp would break
exactly the chain a user most wants — a trace long enough to cross a millisecond boundary.

The message signal in R2 is not optional, though: 946 consecutive
`NetworkSensitiveLogger: *` lines share `(pid, tid, level, tag)` with their neighbours and
are genuinely separate events. Identity establishes *candidacy*; the message establishes
*continuation*.

### D5 — Count `{`/`}` only, never `[`/`]`

logcat carries raw ANSI escapes: `vhdnativeservice` pipes `top` output through, so
`ESC[7m` and `ESC[1m` appear mid-message. Counting square brackets treats every one of
those as an opening bracket.

```
counting { } [ ]  → 557 lines end with positive depth
counting { }      → 237 lines end with positive depth
```

Of the 237, 198 are prefix-less dump text that the root-prefix guard rejects, leaving 39
real payloads — the `BTB_UPDATER/ConfigRepositoryImpl: response body = {` family. A
JSON array payload split across lines is therefore not recognised; that is an accepted gap,
and a cheaper one than 320 false chains.

### D6 — Only `Format::Plain` participates

A JSON Lines record and a logfmt record are self-delimiting by construction: a line is a
complete record or it is malformed, and malformed already has a defined behaviour (skip +
diagnostic). Letting continuations run under those formats would mean a truncated JSON line
silently absorbing the next record instead of being reported. The plain-text path is where
multi-line events actually live.

### D7 — A multi-line JSON payload is grouped, not field-extracted

This is a forced consequence of D1 and must be stated plainly rather than discovered later.

Turning `response body = {` plus its 40 continuation lines into `config.common.*` filter
chips requires reassembling the text and then amending an `Entry` that was already emitted
— which is the emit-then-amend model D1 rejected. So `dispatch_payload`
(`crates/looq-core/src/parsers/plain.rs`) is not modified: the root line still fails
`json::parse_payload` and stays message text, and the existing
`payload_that_fails_to_parse_stays_message_text` test continues to hold unchanged.

What the user gets: the payload collapses into one row and copies as one block. What the
user does not get: filtering on keys nested inside it. If that turns out to matter, it is a
later change that revisits D1, not a patch on this one.

### D8 — Chain length is capped, and truncation is reported

Cap: **1,000 lines**. The HotSpot JVM prints at most 1,024 frames for a
`StackOverflowError` by default, so 1,000 sits just under the largest legitimate trace
while bounding a runaway chain far below the 6,206-line dump run this corpus contains.

When the cap is hit, the chain closes and the offending line starts a fresh entry with
`continuation_of: None`. A `DiagnosticReason::ChainTruncated` is recorded with the root's
line number. Silent truncation is exactly the failure mode CLAUDE.md's testing rules name:
a chain that quietly stops looks to the user like a trace that was short.

Note this does not disturb the accounting invariant in the `log-parsing` spec ("entries
produced plus lines skipped plus blank lines equals the total line count") — a truncated
chain skips no lines. Every line still becomes an entry; only the grouping changes.

### D9 — A chain is one event downstream

- **Timeline** — continuation entries are excluded from bucket counts. Otherwise the
  measured `SocketTimeoutException` reads as ~60 errors in one millisecond, and 831 frames
  become 831 false peaks across the file.
- **Filtering and search** — the predicate is evaluated against the chain root, and a match
  on any member surfaces the root with the chain intact. `level=ERROR` therefore shows the
  whole trace; a search for `IoBridge` shows the trace with the matching frame highlighted,
  rather than a naked frame with no indication of which exception it belonged to.

### D10 — Parser state

`Parser` gains `chain: Option<ChainState>` holding the root ordinal, the root's logcat
identity (owned `pid`/`tid`/`tag` strings plus level), the running brace depth, and the
member count. It is cleared on a blank line, on cap exhaustion, and on any line that no
recognizer accepts. `LogcatRecord` borrows from the line, so the identity must be copied
into the state rather than held by reference.

## Risks / Trade-offs

- **A rule that over-fires collapses the dump sections into giant entries.** → The
  root-must-have-a-prefix guard plus explicit markers measures 0 false positives across
  134,960 prefix-less lines. The cap (D8) bounds the damage even if a future format slips
  through, and the blank-line break gives 3,633 natural chain boundaries in this corpus
  alone.
- **R1 has no coverage in the real corpus.** The one shape PRD §14 Q3 actually describes
  has zero instances in the bugreport. → Synthetic fixtures for Java-to-stdout and Python
  traceback are mandatory deliverables of this change, not nice-to-haves. Verifying against
  the bugreport alone would leave R1 completely untested.
- **`core.wasm` budget.** → The recognizers are byte-prefix comparisons plus a depth
  counter; the new state is one small struct per parser and one `Option<usize>` per entry.
  Expected cost is low single-digit KB against a ~90 KB headroom, but it must be measured
  and recorded, not assumed. `./scripts/build-frontend.sh` runs and
  `crates/looq/assets/` is committed in the same change — the macOS CI job fails on
  vendored-artifact drift otherwise.
- **Live-tail eviction can orphan chain members.** `EntryIndex.evictFront` drops the oldest
  entries, so a chain root can be evicted while its members remain, leaving
  `continuationOf` pointing at an ordinal that is no longer in the index. → An orphaned
  member renders as an ordinary standalone row. This must be an explicit, tested behaviour,
  not an unhandled lookup.
- **Timeline counts stop matching line counts.** Excluding continuations means the
  timeline's total no longer equals the number of table rows. → This is the intended
  reading (events, not lines), but it is a visible semantic change and belongs in both
  READMEs.
- **Cross-format false chaining is prevented by construction, not by testing.** D6 keeps
  JSON and logfmt out entirely, so a truncated JSON record can never absorb the next one.
