## Why

looq recognises a timestamp only when the line opens with `YYYY-MM-DD`, and treats
everything after it as opaque message text. That covers JSON and logfmt cleanly and
almost nothing else: syslog (`Aug  8 17:42:01`), k8s klog (`I0808 17:42:01.123456`),
nginx/Apache access logs (`[08/Aug/2026:17:42:01 +0000]`) and `docker logs` output all
land in the plain-text fallback with `timestamp: None` — which means no point on the
timeline, the feature the product is built around. PRD §1 names `kubectl logs > file.log`
and `docker logs > file.log` as the target scenarios, and today both parse poorly.

The fix is not seven more named grammars. Almost every real log line is the same shape —
a prefix carrying the mandatory fields (when, how bad) followed by a payload carrying the
extended ones — so widening the prefix scanner and parsing the payload with the parsers
that already exist covers far more formats than naming them one at a time would.

## What Changes

- The leading-timestamp scanner learns more shapes: syslog RFC 3164 (`Aug  8 17:42:01`),
  klog (`0808 17:42:01.123456`), Apache/CLF (`08/Aug/2026:17:42:01 +0000`), slash-dates
  (`2026/08/08 17:42:01`) and a leading epoch value, alongside the ISO form it already
  handles.
- A timestamp is looked for within a bounded window at the head of the line rather than
  strictly at offset 0, so `1.2.3.4 - - [08/Aug/2026:...]` and `host app[123]: 2026-...`
  are recognised. Only token starts inside that window are tried.
- Formats that carry no year (syslog 3164, klog) get one inferred from context, and every
  entry that received an inferred year is flagged so the UI can say so. Silently inventing
  a year would put a wrong point on the timeline with no way to notice.
- The level is taken from a positional token immediately after the timestamp when one is
  there — `INFO`, `[INFO]`, `INFO:`, syslog `<130>`, klog/logcat single letters — falling
  back to the existing whole-message scan. **BREAKING (behavioral):** a line like
  `2026-08-08T17:42:01Z INFO retrying after ERROR response` now reports `INFO` where it
  previously reported `ERROR`.
- What remains after the prefix is handed to the existing JSON or logfmt parser when it
  looks like either, so `2026-08-08 17:42:01 INFO {"status":500}` yields a filterable
  `status` field instead of a message blob. **BREAKING (behavioral):** plain text used to
  contribute no fields and now can.
- On a prefix/payload conflict the payload wins for timestamp, level and message; the
  prefix timestamp is kept as an ordinary field so the disagreement stays visible.
- Docker's JSON wrapper (`{"log":…,"stream":…,"time":…}`) is unwrapped and its `log`
  member parsed as a line in its own right, rather than surfacing as an escaped blob in a
  field named `log`.
- Detection reports a prefixed plain-text match as a match rather than as a failure; the
  UI's "Format fell back to plain text" warning stops firing for input that parsed fine.

## Capabilities

### New Capabilities

None. This widens behavior that three existing capabilities already own.

### Modified Capabilities

- `log-parsing`: the plain-text fallback gains prefix recognition and nested-payload
  parsing; the JSON parser gains the Docker wrapper unwrap.
- `field-extraction`: timestamp patterns beyond ISO, the bounded head window, inferred-year
  reporting, positional-token level precedence, and plain text contributing fields.
- `format-detection`: a prefixed plain-text input is reported as a match with its evidence
  instead of as the never-rejected fallback.

## Impact

- `crates/looq-core/src/timestamp.rs` — new hand-rolled scanners, head-window search,
  inferred-year handling. No new crate dependencies: `regex` stays out (measured at ~870KB
  of `core.wasm` during `log-parsing-core`, against a ~300KB TDR §5 budget currently at
  194,350 B).
- `crates/looq-core/src/level.rs` — syslog numeric severities (8 → the 6-level table) and
  single-letter aliases.
- `crates/looq-core/src/parsers/{plain,json}.rs` — prefix split, payload dispatch, wrapper
  unwrap.
- `crates/looq-core/src/{entry,detect}.rs` — the inferred-year flag on `Entry`, detection
  outcome for prefixed plain text.
- `crates/looq-wasm/src/dto.rs` and `web/src/wasm-types.ts` — the new flag crosses the
  boundary.
- `web/src/components/looq-detection.ts` — detection copy.
- Performance: trying several scanners at several offsets on every line is a direct route
  to a regression in the slowest format. `cargo bench -p looq-core` (target: TDR §11's
  <200 ms/MB, currently ~80 ms/MB) gates the change.
- Both READMEs: the supported-input description changes for users.

Explicitly out of scope: named format grammars as `Format` variants (syslog/Apache as
first-class enum members), user-supplied patterns, a format selector in the UI, and
joining multi-line payloads such as stack traces into one entry.
