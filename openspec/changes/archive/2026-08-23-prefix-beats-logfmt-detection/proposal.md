## Why

A log line shaped `<ISO timestamp> <LEVEL> key=value key=value` is one of the most common
things a structured logger emits — a human-readable prefix in front of machine-readable
pairs. looqlog reads it badly: detection picks logfmt, and logfmt looks for `ts=`/`time=`
and `level=` as *keys*. Here the timestamp is a bare leading token and the level is
positional, so both are dropped.

The result is not a subtle degradation. On a 1,428-line sample every entry came back with
no timestamp and no level, and the timeline rendered as *"No entries have a usable
timestamp (1428 total) — nothing to place on a time axis."* The most valuable surface in
the product is simply blank, and nothing tells the user why.

Found while generating a demo log for the landing page: the first shape reached for was
this one, which is a fair indication of how common it is.

## What Changes

- Detection SHALL prefer plain text over logfmt when **both** cross the 80% threshold. The
  plain-text path is strictly more informative for these lines: it extracts the leading
  timestamp and the positional level, then hands the remainder to the *same* logfmt parser
  through `dispatch_payload`, so every field logfmt would have found is still found.
- JSON keeps its current precedence — nothing about JSON Lines detection changes.
- Genuine logfmt is unaffected, and the discriminator needs no new heuristic. Measured:

  | sampled line | `logfmt::matches` | `prefix_shape` |
  |---|---|---|
  | `2026-08-20T14:02:00.371Z INFO service=api msg="x" status=200` | true | `Some((Iso, 0))` |
  | `ts=2026-08-20T14:02:00Z level=info msg="x" service=api` | true | **`None`** |
  | `time=2026-08-20T14:02:00Z level=info msg="x" svc=api` | true | **`None`** |
  | `level=info msg="x" service=api status=200` | true | **`None`** |
  | `Aug  8 17:42:01 host app[123]: connection refused` | false | `Some((Syslog3164, 0))` |

  A real logfmt line has no recognisable prefix, because the timestamp scanner requires a
  token boundary and `ts=` does not provide one. So "logfmt matched **and** a prefix
  matched" already separates the two cases exactly, with no offset rule or new threshold.

No **BREAKING** change: no input that parses correctly today parses differently after. The
inputs that change are the ones currently losing data.

## Capabilities

### New Capabilities
None.

### Modified Capabilities
- `format-detection`: the selection rule gains a tie-break. "First candidate over the
  threshold wins, in JSON → logfmt → plain order" becomes "…except that a logfmt win is
  yielded to plain text when the prefix scanner also crosses the threshold".

## Impact

- `crates/looqlog-core/src/detect.rs` — `detect()` computes `prefix_evidence` before
  deciding logfmt, and the logfmt branch becomes conditional. `match_fraction` reporting
  and the sticky-shape hint follow the branch actually taken.
- `crates/looqlog-core/tests/parser_integration.rs` and `detect.rs`'s own unit tests — new
  cases for the four rows of the table above.
- New fixture covering the shape end to end: timestamp, level and payload fields all
  present on the same entry.
- No frontend change: the UI already renders whatever detection reports, and the
  `looqlog-detection` panel will simply say `plain` where it used to say `logfmt`.
- No `core.wasm` size concern — the change reorders existing calls and adds no new
  scanning.
