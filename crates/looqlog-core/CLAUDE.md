# looqlog-core

Target-agnostic log engine: incremental parsing, format detection, field extraction, the
time index, filtering/search and timeline buckets behind one `Session` per source. No I/O,
no async, no clock (ADR-0005); CI fails if it depends on `wasm-bindgen`/`web-sys` or its
library source touches `std::fs`.

## Parser
- `Parser` takes byte chunks and returns the entries each chunk completes; file chunks and
  line-at-a-time stdin use the same path and give identical entries. An incomplete trailing
  line is held until the next chunk or `finish()`, which also flushes a final line with no
  newline.
- A line is decoded only once complete, so a multi-byte UTF-8 character split across chunks
  decodes correctly. A line that is not valid UTF-8 is decoded as latin-1 and records an
  `EncodingFallback` diagnostic. A trailing `\r` is stripped.
- Blank and whitespace-only lines yield neither entries nor diagnostics; they are counted.
- Entries plus skipped (malformed) lines plus blank lines equals the total line count.
- Lines are never joined: every physical line is its own `Entry`; multi-line events are
  linked, not merged (see Continuations).
- Entries come out in input order, never sorted. `ordinal` is the 1-based input line number;
  `starting_at_line` keeps ordinals when retained lines are re-parsed after eviction.
- One parser per input: reusing one mixes detection and field inventories.

## Formats
- JSON Lines: each line is an independent JSON value. Invalid JSON or a non-object value is
  skipped with an `InvalidJson` / `NonObjectJson` diagnostic, never aborting the parse.
- Docker json-file: an object whose members are exactly `log`, `stream`, `time` is unwrapped —
  `log` is parsed as a line, `time` supplies the timestamp only when the inner line has none,
  `stream` stays a field. An object that merely contains `log` is an ordinary entry.
- logfmt: whitespace-separated `key=value`; double-quoted values may hold spaces and escaped
  quotes; a bare token without `=` joins the message. A value starting with `{` is consumed to
  its matching `}` (nesting tracked, braces inside quotes ignored) and stored whole as one field.
- Plain text: one entry per non-empty line, never malformed. The message is the line minus the
  recognised timestamp and level tokens; text before them within the head window stays in the
  message; with nothing recognised the message is the whole line.
- Payload behind a plain-text prefix: text starting with `{` goes to the JSON parser, text with at
  least two `key=value` pairs to the logfmt parser (a lone `foo=bar` in prose stays text). One
  level only. A payload that fails to parse stays message text with no diagnostic.
- Payload values win over the prefix for timestamp, level and message; the prefix fills what the
  payload omits; a prefix timestamp that differs from the payload's is kept as field `prefix_ts`.

## Timestamps
- Named fields, in order: `timestamp`, `ts`, `time`, `@timestamp`, `t`. Values: RFC 3339 /
  ISO 8601, or integer epoch by magnitude — below 1e9 rejected, below 1e11 seconds, below 1e14
  milliseconds, else microseconds.
- An unparsable value in a timestamp field leaves the entry without a timestamp, keeps the value
  as an ordinary field and records `UnparsableTimestamp`. Entries without a usable timestamp are
  always kept, never given a substitute time.
- Plain-text prefix shapes: ISO 8601, slash-date (`2026/08/08 17:42:01`), klog
  (`I0808 17:42:01.123456`), syslog RFC 3164 (`Aug  8 17:42:01`), Apache/CLF
  (`[08/Aug/2026:17:42:01 +0000]`), logcat (`MM-DD hh:mm:ss.mmm`) and integer epoch. They are
  tried at token starts within the first 64 bytes (`HEAD_WINDOW`); the whole pattern must match.
- An explicit offset is respected. An offset-less value uses the caller's `TimeZonePolicy`: UTC by
  default or a fixed UTC offset; named IANA zones are not supported. `timestamp_used_default_tz`
  marks entries dated by that assumption.
- Year-less shapes (syslog 3164, klog, logcat) take the year of the caller-supplied reference
  instant, one year earlier when that would date the entry in the future; `timestamp_year_inferred`
  marks them. The crate never reads the system clock.
- The timestamp shape and head offset detection picked is tried first on each line, with a fall
  back to every shape on a miss and re-selection after 8 misses. It only orders attempts and never
  changes the entries produced.

