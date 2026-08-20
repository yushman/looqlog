//! Timestamp parsing, the timezone policy (`log-parsing-core` design.md D5, resolves
//! TDR §16 and PRD §14 Q2/timezone question) and the plain-text prefix scanner
//! (`prefix-and-payload-parsing` design.md D2/D3/D4).
//!
//! Field-name precedence for structured formats: `timestamp`, `ts`, `time`,
//! `@timestamp`, `t`, in that order (field-extraction spec).
//!
//! Everything below the `parse_value` section is hand-rolled byte matching. `regex`
//! is deliberately absent: it measured at ~870KB added to `core.wasm` against the
//! ~300KB TDR §5 budget (see `Cargo.toml` and the devlog), so every shape here is a
//! scanner in the style of `match_iso`. Each branch advances only over matched ASCII
//! bytes, so every slice returned is on a `str` char boundary.

use std::cell::Cell;

use chrono::{DateTime, Datelike, FixedOffset, NaiveDate, NaiveDateTime, TimeZone, Utc};

/// Field names checked for a timestamp, in precedence order.
pub const TIMESTAMP_FIELDS: &[&str] = &["timestamp", "ts", "time", "@timestamp", "t"];

/// How a timestamp carrying no explicit offset is interpreted. Named IANA zones
/// (e.g. `Europe/Belgrade`) are NOT supported: that needs a timezone database
/// (`chrono-tz`), whose embedded data would push `core.wasm` well past the ~300KB
/// budget this same change is required to protect (design.md D9 risk, TDR §5). A
/// caller-supplied fixed UTC offset covers the same "naive value written in a known
/// local time" case without that cost. Recorded as a deliberate scope cut — see
/// devlog and the final report ("NEEDS HUMAN DECISION": named-zone support).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TimeZonePolicy {
    default_offset: FixedOffset,
}

impl TimeZonePolicy {
    pub fn utc() -> Self {
        Self {
            default_offset: FixedOffset::east_opt(0).unwrap(),
        }
    }

    pub fn fixed_offset(offset: FixedOffset) -> Self {
        Self {
            default_offset: offset,
        }
    }

    pub fn offset(&self) -> FixedOffset {
        self.default_offset
    }

    fn apply(&self, naive: NaiveDateTime) -> DateTime<Utc> {
        self.default_offset
            .from_local_datetime(&naive)
            .earliest()
            .unwrap_or_else(|| self.default_offset.from_utc_datetime(&naive))
            .with_timezone(&Utc)
    }
}

impl Default for TimeZonePolicy {
    fn default() -> Self {
        Self::utc()
    }
}

/// The timestamp shapes the plain-text prefix scanner recognises. These are *not*
/// `Format` variants (design.md D1): they are recognised inside plain text rather
/// than becoming their own detection candidates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimestampShape {
    /// `2026-08-08T17:42:01.123Z`, `2026-08-08 17:42:01+03:00`.
    Iso,
    /// `2026/08/08 17:42:01`.
    SlashDate,
    /// klog: `0808 17:42:01.123456`, optionally behind a severity letter
    /// (`I0808 …`) which the level extractor consumes.
    Klog,
    /// Android logcat: `04-21 13:07:53.198  1000 806 995 D ActivityManager:` — the
    /// timestamp is only the head of it, and the whole record through the tag's colon
    /// has to match before any of it is consumed (`logcat-and-payload-precision`
    /// design.md D1).
    Logcat,
    /// syslog RFC 3164: `Aug  8 17:42:01` (note the space-padded single-digit day).
    Syslog3164,
    /// Apache/CLF: `08/Aug/2026:17:42:01 +0000`.
    Clf,
    /// A leading integer epoch value, in seconds/millis/micros by magnitude.
    Epoch,
}

impl TimestampShape {
    pub fn as_str(&self) -> &'static str {
        match self {
            TimestampShape::Iso => "iso8601",
            TimestampShape::SlashDate => "slash-date",
            TimestampShape::Klog => "klog",
            TimestampShape::Logcat => "logcat",
            TimestampShape::Syslog3164 => "syslog3164",
            TimestampShape::Clf => "clf",
            TimestampShape::Epoch => "epoch",
        }
    }
}

/// Every shape, in the order one offset is swept. ISO first because it is both the
/// commonest and the cheapest to reject; `Epoch` last because it is the loosest.
/// The order is not load-bearing for *results*: no two shapes can match at the same
/// offset (they disagree on the byte at a fixed index — `-`, `/`, ` `, a month name
/// or a digit), which is what makes the sticky reordering in `extract_leading`
/// result-neutral (design.md D3, format-detection spec "Sticky choice does not
/// change the result"). logcat joins the list on the same terms
/// (`logcat-and-payload-precision` design.md D6): its `MM-DD` head disagrees with
/// every other shape at a fixed index — ISO and slash-date want four digits before
/// their separator, klog four digits then a space, CLF a `/` at index 2, syslog a
/// month name, epoch ten or more digits — so the no-overlap property above still
/// holds with seven entries.
const SHAPES: [TimestampShape; 7] = [
    TimestampShape::Iso,
    TimestampShape::SlashDate,
    TimestampShape::Klog,
    TimestampShape::Logcat,
    TimestampShape::Syslog3164,
    TimestampShape::Clf,
    TimestampShape::Epoch,
];

/// How far into a line a timestamp is looked for (design.md D2). Wide enough for the
/// prefixes real stacks emit (`1.2.3.4 - - [`, an IPv6 address, `host app[123]: `),
/// narrow enough that the work per line stays constant however long the line is.
pub const HEAD_WINDOW: usize = 64;

/// Consecutive lines whose match did not come from the sticky choice before the
/// choice is replaced (design.md D3). Small enough to adapt quickly when a file
/// switches shape part-way, large enough that an occasional odd line does not thrash
/// the hint.
const STICKY_MISS_LIMIT: usize = 8;

/// Everything the parser needs from its caller that it must not work out for itself:
/// the timezone policy for offset-less values, and the reference instant used to
/// infer a year for the shapes that carry none (design.md D4). `looqlog-core` never
/// reads the system clock — ADR-0005 requires it to stay target-agnostic, so "now"
/// arrives as a parameter.
///
/// Also carries the sticky prefix choice (design.md D3). It lives here, behind
/// `Cell`, so the per-line parser signatures stay `&ParseContext`: the hint is a
/// cache, not part of the parse result, and every code path that reads it falls back
/// to the full sweep, so a stale hint can only cost time, never change an entry.
#[derive(Debug)]
pub struct ParseContext {
    tz: TimeZonePolicy,
    reference: Option<DateTime<Utc>>,
    prefix_hint: Cell<Option<(TimestampShape, usize)>>,
    sticky_misses: Cell<usize>,
}

impl ParseContext {
    pub fn new(tz: TimeZonePolicy) -> Self {
        Self {
            tz,
            reference: None,
            prefix_hint: Cell::new(None),
            sticky_misses: Cell::new(0),
        }
    }

    pub fn utc() -> Self {
        Self::new(TimeZonePolicy::utc())
    }

    /// Supply the reference instant for year inference. Without one, year-less shapes
    /// (syslog 3164, klog) are not recognised at all rather than dated by invention —
    /// the same "absent beats guessed" rule the level extractor follows.
    pub fn with_reference(mut self, reference: DateTime<Utc>) -> Self {
        self.reference = Some(reference);
        self
    }

