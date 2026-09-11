## Context

The logcat recognizer landed in `logcat-and-payload-precision`, measured against an
Android bugreport. That corpus decided its shape in a way nobody noticed at the time:
a bugreport's logcat sections are dominated by long tags — `ActivityManager`,
`ProcessCpuTracker`, `SystemServerTiming` — which are at or past `threadtime`'s
8-character tag column, so they carry no padding. `match_logcat_tag` was written to
stop the tag scan at the first space or colon and then require a colon, which is
exactly right for every line in that corpus and wrong for a third of the tags that
come out of `adb logcat`.

The archived change's design D1 is the reason the failure is total rather than
partial: the whole shape must match before any of it is consumed, precisely so that a
line merely opening with a date and some numbers is not half-eaten and handed to the
level matcher. That decision is correct and stays. Its consequence here is that a
single rejected space costs the record its timestamp, its level, its tag and both its
columns at once.

Current state on the live corpus (`adb logcat -v threadtime`, Android 10 emulator,
37,427 lines):

```
01-01 03:00:01.182   135   135 I vold    : Vold 3.0 firing up
                                   └──┘└──┘
                                    tag  padding to width 8
                                        ▲
                          scan stops here, byte is ' ' not ':' → None
                                        │
        match_logcat ──► None ──► prefix_shape ──► None
                                        │
                                        ▼
                                parse_unprefixed
                                  timestamp: None
                                  level:     None
                                  message:   the entire raw line
```

5,791 lines (15.5%) take that path. The three user-visible symptoms — a timeline that
reports 5,805 entries without a timestamp, a `tag=vold` filter that matches nothing
against 67 substring hits, and a detection banner reading "fell back to plain text" on
pure logcat input — are all this one branch.

## Goals / Non-Goals

**Goals:**

- `adb logcat` piped straight into looqlog parses, in its default format, without the
  user having to pass `-v` anything.
- The `tag` field is the tag, not the tag plus its column padding, so `tag=vold`
  filters on what a person would type.
- A stack trace under a padded tag stays one entry.
- Detection's logcat test fails when the recognizer is broken. Today it cannot.

**Non-Goals:**

- The other `-v` layouts. `-v brief` and `-v time` put the tag in a different position
  entirely (`I/vold    (  135): msg`); that is a new shape, not a padding rule, and
  `threadtime` has been the default since Android 7.
- The field-inventory noise measured alongside this bug. `logfmt::looks_like_payload`
  fires on prose whenever two `key=value`-ish tokens appear, and base64 padding in APK
  paths (`/data/app/dev.example-DOEu2lRv9e54rjvTIYQ9Yg==`) supplies them, producing
  field names like `/data/app/dev.example-DOEu2lRv9e54rjvTIYQ9Yg` with the value `=`.
  154 field names on this corpus, a handful of them real. It is not logcat-specific,
  it lives in a hot path shared by every format, and it belongs in its own change.
  Recorded under `## Ideas for later` in the devlog.
- Bugreport section awareness, still out of scope per the README's stated limitations.

## Decisions

### D1 — Accept the padding in the tag scanner, not by pre-trimming the line

`match_logcat_tag` scans the tag, then skips any run of spaces and tabs, then requires
the colon. The alternative — normalising whitespace in the line before matching —
would have to happen for every line of every format to help this one, and would
destroy the message's own indentation, which `logcat_message_indented` reads to decide
whether a line continues the one above it.

### D2 — The space run is not length-bounded

`threadtime` pads to width 8, so the longest run in the measured corpus is 5 spaces
(a three-character tag). A cap of 7 would cover every real case.

Rejected. The shape is already anchored on both ends: a tag token with no whitespace
and no colon of its own, then a mandatory colon, inside a sequence that must match in
full from the `MM-DD` onward before anything is consumed. The padding width adds no
safety a reader can act on, and an arbitrary constant is a thing the next person has
to research and cannot verify — AOSP's column width is not a promise to anyone. An
unbounded run of spaces before a required colon is the honest statement of what the
scanner is looking for.

The false-positive this gives up on is a line of the exact form `MM-DD hh:mm:ss.mmm`,
two or three lowercase/numeric columns, a severity letter, a word, a long run of
spaces, and a colon. Nothing in the measured corpus is shaped like that, and such a
line would be a logcat record by every other measure anyway.

### D3 — The tag is trimmed once, where the match is turned into a `LogcatRecord`

`match_logcat` already returns the tag as a `(start, end)` byte range. `end` is the
index the scan stopped at, which is now the first space rather than the colon, so the
range never includes the padding and the trim is free — nothing downstream has to know
about it.

This matters more than it looks. `LogcatIdentity::of` (`plain.rs`) stores the tag as an
owned `String` and compares it byte-for-byte against the next line's, which is how the
`entry-continuations` rule "same pid, tid, level and tag" is enforced. If the padding
survived into the record and were stripped later — say, only in `logcat_fields` — then
`vold    ` and `vold   ` (different padding for the same tag is impossible in practice,
but different tags of different lengths are not) would flow into identity comparison,
and any future change to how the tag is rendered would silently change which lines glue
together. One trim, at the point the range becomes a string, keeps the field value and
the identity comparison provably the same text.

### D4 — Detection is not touched

A recognised prefix in at least 80% of the sample is already a threshold match, not a
fallback (`prefix-and-payload-parsing` D9). With the recognizer fixed, the boot-log
sample goes from 68/100 to 99/100 and the banner corrects itself. There is no
detection bug to fix — there is a detection *test* that cannot fail, which is a
different defect and is fixed by giving it a padded fixture.

### D5 — The fixture comes from real output, and stops where the real output stops

Tests get a slice of actual `adb logcat` output rather than hand-written lines, since
hand-written lines are what produced this bug. The slice carries padded tags
(`vold`, `libc`, `netd`), unpadded ones (`ServiceManagement`, `wificond`), the
two-column layout and a continuation chain.

It will not carry a three-column `uid` record, because the measured emulator emits
none. Those scenarios keep the spec's existing synthetic lines, and the fixture does
not invent a shape nobody observed — an invented fixture is how the spec got a rule it
had never seen violated.

## Risks / Trade-offs

**A previously-unmatched line now matches, changing entry boundaries** → These lines
were not parsed before, they were dumped whole into a message. Any change to them is a
correction. The risk that matters is the opposite direction: a line that used to match
and now does not. Nothing in the change makes the scanner stricter, and the existing
"partial match is not consumed" scenario keeps the anchor test in place.

**Continuation chains change shape on padded tags** → Lines that previously had no
logcat identity at all could not open or extend a chain; they were unprefixed text,
which the chain logic treats by its own rules. Now they can. A padded-tag stack trace
collapsing into one entry is the intended outcome, but it is a visible difference on
existing input, so it gets its own test rather than being assumed.

**The unbounded space run widens the shape** (D2) → Mitigated by the shape's existing
anchors; see the false-positive analysis there. If a real false positive is ever
observed, the cap is a one-line change and the fixture to justify it will exist.