## Levels
- From a `level`, `lvl` or `severity` field first. Otherwise from the token right after a
  recognised prefix: a bare word, `[INFO]`, `<INFO>`, `INFO:`, a syslog priority (`<130>`), or a
  single klog/logcat letter `V D I W E F`. Otherwise the first case-insensitive
  `TRACE|DEBUG|INFO|WARN|ERROR|FATAL` in the message, skipping any token followed by `=` (a key,
  so `err=Success` is no level). Single letters count only in the positional slot.
- Aliases: `WARNING`→WARN, `ERR`→ERROR, `CRITICAL`→FATAL. Syslog severities: emerg/alert/crit→FATAL,
  err→ERROR, warning→WARN, notice/info→INFO, debug→DEBUG. No match leaves the level absent; there
  is no default.
- The message comes from a `message` field, then `msg`.

## logcat
- A record is recognised only as a whole: `MM-DD hh:mm:ss.mmm`, two or three columns (digits, a
  short lowercase word, or `u0_aNN`; at most 16 chars each), a severity letter from `VDIWEFS`, and a
  tag ended by `:` — any run of padding whitespace may precede the colon (`vold    :`). A partial
  match leaves the line to the other shapes.
- The severity letter supplies the level; `S` (silent) supplies none.
- The tag becomes field `tag` without padding; columns become `pid` and `tid`, with `uid` first when
  there are three. The message is the text after the tag's colon.

## Fields
- Top-level JSON members other than the recognised timestamp/level/message keys become fields;
  nested objects and arrays stay as JSON text (`FieldValue::Json`), never flattened. Numbers keep
  their original text.
- Plain text contributes fields only from a parsed payload or a logcat record.
- `FieldInventory` keeps, per field, a total count and up to 10,000 distinct values with counts
  (`DEFAULT_FIELD_VALUE_CAP`). A field that overflows is flagged `high_cardinality` and stops
  collecting values, so memory does not grow with distinct values.

## Diagnostics
- Reasons: `InvalidJson`, `NonObjectJson`, `UnparsableTimestamp`, `EncodingFallback`,
  `ChainTruncated`. Per-reason counts are always exact; at most 5,000 examples (line, reason,
  detail) are retained (`DEFAULT_DIAGNOSTIC_CAP`).

## Format detection
- Samples the first 100 non-empty lines (fewer at `finish()` or `force_detection()`); those lines
  are held and replayed under the chosen format, so detection works on a partial read and on a
  stream's opening lines.
- JSON, then logfmt; a candidate wins when at least 80% of the sample parses under it. logfmt yields
  to plain text when the prefix scanner also recognises a timestamp in at least 80% of the sample,
  because plain text extracts the leading timestamp and level and still parses the logfmt payload.
  Otherwise plain text.
- Plain text is reported as a `Threshold` match when the prefix scanner recognises at least 80% of
  the sample, else as `Fallback`. Padded and unpadded logcat records count toward one logcat shape.
- `DetectionResult` reports the format, its match fraction (plain's own prefix fraction when it won
  the tie with logfmt), the outcome, and the modal timestamp shape and offset.
- An explicit format skips detection entirely, even when most lines then fail; the diagnostics show
  the mismatch.
- `force_detection()` ends the hold without ending input; it is a no-op on an empty sample, which
  would lock in plain text.

## Continuations
- Plain text only. JSON and logfmt never chain: a malformed line stays skipped and reported. The
  active format decides, not the content.
- A continuation line still produces its own `Entry` with its own values; `continuation_of` names
  the chain root's ordinal, not the predecessor. It is decided from the current line and state about
  lines already consumed, so no entry is ever held back for a later line.
- A chain opens only beneath an entry whose line had a recognised timestamp prefix. A blank line
  closes it, and a line that is not the immediate successor of the last member never links.
- An unprefixed line continues when it matches a frame marker, after optional leading whitespace:
  `at `, `Caused by:`, `... N more`, `Suppressed:`, `Traceback (most recent call last):`, a Python
  `File "…", line N` frame, or an exception header whose leading identifier's last dotted segment
  ends in `Exception`, `Error` or `Throwable`. A missing prefix alone does not make a continuation.
- A logcat record continues when pid, tid, level and tag equal the root's (timestamp ignored) and
  its message has leading whitespace, a frame marker, or the root left a `{` unclosed.
- While the root's `{` depth stays positive, following lines continue; the chain closes at zero.
  Only braces are counted, not brackets, because ANSI escapes contain `[`. Continuation lines add no
  fields to the root.