    pub fn tz(&self) -> &TimeZonePolicy {
        &self.tz
    }

    pub fn reference(&self) -> Option<DateTime<Utc>> {
        self.reference
    }

    /// Seed the sticky choice from detection (format-detection spec, "Detection fixes
    /// the prefix shape for the input").
    pub fn set_prefix_hint(&self, shape: TimestampShape, offset: usize) {
        self.prefix_hint.set(Some((shape, offset)));
        self.sticky_misses.set(0);
    }

    pub fn prefix_hint(&self) -> Option<(TimestampShape, usize)> {
        self.prefix_hint.get()
    }

    /// Consecutive matches that did not come from the sticky choice. Exposed for
    /// tests asserting the "uniform input pays for one attempt" scenario.
    pub fn sticky_misses(&self) -> usize {
        self.sticky_misses.get()
    }

    fn note_sticky_hit(&self) {
        self.sticky_misses.set(0);
    }

    fn note_sticky_miss(&self, shape: TimestampShape, offset: usize) {
        let misses = self.sticky_misses.get() + 1;
        if misses > STICKY_MISS_LIMIT || self.prefix_hint.get().is_none() {
            self.set_prefix_hint(shape, offset);
        } else {
            self.sticky_misses.set(misses);
        }
    }
}

impl Default for ParseContext {
    fn default() -> Self {
        Self::utc()
    }
}

/// Result of parsing a timestamp value: the instant, whether the caller's
/// default/override timezone had to be applied (i.e. the value carried no explicit
/// offset), and whether its year was inferred rather than read (design.md D4).
/// Both assumptions are reported so a UI can tell the user which entries are dated
/// by assumption rather than by the log.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ParsedTimestamp {
    pub instant: DateTime<Utc>,
    pub used_default_tz: bool,
    pub year_inferred: bool,
}

/// Parse a timestamp value: RFC 3339 / ISO 8601 with an explicit offset, a naive
/// date-time interpreted per `tz`, or an integer epoch value in seconds,
/// milliseconds or microseconds (disambiguated by magnitude). Returns `None` if
/// nothing recognisable matches.
pub fn parse_value(raw: &str, tz: &TimeZonePolicy) -> Option<ParsedTimestamp> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }

    if let Some(epoch) = parse_epoch(raw) {
        return Some(ParsedTimestamp {
            instant: epoch,
            used_default_tz: false,
            year_inferred: false,
        });
    }

    // RFC 3339 handles an explicit offset (including `Z`) directly.
    if let Ok(dt) = DateTime::parse_from_rfc3339(raw) {
        return Some(ParsedTimestamp {
            instant: dt.with_timezone(&Utc),
            used_default_tz: false,
            year_inferred: false,
        });
    }

    // Accept a space separator as well as `T` (common in plain-text logs), still
    // with an explicit offset/`Z` suffix.
    let t_form = replace_first_space_with_t(raw);
    if let Ok(dt) = DateTime::parse_from_rfc3339(&t_form) {
        return Some(ParsedTimestamp {
            instant: dt.with_timezone(&Utc),
            used_default_tz: false,
            year_inferred: false,
        });
    }

    // No offset at all: naive date-time, interpreted per the timezone policy.
    for fmt in ["%Y-%m-%dT%H:%M:%S%.f", "%Y-%m-%dT%H:%M:%S"] {
        if let Ok(naive) = NaiveDateTime::parse_from_str(&t_form, fmt) {
            return Some(ParsedTimestamp {
                instant: tz.apply(naive),
                used_default_tz: true,
                year_inferred: false,
            });
        }
    }

    None
}

fn replace_first_space_with_t(raw: &str) -> String {
    if let Some(pos) = raw.find(' ') {
        let mut s = raw.to_string();
        s.replace_range(pos..pos + 1, "T");
        s
    } else {
        raw.to_string()
    }
}

/// Epoch magnitude thresholds distinguishing seconds / milliseconds / microseconds.
/// Judgement call, not a spec: seconds since epoch for dates in the plausible log
/// range (roughly 2001-09-09 onward) are 10 digits; multiplying by 1000 or 1e6 moves
/// the value into the next decade of magnitude, which is what these bounds check.
fn parse_epoch(raw: &str) -> Option<DateTime<Utc>> {
    if raw.is_empty() || !raw.bytes().all(|b| b.is_ascii_digit() || b == b'-') {
        return None;
    }
    let value: i64 = raw.parse().ok()?;
    let abs = value.unsigned_abs();
    if abs < 1_000_000_000 {
        return None; // too small to be a plausible epoch-seconds log timestamp
    }
    if abs < 100_000_000_000 {
        Utc.timestamp_opt(value, 0).single()
    } else if abs < 100_000_000_000_000 {
        Utc.timestamp_millis_opt(value).single()
    } else {
        DateTime::from_timestamp_micros(value)
    }
}

// ---------------------------------------------------------------------------
// Byte-scanner primitives. Every one of these advances only over ASCII it has
// matched, so an index derived from them is always a `str` char boundary.
// ---------------------------------------------------------------------------

fn take_digits(bytes: &[u8], i: usize, n: usize) -> Option<usize> {
    let end = i.checked_add(n)?;
    if end > bytes.len() || !bytes[i..end].iter().all(u8::is_ascii_digit) {
        return None;
    }
    Some(end)
}

fn take_byte(bytes: &[u8], i: usize, b: u8) -> Option<usize> {
    if bytes.get(i) == Some(&b) {
        Some(i + 1)
    } else {
        None
    }
}

/// Value of `n` digits known to be present at `i`.
fn digits_value(bytes: &[u8], i: usize, n: usize) -> u32 {
    bytes[i..i + n]
        .iter()
        .fold(0u32, |acc, b| acc * 10 + u32::from(b - b'0'))
}

const MONTHS: [&[u8; 3]; 12] = [
    b"JAN", b"FEB", b"MAR", b"APR", b"MAY", b"JUN", b"JUL", b"AUG", b"SEP", b"OCT", b"NOV", b"DEC",
];

/// Three-letter English month name at `i`, case-insensitive, 1-based.
fn month_name(bytes: &[u8], i: usize) -> Option<u32> {
    let name = bytes.get(i..i + 3)?;
    let upper = [
        name[0].to_ascii_uppercase(),
        name[1].to_ascii_uppercase(),
        name[2].to_ascii_uppercase(),
    ];
    MONTHS
        .iter()
        .position(|m| m[..] == upper[..])
        .map(|idx| idx as u32 + 1)
}

/// `HH:MM:SS`.
fn match_time(bytes: &[u8], i: usize) -> Option<usize> {
    let i = take_digits(bytes, i, 2)?;
    let i = take_byte(bytes, i, b':')?;
    let i = take_digits(bytes, i, 2)?;
    let i = take_byte(bytes, i, b':')?;
    take_digits(bytes, i, 2)
}

/// Optional `.` followed by one or more digits. Never fails: returns `i` unchanged
/// when there is no fraction.
fn match_fraction(bytes: &[u8], i: usize) -> usize {
    if bytes.get(i) != Some(&b'.') {
        return i;
    }
    let mut j = i + 1;
    while bytes.get(j).is_some_and(u8::is_ascii_digit) {
        j += 1;
    }
    if j > i + 1 {
        j
    } else {
        i
    }
}

