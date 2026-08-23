## Context

`detect()` (`crates/looqlog-core/src/detect.rs`) walks candidates in TDR §8 priority order
and returns the first to cross `THRESHOLD` (0.8):

```
json_fraction   >= 0.8  ?  → Json      return
logfmt_fraction >= 0.8  ?  → Logfmt    return          ← returns here today
prefix_evidence            → Plain, Threshold or Fallback
```

For a line like

```
2026-08-20T14:02:00.371Z INFO service=api msg="request completed" status=200 duration_ms=23
└────── bare token ─────┘ └──┘ └──────────────── k=v pairs, enough for logfmt ───────────┘
                        positional
```

`logfmt::matches` is satisfied by the pairs, so the function returns before
`prefix_evidence` is ever computed. The logfmt parser then looks for a `ts=`/`time=` key
and a `level=` key, finds neither, and the entry arrives with `timestamp: None` and
`level: None`. Measured on a 1,428-line sample: 1,428 entries with no timestamp, and the
timeline reporting that it has nothing to place on a time axis.

What makes this fixable cheaply is that the plain-text path already handles this exact
shape. `prefix-and-payload-parsing` design D6 gave `plain::parse_line` a payload
dispatcher: it consumes a recognised timestamp prefix and the level token behind it, then
hands whatever remains to the JSON or logfmt parser. So for these lines plain produces a
strict superset of what logfmt produces.

## Goals / Non-Goals

**Goals:**
- A line carrying both a timestamp prefix and logfmt pairs yields its timestamp, its level
  *and* its fields.
- Genuine logfmt input keeps being detected and reported as logfmt.
- No new heuristic, no new threshold, no new tuning constant.

**Non-Goals:**
- Teaching the logfmt parser to recognise a bare leading timestamp. That would duplicate
  the prefix scanner inside a second parser and leave two places to keep in agreement.
- Changing JSON's precedence, or the 80% threshold, or the sample size.
- Making detection reconsider itself mid-stream. The choice stays a one-time decision over
  the head of the input.

## Decisions

### D1 — Plain wins a tie with logfmt, because plain subsumes logfmt here

When both candidates cross the threshold, the plain-text path returns everything the
logfmt path would have, plus two fields it would have dropped:

```
plain::parse_line("2026-08-20T14:02:00.371Z INFO service=api msg=\"x\" status=200")

  extract_leading      → timestamp 2026-08-20T14:02:00.371Z     ← lost under logfmt
  level::match_positional("INFO")  → Level::Info                ← lost under logfmt
  rest = "service=api msg=\"x\" status=200"
  dispatch_payload → logfmt::parse_line(rest)
                     → message "x", fields service/status       ← identical to logfmt
```

The tie-break is therefore not a judgement about which format the file "really is" — it is
a choice between a parse that keeps data and a parse that discards it.

### D2 — "logfmt matched AND a prefix matched" is the whole discriminator

The obvious worry is that this flips genuine logfmt files to plain. It does not, and the
reason is a property of the existing scanner rather than a new rule. Measured on the four
shapes that matter:

| sampled line | `logfmt::matches` | `prefix_shape` |
|---|---|---|
| `2026-08-20T14:02:00.371Z INFO service=api msg="x" status=200` | true | `Some((Iso, 0))` |
| `ts=2026-08-20T14:02:00Z level=info msg="x" service=api` | true | `None` |
| `time=2026-08-20T14:02:00Z level=info msg="x" svc=api` | true | `None` |
| `level=info msg="x" service=api status=200` | true | `None` |

`timestamp::extract_leading` requires a token boundary before a candidate timestamp, and
`ts=` does not supply one, so a logfmt line that carries its timestamp *as a value* yields
no prefix at all. The two populations are already disjoint on this test.

An earlier draft of this design added "…and the modal prefix offset is 0" as a second
condition, on the assumption that `ts=2026-…` would match at offset 3. The measurement
above shows it matches at no offset, so the extra condition would be dead weight — a
guard for a case that cannot arise, which future readers would have to reason about. It is
deliberately not included.

### D3 — Evaluation order changes, priority order does not

`prefix_evidence` moves above the logfmt branch because the logfmt decision now depends on
it. That is an evaluation-order change, not a priority change: JSON still wins over
everything, and logfmt still wins over plain whenever plain has no prefix evidence. The
spec sentence gains a clause; it does not get reordered.

`prefix_evidence` is O(sample × shapes) and runs over at most `SAMPLE_SIZE` (100) lines. It
was already being computed on every plain-text detection; now it is also computed on
logfmt-shaped input. One extra pass over 100 lines, once per input, is not a hot path.

### D4 — The reported format changes, and that is the point

Input that used to report `logfmt (100%)` will report `plain (100%)`, and the detection
panel will say so. That is a visible change, and it is correct: the file is plain text with
a structured payload, which is precisely what the plain path models. `match_fraction`
follows the branch actually taken, so the number shown stays the evidence for the format
shown.

## Risks / Trade-offs

- **A genuine logfmt file gets pulled into plain.** → Requires a logfmt line whose
  timestamp sits outside a `key=` position while still parsing as a prefix. The measured
  table shows the common forms do not. The residual case is a file whose lines begin with a
  bare timestamp *and* are otherwise pure logfmt — which is exactly the shape this change
  exists to serve, and which plain handles better anyway.
- **Someone relied on the old behaviour.** → Nothing they relied on produced correct
  timestamps or levels for these lines, so there is no working configuration to break. The
  fixture and tests pin both the old-correct cases and the newly-correct ones.
- **The reported format string changes for affected inputs.** → Visible in the detection
  panel and in `#format=` URLs people may have saved. An explicit `#format=logfmt` override
  still forces logfmt, unchanged: overrides skip detection entirely, so anyone who wants
  the old parse can still ask for it by name.
- **Detection stays a one-shot decision over the head of the input.** A file whose first
  100 lines have prefixes and whose remainder does not will still be parsed as plain
  throughout. That is pre-existing behaviour and out of scope here.