- At most 1,000 continuation lines per chain (`MAX_CHAIN_LINES`, just under HotSpot's 1,024-frame
  default). Past it the chain closes, the next line starts unlinked, and `ChainTruncated` names the
  root line; a chain of exactly 1,000 is not reported.

## Index
- `EntryIndex` keeps entries in input order; ordinals strictly increase, so lookup is a binary search
  and "retained" means `ordinal >= first retained ordinal`. A separate timestamp-sorted list of
  timestamped entries answers ranges and buckets without scanning; timestampless entries are counted
  apart. The browser never holds the index or the entry list.
- Append and front eviction are incremental; evicted entries leave ranges and bucket counts.
- Ranges are half-open `[start_ms, end_ms)` and return ordinals in input order.

## Filtering and search
- Field filters: OR within a field, AND across fields; `level` behaves as a field. The time range is
  not part of the predicate (the index answers it) and is ANDed in by the view.
- Search text: empty is no search; `re:` compiles a Rust `regex` as written, with no implicit flags;
  `\re:` searches the literal text; anything else is a case-insensitive substring. It matches the
  message, the level and every field value, never field names.
- An invalid regex returns an error (the readable last line of the `regex` error, e.g. look-around is
  not supported), never a search that matches nothing.
- A whitespace-terminated `field=value` token (`[A-Za-z_][\w.-]*`, non-empty value) naming a known
  field (the inventory plus `level`) becomes a filter; an unknown field stays search text, and the
  token still being typed is left alone.
- Chains: field filters judge the chain root only, so a whole trace shows under `level=ERROR`; the
  search may match any member and surfaces the whole chain. A member whose root is gone is judged on
  its own values as a standalone entry.
- Highlight ranges cover the message only, in UTF-16 code units.

## Session and View
- A `Session` holds one source: parser, index and retained raw lines. A `View` holds one client's
  query state (filters, search, range, chain toggles, generation), so clients never see each other's
  filters; views catch up lazily with appends and evictions.
- `SessionConfig::max_lines` bounds retained raw lines; the oldest are evicted from the raw lines and
  the index, and `evicted_lines` is cumulative.
- `set_query` starts a new generation. On a compile error the previous generation and state stay
  current and the error is returned. The reply carries total, matched and row counts, matched
  timestampless entries when a range is active, extracted tokens, remaining text and the error.
- A chain is expanded when exactly one holds: the search matched one of its members, or its root is
  toggled. Members of collapsed chains are left out of the rows that offsets address. Toggling
  re-folds rows without re-evaluating the predicate.
- `window(offset, limit)` returns at most 1,000 rows (`MAX_WINDOW_ROWS`); past the end it returns no
  rows, not an error. A row carries ordinal, RFC 3339 timestamp, the timezone/year flags, level,
  message, fields, `continuation_of`, `chain_size` (root plus visible members, 0 for none),
  `expanded`, `chain_member` and `match_ranges`.
- `locate(ordinal)` returns the row position of that entry or the first row after it, so an anchored
  window keeps its place while older entries are evicted.
- `entry(ordinal)` returns the row and its raw line, or neither once evicted.
- `buckets` covers the requested span, the full span, or by default the robust span: Tukey's fence
  (1.5×IQR; zero IQR pads 1 s), with entries outside it counted as outliers. The width is the smallest
  rung of 1s, 5s, 10s, 30s, 1m, 5m, 10m, 30m, 1h, 3h, 6h, 12h, 1d, 7d, 30d, 365d giving at most 120
  buckets; the grid is aligned to the width and includes the end instant. It returns unfiltered and
  filtered counts (the range is not applied), timestamped and timestampless counts, and the outlier
  count. Chain members are not counted: a sixty-frame trace is one event.
- `set_format(format | None, tz_offset_minutes)` re-parses the retained raw lines with the same
  ordinals, applies to later lines, and bumps the epoch so every view rebuilds.
- `stats()` returns format, detection, diagnostics summary, field inventory, per-level counts,
  retained entries, total and blank lines, evicted lines, bytes ingested and whether input finished.
- `retained_bytes()` (raw lines plus an index estimate calibrated with `examples/mem_probe.rs`) is
  what the server's memory budget counts.

## Benchmarks
- `cargo bench`: `parse_1mb` per format (`benches/parse.rs`); `session_ingest_1mb` and `refilter` at
  10,000 and 1,000,000 entries (`benches/session.rs`). Targets: under 200 ms per MB parsed; a filter
  change on 10,000 entries under 50 ms to the query reply.