/// Optional `Z` or `[+-]\d{2}:?\d{2}`. Never fails.
fn match_zone(bytes: &[u8], i: usize) -> usize {
    match bytes.get(i) {
        Some(b'Z') => i + 1,
        Some(b'+') | Some(b'-') => {
            let Some(mut j) = take_digits(bytes, i + 1, 2) else {
                return i;
            };
            if bytes.get(j) == Some(&b':') {
                j += 1;
            }
            take_digits(bytes, j, 2).unwrap_or(i)
        }
        _ => i,
    }
}

/// Fractional-second digits in `start..end` scaled to nanoseconds, zero-padded to
/// nine places and truncated beyond them (`.123456` → 123_456_000).
fn fraction_nanos(bytes: &[u8], start: usize, end: usize) -> u32 {
    let mut nanos = 0u32;
    for k in 0..9 {
        let digit = bytes
            .get(start + k)
            .filter(|_| start + k < end)
            .filter(|b| b.is_ascii_digit())
            .map_or(0, |b| u32::from(b - b'0'));
        nanos = nanos * 10 + digit;
    }
    nanos
}

// ---------------------------------------------------------------------------
// One matcher per shape. Each answers "does the whole pattern match starting at
// `start`?" and returns the end offset — requiring the *whole* pattern, never a
// prefix of it, is what keeps false positives inside the head window low (D2).
// ---------------------------------------------------------------------------

/// `\d{4}-\d{2}-\d{2}[T ]\d{2}:\d{2}:\d{2}(\.\d+)?(Z|[+-]\d{2}:?\d{2})?`
fn match_iso(bytes: &[u8], start: usize) -> Option<usize> {
    match_dated_time(bytes, start, b'-')
}

/// `\d{4}/\d{2}/\d{2}[T ]\d{2}:\d{2}:\d{2}(\.\d+)?(Z|[+-]\d{2}:?\d{2})?`
fn match_slash_date(bytes: &[u8], start: usize) -> Option<usize> {
    match_dated_time(bytes, start, b'/')
}

fn match_dated_time(bytes: &[u8], start: usize, sep: u8) -> Option<usize> {
    let i = take_digits(bytes, start, 4)?;
    let i = take_byte(bytes, i, sep)?;
    let i = take_digits(bytes, i, 2)?;
    let i = take_byte(bytes, i, sep)?;
    let i = take_digits(bytes, i, 2)?;
    let i = match bytes.get(i) {
        Some(b'T') | Some(b' ') => i + 1,
        _ => return None,
    };
    let i = match_time(bytes, i)?;
    let i = match_fraction(bytes, i);
    Some(match_zone(bytes, i))
}

/// klog/logcat: `[IWEFDV]?MMDD HH:MM:SS(.\d+)?`. The severity letter is matched here
/// rather than by the positional level scan because it sits *before* the timestamp,
/// where that scan cannot see it; `LeadingMatch::level_letter` hands it on (task 2.3).
fn match_klog(bytes: &[u8], start: usize) -> Option<(usize, Option<u8>)> {
    let (i, letter) = match bytes.get(start) {
        Some(&b) if crate::level::is_letter_code(b) => (start + 1, Some(b)),
        _ => (start, None),
    };
    let i = take_digits(bytes, i, 4)?;
    let i = take_byte(bytes, i, b' ')?;
    let i = match_time(bytes, i)?;
    Some((match_fraction(bytes, i), letter))
}

// --- logcat (`logcat-and-payload-precision` design.md D1/D2) -----------------

/// Longest accepted uid/pid/tid column. Real ones are a pid (`29149`), a uid
/// (`1000`) or a uid *name* (`root`, `system`, `webview_zygote`); the cap is what
/// stops the column skipper from wandering off down a long token.
const LOGCAT_COLUMN_MAX: usize = 16;

/// The severity letters logcat emits. `S` (silent) is here so the *shape* still
/// matches — a silent record is a record, and rejecting it would leave the line to
/// be misread by another shape — but `level::from_letter` deliberately does not map
/// it, because "emit nothing" is not a severity (design.md D2).
const LOGCAT_SEVERITIES: &[u8] = b"VDIWEFS";

fn is_logcat_severity(b: u8) -> bool {
    LOGCAT_SEVERITIES.contains(&b)
}

/// Everything `match_logcat` found, as byte ranges into the line. Ranges rather than
/// slices so the matcher stays on `&[u8]` like every other scanner here; `try_shape`
/// turns them into `&str` once, at the point where the line is in hand.
struct LogcatMatch {
    /// One past the tag's colon.
    end: usize,
    letter: u8,
    columns: [(usize, usize); 3],
    /// 2 (`pid tid`) or 3 (`uid pid tid`) — the layouts observed in a real bugreport.
    column_count: usize,
    tag: (usize, usize),
}

/// One or more spaces followed by a byte that is not one. `None` at end of line, so a
/// truncated record cannot be mistaken for a complete one.
fn skip_spaces(bytes: &[u8], i: usize) -> Option<usize> {
    let mut j = i;
    while bytes.get(j) == Some(&b' ') {
        j += 1;
    }
    if j == i || j >= bytes.len() {
        return None;
    }
    Some(j)
}

/// Android's per-app uid column: `u0_a2`, `u10_a137` — `u`, digits, `_a`, digits, and
/// nothing else.
fn is_app_uid(token: &[u8]) -> bool {
    if token.first() != Some(&b'u') {
        return false;
    }
    let mut i = 1;
    let digits_start = i;
    while token.get(i).is_some_and(u8::is_ascii_digit) {
        i += 1;
    }
    if i == digits_start || token.get(i) != Some(&b'_') || token.get(i + 1) != Some(&b'a') {
        return false;
    }
    i += 2;
    let digits_start = i;
    while token.get(i).is_some_and(u8::is_ascii_digit) {
        i += 1;
    }
    i > digits_start && i == token.len()
}

/// One uid/pid/tid column: all digits (`1000`, `29149`), a short lowercase word
/// (`root`, `shell`, `webview_zygote`), or the `u0_aNN` form. Uppercase is
/// deliberately excluded from every alternative — that is what makes the severity
/// letter unambiguously *not* a column, so the run of columns terminates on it
/// without needing a lookahead vocabulary.
fn match_logcat_column(bytes: &[u8], start: usize) -> Option<usize> {
    let mut i = start;
    while bytes.get(i).is_some_and(|b| *b != b' ' && *b != b'\t') {
        i += 1;
    }
    let token = bytes.get(start..i)?;
    if token.is_empty() || token.len() > LOGCAT_COLUMN_MAX {
        return None;
    }
    let accepted = token.iter().all(u8::is_ascii_digit)
        || token.iter().all(|b| b.is_ascii_lowercase() || *b == b'_')
        || is_app_uid(token);
    accepted.then_some(i)
}

/// The tag and its terminating colon, as `(tag_end, after_colon)`. The tag carries no
/// whitespace and no colon of its own, which is what stops the scan at
/// `UsbDescriptorParser:` on a record whose *message* also contains a colon
/// (`Unrecognized len: 58`).
fn match_logcat_tag(bytes: &[u8], start: usize) -> Option<(usize, usize)> {
    let mut i = start;
    while bytes
        .get(i)
        .is_some_and(|b| *b != b':' && *b != b' ' && *b != b'\t')
    {
        i += 1;
    }
    if i == start || bytes.get(i) != Some(&b':') {
        return None;
    }
    Some((i, i + 1))
}

/// logcat: `MM-DD hh:mm:ss.mmm`, two or three uid/pid/tid columns, a severity letter,
/// and a tag terminated by `:`.
///
/// The whole sequence through that colon must match before anything is consumed
/// (design.md D1). The tempting implementation — match the timestamp, then skip
/// number-like tokens — is the dangerous one: an unbounded skip eventually swallows
/// the head of an ordinary line and hands whatever follows to the level matcher. The
/// `Tag:` anchor means a line that merely opens with a date and some numbers is
/// rejected outright and left to the other shapes.
fn match_logcat(bytes: &[u8], start: usize) -> Option<LogcatMatch> {
    let i = take_digits(bytes, start, 2)?;
    let i = take_byte(bytes, i, b'-')?;
    let i = take_digits(bytes, i, 2)?;
    let i = take_byte(bytes, i, b' ')?;
    let i = match_time(bytes, i)?;
    // The fraction is required: `MM-DD hh:mm:ss` on its own is too weak a head to
    // hang the rest of the shape on, and logcat always emits milliseconds.
    let after_fraction = match_fraction(bytes, i);
    if after_fraction == i {
        return None;
    }
    let mut i = after_fraction;

    let mut columns = [(0usize, 0usize); 3];
    let mut column_count = 0usize;
    let letter = loop {
        let j = skip_spaces(bytes, i)?;
        // A single severity letter followed by a space ends the run. Only checked
        // once two columns are in hand, so `pid tid` is the minimum layout.
        if column_count >= 2 && is_logcat_severity(bytes[j]) && bytes.get(j + 1) == Some(&b' ') {
            i = j + 1;
            break bytes[j];
        }
        if column_count == 3 {
            return None; // four columns is not a layout logcat emits
        }
        let end = match_logcat_column(bytes, j)?;
        columns[column_count] = (j, end);
        column_count += 1;
        i = end;
    };

    let tag_start = skip_spaces(bytes, i)?;
    let (tag_end, after_colon) = match_logcat_tag(bytes, tag_start)?;
    Some(LogcatMatch {
        end: after_colon,
        letter,
        columns,
        column_count,
        tag: (tag_start, tag_end),
    })
}

/// syslog RFC 3164: `MMM [ ]?\d{1,2} HH:MM:SS(.\d+)?` — `Aug  8` (space-padded) and
/// `Aug 08` are both in the wild, so both are accepted.
fn match_syslog_3164(bytes: &[u8], start: usize) -> Option<usize> {
    month_name(bytes, start)?;
    let mut i = start + 3;
    let mut spaces = 0;
    while spaces < 2 && bytes.get(i) == Some(&b' ') {
        i += 1;
        spaces += 1;
    }
    if spaces == 0 {
        return None;
    }
    let day_start = i;
    while i - day_start < 2 && bytes.get(i).is_some_and(u8::is_ascii_digit) {
        i += 1;
    }
    if i == day_start || bytes.get(i).is_some_and(u8::is_ascii_digit) {
        return None; // no day, or a third digit — not a day of the month
    }
    let i = take_byte(bytes, i, b' ')?;
    let i = match_time(bytes, i)?;
    Some(match_fraction(bytes, i))
}

/// Apache/CLF: `\d{2}/MMM/\d{4}:\d{2}:\d{2}:\d{2}( [+-]\d{4})?`.
fn match_clf(bytes: &[u8], start: usize) -> Option<usize> {
    let i = take_digits(bytes, start, 2)?;
    let i = take_byte(bytes, i, b'/')?;
    month_name(bytes, i)?;
    let i = take_byte(bytes, i + 3, b'/')?;
    let i = take_digits(bytes, i, 4)?;
    let i = take_byte(bytes, i, b':')?;
    let i = match_time(bytes, i)?;
    if bytes.get(i) == Some(&b' ') && matches!(bytes.get(i + 1), Some(b'+') | Some(b'-')) {
        if let Some(end) = take_digits(bytes, i + 2, 4) {
            return Some(end);
        }
    }
    Some(i)
}

/// A *leading* integer epoch value (task 2.6): 10–19 digits at offset 0, terminated
/// by whitespace or end of line. Restricted to offset 0 and to a whole token because
/// a bare number anywhere else is the classic false-positive timestamp (D2's residual
/// risk); the magnitude rules are `parse_epoch`'s, not duplicated here.
fn match_epoch(bytes: &[u8], start: usize) -> Option<usize> {
    if start != 0 {
        return None;
    }
    let mut i = 0;
    while bytes.get(i).is_some_and(u8::is_ascii_digit) {
        i += 1;
    }
    if !(10..=19).contains(&i) {
        return None;
    }
    match bytes.get(i) {
        None | Some(b' ') | Some(b'\t') => Some(i),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Parsing a matched slice into an instant.
// ---------------------------------------------------------------------------

/// The year-less shapes (syslog 3164, klog) get their year from the caller's
/// reference instant, stepped back one when the reference year would date the entry
/// in the future (design.md D4). Returns `None` when no reference was supplied —
/// inventing a year silently is the failure mode this whole flag exists to prevent.
fn resolve_inferred_year(
    month: u32,
    day: u32,
    hour: u32,
    minute: u32,
    second: u32,
    nanos: u32,
    ctx: &ParseContext,
) -> Option<ParsedTimestamp> {
    let reference = ctx.reference()?;
    let build = |year: i32| -> Option<DateTime<Utc>> {
        let naive = NaiveDate::from_ymd_opt(year, month, day)?
            .and_hms_nano_opt(hour, minute, second, nanos)?;
        Some(ctx.tz.apply(naive))
    };
    let instant = match build(reference.year()) {
        // Dating the entry after "now" means the log wrapped a new year: December
        // lines read in January belong to the previous year.
        Some(candidate) if candidate <= reference => candidate,
        // `None` covers 29 February against a non-leap reference year, which is the
        // same answer: the entry belongs to the year before.
        _ => build(reference.year() - 1)?,
    };
    Some(ParsedTimestamp {
        instant,
        used_default_tz: true,
        year_inferred: true,
    })
}

fn parse_syslog_3164(text: &str, ctx: &ParseContext) -> Option<ParsedTimestamp> {
    let b = text.as_bytes();
    let month = month_name(b, 0)?;
    let mut i = 3;
    while b.get(i) == Some(&b' ') {
        i += 1;
    }
    let day_start = i;
    while b.get(i).is_some_and(u8::is_ascii_digit) {
        i += 1;
    }
    let day = digits_value(b, day_start, i - day_start);
    i += 1; // the single space before the time
    let (hour, minute, second) = (
        digits_value(b, i, 2),
        digits_value(b, i + 3, 2),
        digits_value(b, i + 6, 2),
    );
    let nanos = if b.get(i + 8) == Some(&b'.') {
        fraction_nanos(b, i + 9, b.len())
    } else {
        0
    };
    resolve_inferred_year(month, day, hour, minute, second, nanos, ctx)
}

fn parse_klog(text: &str, ctx: &ParseContext) -> Option<ParsedTimestamp> {
    let b = text.as_bytes();
    let i = if b[0].is_ascii_alphabetic() { 1 } else { 0 };
    let month = digits_value(b, i, 2);
    let day = digits_value(b, i + 2, 2);
    let t = i + 5; // 4 digits + the space
    let (hour, minute, second) = (
        digits_value(b, t, 2),
        digits_value(b, t + 3, 2),
        digits_value(b, t + 6, 2),
    );
    let nanos = if b.get(t + 8) == Some(&b'.') {
        fraction_nanos(b, t + 9, b.len())
    } else {
        0
    };
    resolve_inferred_year(month, day, hour, minute, second, nanos, ctx)
}

/// logcat's `MM-DD hh:mm:ss.mmm`. Fixed-width up to the fraction, so fixed indices;
/// `text` is the *whole* matched record (through the tag's colon), of which only the
/// leading timestamp is read here. Year-less like syslog 3164 and klog, so it goes
/// through the same caller-supplied reference instant (design.md D3) and comes back
/// flagged `year_inferred` — a bugreport older than a year dates wrong and says so.
fn parse_logcat(text: &str, ctx: &ParseContext) -> Option<ParsedTimestamp> {
    let b = text.as_bytes();
    let month = digits_value(b, 0, 2);
    let day = digits_value(b, 3, 2);
    let (hour, minute, second) = (
        digits_value(b, 6, 2),
        digits_value(b, 9, 2),
        digits_value(b, 12, 2),
    );
    // The matcher required a fraction, so index 15 is its first digit.
    let mut end = 15;
    while b.get(end).is_some_and(u8::is_ascii_digit) {
        end += 1;
    }
    let nanos = fraction_nanos(b, 15, end);
    resolve_inferred_year(month, day, hour, minute, second, nanos, ctx)
}

/// `08/Aug/2026:17:42:01 +0000` — fixed layout, so fixed indices.
fn parse_clf(text: &str, ctx: &ParseContext) -> Option<ParsedTimestamp> {
    let b = text.as_bytes();
    let day = digits_value(b, 0, 2);
    let month = month_name(b, 3)?;
    let year = digits_value(b, 7, 4) as i32;
    let naive = NaiveDate::from_ymd_opt(year, month, day)?.and_hms_opt(
        digits_value(b, 12, 2),
        digits_value(b, 15, 2),
        digits_value(b, 18, 2),
    )?;
    if b.len() >= 26 && (b[21] == b'+' || b[21] == b'-') {
        let minutes = (digits_value(b, 22, 2) * 60 + digits_value(b, 24, 2)) as i32;
        let seconds = if b[21] == b'-' {
            -minutes * 60
        } else {
            minutes * 60
        };
        let offset = FixedOffset::east_opt(seconds)?;
        return Some(ParsedTimestamp {
            instant: offset
                .from_local_datetime(&naive)
                .earliest()?
                .with_timezone(&Utc),
            used_default_tz: false,
            year_inferred: false,
        });
    }
    Some(ParsedTimestamp {
        instant: ctx.tz.apply(naive),
        used_default_tz: true,
        year_inferred: false,
    })
}

/// `2026/08/08 17:42:01` reuses `parse_value` by normalising its date separators —
/// the rest of the grammar (space-or-`T`, fraction, offset) is identical to ISO.
fn parse_slash_date(text: &str, ctx: &ParseContext) -> Option<ParsedTimestamp> {
    let normalised: String = text
        .chars()
        .map(|c| if c == '/' { '-' } else { c })
        .collect();
    parse_value(&normalised, &ctx.tz)
}

// ---------------------------------------------------------------------------
// The head-window sweep.
// ---------------------------------------------------------------------------

/// The non-timestamp parts of a recognised logcat record (`logcat-and-payload-precision`
/// design.md D1). They ride on `LeadingMatch` because the shape's own scanner is the
/// only thing that can see them: they sit *between* the timestamp and the message,
/// where neither the positional level matcher nor a payload parser would look, and
/// they have to leave the message text (field-extraction spec, "Columns leave the
/// message").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LogcatRecord<'a> {
    /// Present only in the three-column `uid pid tid` layout; `None` for `pid tid`.
    /// Two columns versus three is the whole distinction, and it is decidable from
    /// the count alone.
    pub uid: Option<&'a str>,
    pub pid: &'a str,
    pub tid: &'a str,
    pub tag: &'a str,
}

/// A recognised timestamp prefix: what matched, where, and what is left of the line.
#[derive(Debug)]
pub struct LeadingMatch<'a> {
    pub parsed: ParsedTimestamp,
    pub shape: TimestampShape,
    /// Byte offset of the match within the line.
    pub offset: usize,
    /// Unrecognised text before the timestamp (`1.2.3.4 - -` of an access-log line),
    /// with any bracket that opened the timestamp removed. Empty for offset 0.
    pub head: &'a str,
    /// The line after the timestamp (and its closing bracket), trimmed.
    pub rest: &'a str,
    /// As `rest`, but with the leading whitespace left in place. The continuation
    /// recognizers need it: an indented logcat message is one of the signals that a
    /// re-stamped line continues the one above it (`multiline-entry-continuations`
    /// design D3, R2), and trimming would erase exactly that evidence.
    pub rest_raw: &'a str,
    /// klog/logcat severity letter that was part of the match (`I0808 …`, or the
    /// letter sitting after logcat's columns).
    pub level_letter: Option<u8>,
    /// Columns and tag, for a logcat record only.
    pub logcat: Option<LogcatRecord<'a>>,
}

fn is_token_boundary(b: u8) -> bool {
    b == b' ' || b == b'\t' || b == b'[' || b == b'(' || b == b'<'
}

fn closer_for(opener: u8) -> Option<u8> {
    match opener {
        b'[' => Some(b']'),
        b'(' => Some(b')'),
        b'<' => Some(b'>'),
        _ => None,
    }
}

/// Match `shape` at `offset` and parse what it matched. A match that does not parse
/// (`2026-13-45T…`, klog `2026 17:42:01` whose "month" is 20) is not a timestamp, so
/// the sweep continues rather than giving up on the line.
fn try_shape<'a>(
    line: &'a str,
    offset: usize,
    shape: TimestampShape,
    ctx: &ParseContext,
) -> Option<LeadingMatch<'a>> {
    let bytes = line.as_bytes();
    let (end, level_letter, logcat) = match shape {
        TimestampShape::Iso => (match_iso(bytes, offset)?, None, None),
        TimestampShape::SlashDate => (match_slash_date(bytes, offset)?, None, None),
        TimestampShape::Klog => {
            let (end, letter) = match_klog(bytes, offset)?;
            (end, letter, None)
        }
        TimestampShape::Logcat => {
            let found = match_logcat(bytes, offset)?;
            (found.end, Some(found.letter), Some(found))
        }
        TimestampShape::Syslog3164 => (match_syslog_3164(bytes, offset)?, None, None),
        TimestampShape::Clf => (match_clf(bytes, offset)?, None, None),
        TimestampShape::Epoch => (match_epoch(bytes, offset)?, None, None),
    };

    let matched = &line[offset..end];
    let parsed = match shape {
        TimestampShape::Iso | TimestampShape::Epoch => parse_value(matched, &ctx.tz)?,
        TimestampShape::SlashDate => parse_slash_date(matched, ctx)?,
        // The severity letter is not part of the value.
        TimestampShape::Klog => parse_klog(matched, ctx)?,
        TimestampShape::Logcat => parse_logcat(matched, ctx)?,
        TimestampShape::Syslog3164 => parse_syslog_3164(matched, ctx)?,
        TimestampShape::Clf => parse_clf(matched, ctx)?,
    };

    // Column/tag ranges become slices only now, once the line itself is in hand. Both
    // ends came out of a byte scanner that advanced only over matched ASCII, so every
    // one is on a `str` char boundary.
    let logcat = logcat.map(|found| {
        let slice = |(start, end): (usize, usize)| &line[start..end];
        LogcatRecord {
            uid: (found.column_count == 3).then(|| slice(found.columns[0])),
            pid: slice(found.columns[found.column_count - 2]),
            tid: slice(found.columns[found.column_count - 1]),
            tag: slice(found.tag),
        }
    });

    // A timestamp opened by `[`/`(`/`<` takes its closer with it, so the message does
    // not start with a stray bracket.
    let opener = offset.checked_sub(1).map(|p| bytes[p]);
    let mut head_end = offset;
    let mut rest_start = end;
    if let Some(closer) = opener.and_then(closer_for) {
        if bytes.get(end) == Some(&closer) {
            head_end = offset - 1;
            rest_start = end + 1;
        }
    }

    Some(LeadingMatch {
        parsed,
        shape,
        offset,
        head: line[..head_end].trim(),
        rest: line[rest_start..].trim_start(),
        rest_raw: &line[rest_start..],
        level_letter,
        logcat,
    })
}

/// Candidate match positions: offset 0, plus every position inside the head window
/// preceded by whitespace or an opening bracket (design.md D2). Only token starts are
/// tried, which is what keeps a number in the middle of a word from being read as a
/// timestamp.
fn token_starts(bytes: &[u8]) -> impl Iterator<Item = usize> + '_ {
    let limit = HEAD_WINDOW.min(bytes.len());
    (0..limit).filter(move |&i| i == 0 || is_token_boundary(bytes[i - 1]))
}

/// Find the leading timestamp of `line`, if any.
///
/// Offsets are always swept in ascending order and the first full match wins, with or
/// without a sticky hint — that is what makes the hint result-neutral (format-detection
/// spec, "Sticky choice does not change the result"). The hint reorders the *shapes*
/// tried at each offset, so a uniform input costs one match attempt at one offset
/// instead of up to six; it deliberately does not let the scanner skip ahead to the
/// remembered offset, because an earlier offset on some later line could hold a match
/// the sweep would have found first, and D3's speedup is not worth changing results
/// for. (For the common hint offset of 0 the two are the same thing anyway: 0 is
/// always the first candidate.)
pub fn extract_leading<'a>(line: &'a str, ctx: &ParseContext) -> Option<LeadingMatch<'a>> {
    let bytes = line.as_bytes();
    let hint = ctx.prefix_hint().map(|(shape, _)| shape);

    for offset in token_starts(bytes) {
        if let Some(shape) = hint {
            if let Some(matched) = try_shape(line, offset, shape, ctx) {
                ctx.note_sticky_hit();
                return Some(matched);
            }
        }
        for shape in SHAPES {
            if Some(shape) == hint {
                continue;
            }
            if let Some(matched) = try_shape(line, offset, shape, ctx) {
                ctx.note_sticky_miss(matched.shape, matched.offset);
                return Some(matched);
            }
        }
    }
    None
}

/// Whether `line` carries a recognisable leading timestamp, for detection's
/// prefix-match fraction (format-detection spec, "Recognised prefixes are a match").
/// Reports the winning shape and offset so detection can record them.
pub fn prefix_shape(line: &str, ctx: &ParseContext) -> Option<(TimestampShape, usize)> {
    extract_leading(line, ctx).map(|m| (m.shape, m.offset))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx_at(reference: &str) -> ParseContext {
        ParseContext::utc().with_reference(
            DateTime::parse_from_rfc3339(reference)
                .unwrap()
                .with_timezone(&Utc),
        )
    }

    #[test]
    fn offset_is_respected() {
        let parsed = parse_value("2026-08-08T17:42:01+03:00", &TimeZonePolicy::utc()).unwrap();
        assert!(!parsed.used_default_tz);
        assert_eq!(parsed.instant.to_rfc3339(), "2026-08-08T14:42:01+00:00");
    }

    #[test]
    fn naive_defaults_to_utc() {
        let parsed = parse_value("2026-08-08 17:42:01", &TimeZonePolicy::utc()).unwrap();
        assert!(parsed.used_default_tz);
        assert_eq!(parsed.instant.to_rfc3339(), "2026-08-08T17:42:01+00:00");
    }

    #[test]
    fn caller_supplied_fixed_offset() {
        let tz = TimeZonePolicy::fixed_offset(FixedOffset::east_opt(2 * 3600).unwrap());
        let parsed = parse_value("2026-08-08 17:42:01", &tz).unwrap();
        assert!(parsed.used_default_tz);
        assert_eq!(parsed.instant.to_rfc3339(), "2026-08-08T15:42:01+00:00");
    }

    #[test]
    fn epoch_seconds() {
        let parsed = parse_value("1754668800", &TimeZonePolicy::utc()).unwrap();
        assert!(!parsed.used_default_tz);
        assert_eq!(parsed.instant.timestamp(), 1_754_668_800);
    }

    #[test]
    fn epoch_milliseconds() {
        let parsed = parse_value("1754668800000", &TimeZonePolicy::utc()).unwrap();
        assert_eq!(parsed.instant.timestamp(), 1_754_668_800);
    }

    #[test]
    fn epoch_microseconds() {
        let parsed = parse_value("1754668800000000", &TimeZonePolicy::utc()).unwrap();
        assert_eq!(parsed.instant.timestamp(), 1_754_668_800);
    }

    #[test]
    fn leading_timestamp_in_plain_text() {
        let matched = extract_leading(
            "2026-08-08T17:42:01Z something happened",
            &ParseContext::utc(),
        )
        .unwrap();
        assert_eq!(matched.shape, TimestampShape::Iso);
        assert!(!matched.parsed.used_default_tz);
        assert!(!matched.parsed.year_inferred);
        assert_eq!(matched.rest, "something happened");
        assert_eq!(matched.head, "");
    }

    #[test]
    fn no_leading_timestamp() {
        assert!(extract_leading("nothing here", &ParseContext::utc()).is_none());
    }

    #[test]
    fn garbage_is_not_a_timestamp() {
        assert!(parse_value("yesterday", &TimeZonePolicy::utc()).is_none());
    }

    // --- shapes (task 2.8) ---------------------------------------------------

    #[test]
    fn syslog_3164_padded_day() {
        let ctx = ctx_at("2026-08-20T00:00:00Z");
        let matched = extract_leading("Aug  8 17:42:01 host app[123]: boom", &ctx).unwrap();
        assert_eq!(matched.shape, TimestampShape::Syslog3164);
        assert_eq!(
            matched.parsed.instant.to_rfc3339(),
            "2026-08-08T17:42:01+00:00"
        );
        assert!(matched.parsed.year_inferred);
        assert_eq!(matched.rest, "host app[123]: boom");
    }

    #[test]
    fn syslog_3164_two_digit_day() {
        let ctx = ctx_at("2026-08-20T00:00:00Z");
        let matched = extract_leading("Aug 08 17:42:01 host app: boom", &ctx).unwrap();
        assert_eq!(matched.shape, TimestampShape::Syslog3164);
        assert_eq!(
            matched.parsed.instant.to_rfc3339(),
            "2026-08-08T17:42:01+00:00"
        );
    }

    #[test]
    fn klog_with_severity_letter() {
        let ctx = ctx_at("2026-08-20T00:00:00Z");
        let matched =
            extract_leading("I0808 17:42:01.123456       1 main.go:10] starting", &ctx).unwrap();
        assert_eq!(matched.shape, TimestampShape::Klog);
        assert_eq!(matched.level_letter, Some(b'I'));
        assert_eq!(
            matched.parsed.instant.to_rfc3339(),
            "2026-08-08T17:42:01.123456+00:00"
        );
        assert_eq!(matched.rest, "1 main.go:10] starting");
    }

    #[test]
    fn klog_without_severity_letter() {
        let ctx = ctx_at("2026-08-20T00:00:00Z");
        let matched = extract_leading("0808 17:42:01.123456 starting", &ctx).unwrap();
        assert_eq!(matched.shape, TimestampShape::Klog);
        assert_eq!(matched.level_letter, None);
    }

    #[test]
    fn clf_timestamp_inside_brackets_past_offset_zero() {
        let matched = extract_leading(
            r#"1.2.3.4 - - [08/Aug/2026:17:42:01 +0000] "GET /x HTTP/1.1" 500 12"#,
            &ParseContext::utc(),
        )
        .unwrap();
        assert_eq!(matched.shape, TimestampShape::Clf);
        assert_eq!(matched.offset, 13);
        // The `+0000` offset is part of the match, so no default timezone is applied.
        assert!(!matched.parsed.used_default_tz);
        assert_eq!(
            matched.parsed.instant.to_rfc3339(),
            "2026-08-08T17:42:01+00:00"
        );
        assert_eq!(matched.head, "1.2.3.4 - -");
        assert_eq!(matched.rest, r#""GET /x HTTP/1.1" 500 12"#);
    }

    #[test]
    fn clf_explicit_offset_is_applied() {
        let matched = extract_leading(
            "1.2.3.4 [08/Aug/2026:17:42:01 -0300] GET",
            &ParseContext::utc(),
        )
        .unwrap();
        assert_eq!(
            matched.parsed.instant.to_rfc3339(),
            "2026-08-08T20:42:01+00:00"
        );
    }

    #[test]
    fn slash_date_shape() {
        let matched = extract_leading(
            "2026/08/08 17:42:01 something happened",
            &ParseContext::utc(),
        )
        .unwrap();
        assert_eq!(matched.shape, TimestampShape::SlashDate);
        assert_eq!(
            matched.parsed.instant.to_rfc3339(),
            "2026-08-08T17:42:01+00:00"
        );
        assert!(matched.parsed.used_default_tz);
        assert_eq!(matched.rest, "something happened");
    }

    #[test]
    fn leading_epoch_value() {
        let matched = extract_leading("1754668800 worker started", &ParseContext::utc()).unwrap();
        assert_eq!(matched.shape, TimestampShape::Epoch);
        assert_eq!(matched.parsed.instant.timestamp(), 1_754_668_800);
        assert_eq!(matched.rest, "worker started");
    }

    #[test]
    fn epoch_is_only_accepted_at_the_start_of_the_line() {
        // The same value one token in is not a leading epoch value.
        assert!(extract_leading("worker 1754668800 started", &ParseContext::utc()).is_none());
    }

    // --- logcat (`logcat-and-payload-precision` tasks 2.3, 2.6) --------------

    /// The safety property (design D1), written before the feature: a line that opens
    /// with a logcat timestamp and numeric columns but carries no `Tag:` must not be
    /// consumed at all. If it were, the column skipper would hand whatever follows to
    /// the level matcher on every date-and-numbers line in the wild.
    #[test]
    fn logcat_without_a_tag_is_not_consumed() {
        let ctx = ctx_at("2026-08-20T00:00:00Z");
        assert!(extract_leading("04-21 13:07:51.985   806 29149 W", &ctx).is_none());
        // …nor when something non-tag-shaped follows the letter.
        assert!(extract_leading("04-21 13:07:51.985   806 29149 W no colon here", &ctx).is_none());
        // …nor with a trailing space where the tag would be.
        assert!(extract_leading("04-21 13:07:51.985   806 29149 W ", &ctx).is_none());
        // …nor when the "tag" is empty.
        assert!(extract_leading("04-21 13:07:51.985   806 29149 W : boom", &ctx).is_none());
    }

    #[test]
    fn logcat_three_columns_with_a_numeric_uid() {
        let ctx = ctx_at("2026-08-20T00:00:00Z");
        let matched = extract_leading(
            "04-18 19:21:16.151  1000   806   995 D ActivityManager: freezing 2521 com.x",
            &ctx,
        )
        .unwrap();
        assert_eq!(matched.shape, TimestampShape::Logcat);
        assert_eq!(matched.level_letter, Some(b'D'));
        assert_eq!(
            matched.parsed.instant.to_rfc3339(),
            "2026-04-18T19:21:16.151+00:00"
        );
        assert!(matched.parsed.year_inferred);
        let record = matched.logcat.unwrap();
        assert_eq!(record.uid, Some("1000"));
        assert_eq!(record.pid, "806");
        assert_eq!(record.tid, "995");
        assert_eq!(record.tag, "ActivityManager");
        assert_eq!(matched.rest, "freezing 2521 com.x");
    }

    #[test]
    fn logcat_two_columns() {
        let ctx = ctx_at("2026-08-20T00:00:00Z");
        let matched = extract_leading(
            "04-21 13:07:51.985   806 29149 W UsbDescriptorParser: Unrecognized len: 58",
            &ctx,
        )
        .unwrap();
        assert_eq!(matched.shape, TimestampShape::Logcat);
        assert_eq!(matched.level_letter, Some(b'W'));
        let record = matched.logcat.unwrap();
        assert_eq!(record.uid, None);
        assert_eq!(record.pid, "806");
        assert_eq!(record.tid, "29149");
        // The tag stops at its own colon, not at the one in the message.
        assert_eq!(record.tag, "UsbDescriptorParser");
        assert_eq!(matched.rest, "Unrecognized len: 58");
    }

    #[test]
    fn logcat_named_and_app_uid_columns() {
        let ctx = ctx_at("2026-08-20T00:00:00Z");
        for (line, uid) in [
            (
                "04-21 12:57:16.290  root   511   614 D vhdnativeservice: cmd: dumpsys cpuinfo",
                "root",
            ),
            (
                "04-21 13:07:39.761 u0_a2  1906  1915 I d.process.medi: Thread[6]",
                "u0_a2",
            ),
            (
                "04-21 13:07:39.761 webview_zygote 1906 1915 I Zygote: forked",
                "webview_zygote",
            ),
        ] {
            let matched = extract_leading(line, &ctx).unwrap();
            assert_eq!(matched.shape, TimestampShape::Logcat, "{line}");
            assert_eq!(matched.logcat.unwrap().uid, Some(uid), "{line}");
        }
    }

    #[test]
    fn logcat_tag_may_contain_dots() {
        let ctx = ctx_at("2026-08-20T00:00:00Z");
        let matched = extract_leading(
            "04-21 13:07:39.761 u0_a2  1906  1915 I d.process.medi: Thread[6]",
            &ctx,
        )
        .unwrap();
        assert_eq!(matched.logcat.unwrap().tag, "d.process.medi");
    }

    #[test]
    fn logcat_silent_severity_matches_the_shape_but_maps_to_no_level() {
        let ctx = ctx_at("2026-08-20T00:00:00Z");
        let matched = extract_leading(
            "04-21 13:07:53.198  1000   806   806 S SilentTag: nothing to report here",
            &ctx,
        )
        .unwrap();
        assert_eq!(matched.level_letter, Some(b'S'));
        assert_eq!(crate::level::from_letter(b'S'), None);
    }

    #[test]
    fn logcat_rejects_shapes_it_does_not_emit() {
        let ctx = ctx_at("2026-08-20T00:00:00Z");
        // One column only.
        assert!(extract_leading("04-21 13:07:51.985 806 W Tag: boom", &ctx).is_none());
        // Four columns.
        assert!(extract_leading("04-21 13:07:51.985 1 2 3 4 W Tag: boom", &ctx).is_none());
        // No fractional seconds.
        assert!(extract_leading("04-21 13:07:51 806 29149 W Tag: boom", &ctx).is_none());
        // An uppercase column would be indistinguishable from the severity letter.
        assert!(extract_leading("04-21 13:07:51.985 ROOT 29149 W Tag: boom", &ctx).is_none());
        // A severity letter that is not one.
        assert!(extract_leading("04-21 13:07:51.985 806 29149 Q Tag: boom", &ctx).is_none());
        // Not a real date.
        assert!(extract_leading("04-99 13:07:51.985 806 29149 W Tag: boom", &ctx).is_none());
    }

    #[test]
    fn logcat_needs_a_reference_instant_like_every_other_year_less_shape() {
        assert!(extract_leading(
            "04-18 19:21:16.151  1000   806   995 D ActivityManager: boom",
            &ParseContext::utc()
        )
        .is_none());
    }

    #[test]
    fn logcat_does_not_collide_with_the_other_shapes() {
        let ctx = ctx_at("2026-08-20T00:00:00Z");
        // Every other shape still wins on its own input with logcat in the sweep.
        for (line, shape) in [
            ("2026-08-08T17:42:01Z x", TimestampShape::Iso),
            ("2026/08/08 17:42:01 x", TimestampShape::SlashDate),
            ("I0808 17:42:01.123 x", TimestampShape::Klog),
            ("Aug  8 17:42:01 host x", TimestampShape::Syslog3164),
            ("1754668800 x", TimestampShape::Epoch),
        ] {
            assert_eq!(extract_leading(line, &ctx).unwrap().shape, shape, "{line}");
        }
    }

    // --- negatives (task 2.8) ------------------------------------------------

    #[test]
    fn date_like_number_past_the_head_window_is_not_a_timestamp() {
        let padding = "x".repeat(HEAD_WINDOW);
        let line = format!("{padding} 2026-08-08T17:42:01Z late");
        assert!(extract_leading(&line, &ParseContext::utc()).is_none());
    }

    #[test]
    fn partial_match_is_rejected() {
        // Date but no time: the whole pattern must match, not a prefix of it.
        assert!(extract_leading("2026-08-08 nothing else", &ParseContext::utc()).is_none());
        // Truncated time.
        assert!(extract_leading("2026-08-08T17:42 nothing else", &ParseContext::utc()).is_none());
        // Month name but no time.
        let ctx = ctx_at("2026-08-20T00:00:00Z");
        assert!(extract_leading("Aug  8 something", &ctx).is_none());
    }

    #[test]
    fn match_inside_a_word_is_not_tried() {
        // `v2026-08-08T17:42:01Z` starts mid-token, so no candidate offset covers it.
        assert!(extract_leading("v2026-08-08T17:42:01Z build", &ParseContext::utc()).is_none());
    }

    #[test]
    fn impossible_date_is_not_a_timestamp() {
        assert!(extract_leading("2026-13-45T17:42:01Z boom", &ParseContext::utc()).is_none());
    }

    #[test]
    fn year_less_shape_needs_a_reference_instant() {
        // No reference supplied: the shape matches but is not dated by invention.
        assert!(extract_leading("Aug  8 17:42:01 host app: boom", &ParseContext::utc()).is_none());
    }

    #[test]
    fn multibyte_head_does_not_break_slicing() {
        let ctx = ctx_at("2026-08-20T00:00:00Z");
        let matched = extract_leading("привет 2026-08-08T17:42:01Z tail", &ctx).unwrap();
        assert_eq!(matched.head, "привет");
        assert_eq!(matched.rest, "tail");
    }

    // --- inferred year (task 3.4) -------------------------------------------

    #[test]
    fn december_line_read_in_january_dates_to_the_previous_year() {
        let ctx = ctx_at("2026-01-05T00:00:00Z");
        let matched = extract_leading("Dec 31 23:59:59 host app: boom", &ctx).unwrap();
        assert_eq!(
            matched.parsed.instant.to_rfc3339(),
            "2025-12-31T23:59:59+00:00"
        );
        assert!(matched.parsed.year_inferred);
    }

    #[test]
    fn january_line_read_in_december_keeps_the_reference_year() {
        let ctx = ctx_at("2026-12-31T23:00:00Z");
        let matched = extract_leading("Jan  2 00:00:01 host app: boom", &ctx).unwrap();
        assert_eq!(
            matched.parsed.instant.to_rfc3339(),
            "2026-01-02T00:00:01+00:00"
        );
        assert!(matched.parsed.year_inferred);
    }

    #[test]
    fn timestamp_carrying_its_own_year_is_not_flagged() {
        let ctx = ctx_at("2026-08-20T00:00:00Z");
        let matched = extract_leading("2026-08-08T17:42:01Z boom", &ctx).unwrap();
        assert!(!matched.parsed.year_inferred);
    }

    #[test]
    fn leap_day_falls_back_to_the_previous_year_when_the_reference_year_has_none() {
        let ctx = ctx_at("2029-06-01T00:00:00Z"); // 2029 is not a leap year, 2028 is
        let matched = extract_leading("Feb 29 12:00:00 host app: boom", &ctx).unwrap();
        assert_eq!(
            matched.parsed.instant.to_rfc3339(),
            "2028-02-29T12:00:00+00:00"
        );
    }

    // --- sticky selection (design D3) ---------------------------------------

    #[test]
    fn sticky_hint_is_adopted_and_then_costs_no_misses() {
        let ctx = ParseContext::utc();
        ctx.set_prefix_hint(TimestampShape::Iso, 0);
        for _ in 0..10 {
            extract_leading("2026-08-08T17:42:01Z uniform line", &ctx).unwrap();
        }
        assert_eq!(ctx.sticky_misses(), 0);
        assert_eq!(ctx.prefix_hint(), Some((TimestampShape::Iso, 0)));
    }

    #[test]
    fn a_run_of_misses_reselects_the_hint() {
        let ctx = ctx_at("2026-08-20T00:00:00Z");
        ctx.set_prefix_hint(TimestampShape::Iso, 0);
        for _ in 0..(STICKY_MISS_LIMIT + 1) {
            extract_leading("Aug  8 17:42:01 host app: boom", &ctx).unwrap();
        }
        assert_eq!(ctx.prefix_hint(), Some((TimestampShape::Syslog3164, 0)));
    }

    #[test]
    fn a_wrong_hint_never_changes_the_match() {
        let line = r#"1.2.3.4 - - [08/Aug/2026:17:42:01 +0000] "GET /x" 200 1"#;
        let plain_ctx = ParseContext::utc();
        let expected = extract_leading(line, &plain_ctx).unwrap();

        let hinted = ParseContext::utc();
        hinted.set_prefix_hint(TimestampShape::Epoch, 0);
        let actual = extract_leading(line, &hinted).unwrap();

        assert_eq!(actual.shape, expected.shape);
        assert_eq!(actual.offset, expected.offset);
        assert_eq!(actual.parsed, expected.parsed);
        assert_eq!(actual.rest, expected.rest);
    }
}
