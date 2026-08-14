//! Timestamp parsing and the timezone policy (design.md D5, resolves TDR §16 and
//! PRD §14 Q2/timezone question).
//!
//! Field-name precedence for structured formats: `timestamp`, `ts`, `time`,
//! `@timestamp`, `t`, in that order (field-extraction spec).

use chrono::{DateTime, FixedOffset, NaiveDateTime, TimeZone, Utc};

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

/// Result of parsing a timestamp value: the instant, and whether the caller's
/// default/override timezone had to be applied (i.e. the value carried no explicit
/// offset). Reported so a UI can tell the user which assumption was used
/// (field-extraction spec: "Timezone policy for offset-less timestamps").
pub struct ParsedTimestamp {
    pub instant: DateTime<Utc>,
    pub used_default_tz: bool,
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
        });
    }

    // RFC 3339 handles an explicit offset (including `Z`) directly.
    if let Ok(dt) = DateTime::parse_from_rfc3339(raw) {
        return Some(ParsedTimestamp {
            instant: dt.with_timezone(&Utc),
            used_default_tz: false,
        });
    }

    // Accept a space separator as well as `T` (common in plain-text logs), still
    // with an explicit offset/`Z` suffix.
    let t_form = replace_first_space_with_t(raw);
    if let Ok(dt) = DateTime::parse_from_rfc3339(&t_form) {
        return Some(ParsedTimestamp {
            instant: dt.with_timezone(&Utc),
            used_default_tz: false,
        });
    }

    // No offset at all: naive date-time, interpreted per the timezone policy.
    for fmt in ["%Y-%m-%dT%H:%M:%S%.f", "%Y-%m-%dT%H:%M:%S"] {
        if let Ok(naive) = NaiveDateTime::parse_from_str(&t_form, fmt) {
            return Some(ParsedTimestamp {
                instant: tz.apply(naive),
                used_default_tz: true,
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

/// Leading-timestamp pattern for plain text (field-extraction spec: "Leading
/// timestamp in plain text"). Matches only at the start of the line — matching
/// anywhere is more forgiving but more likely to snag a number that isn't a
/// timestamp (design.md Open Questions); deferred rather than guessed at.
///
/// Hand-rolled instead of `regex` (design.md D9 risk / task 6.4): pulling in the
/// `regex` crate for this one fixed pattern measured at ~1MB added to `core.wasm`
/// (see devlog), an order of magnitude over the ~300KB TDR §5 budget on its own.
/// This scanner matches `\d{4}-\d{2}-\d{2}[T ]\d{2}:\d{2}:\d{2}(\.\d+)?(Z|[+-]\d{2}:?\d{2})?`
/// byte-by-byte; every branch only advances over matched ASCII bytes, so the
/// returned prefix is always a valid `str` char-boundary slice.
fn match_leading_timestamp(line: &str) -> Option<&str> {
    let bytes = line.as_bytes();
    let mut i = 0usize;

    fn take_digits(bytes: &[u8], i: &mut usize, n: usize) -> bool {
        if *i + n > bytes.len() {
            return false;
        }
        if !bytes[*i..*i + n].iter().all(u8::is_ascii_digit) {
            return false;
        }
        *i += n;
        true
    }
    fn take_byte(bytes: &[u8], i: &mut usize, b: u8) -> bool {
        if bytes.get(*i) == Some(&b) {
            *i += 1;
            true
        } else {
            false
        }
    }

    if !take_digits(bytes, &mut i, 4) {
        return None;
    }
    if !take_byte(bytes, &mut i, b'-') {
        return None;
    }
    if !take_digits(bytes, &mut i, 2) {
        return None;
    }
    if !take_byte(bytes, &mut i, b'-') {
        return None;
    }
    if !take_digits(bytes, &mut i, 2) {
        return None;
    }
    match bytes.get(i) {
        Some(b'T') | Some(b' ') => i += 1,
        _ => return None,
    }
    if !take_digits(bytes, &mut i, 2) {
        return None;
    }
    if !take_byte(bytes, &mut i, b':') {
        return None;
    }
    if !take_digits(bytes, &mut i, 2) {
        return None;
    }
    if !take_byte(bytes, &mut i, b':') {
        return None;
    }
    if !take_digits(bytes, &mut i, 2) {
        return None;
    }

    // Optional fractional seconds: `.` followed by one or more digits.
    if bytes.get(i) == Some(&b'.') {
        let mut j = i + 1;
        let start_digits = j;
        while bytes.get(j).is_some_and(u8::is_ascii_digit) {
            j += 1;
        }
        if j > start_digits {
            i = j;
        }
    }

    // Optional offset: `Z`, or `[+-]\d{2}:?\d{2}`.
    match bytes.get(i) {
        Some(b'Z') => i += 1,
        Some(b'+') | Some(b'-') => {
            let save = i;
            let mut j = i + 1;
            if take_digits(bytes, &mut j, 2) {
                if bytes.get(j) == Some(&b':') {
                    j += 1;
                }
                if take_digits(bytes, &mut j, 2) {
                    i = j;
                } else {
                    i = save;
                }
            } else {
                i = save;
            }
        }
        _ => {}
    }

    Some(&line[..i])
}

/// Extract a leading timestamp from `line`. Returns the parsed timestamp and the
/// remainder of the line (trimmed), or `None` if the line does not start with a
/// recognisable timestamp.
pub fn extract_leading<'a>(
    line: &'a str,
    tz: &TimeZonePolicy,
) -> Option<(ParsedTimestamp, &'a str)> {
    let matched = match_leading_timestamp(line)?;
    let parsed = parse_value(matched, tz)?;
    let rest = line[matched.len()..].trim_start();
    Some((parsed, rest))
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let (parsed, rest) = extract_leading(
            "2026-08-08T17:42:01Z something happened",
            &TimeZonePolicy::utc(),
        )
        .unwrap();
        assert!(!parsed.used_default_tz);
        assert_eq!(rest, "something happened");
    }

    #[test]
    fn no_leading_timestamp() {
        assert!(extract_leading("nothing here", &TimeZonePolicy::utc()).is_none());
    }

    #[test]
    fn garbage_is_not_a_timestamp() {
        assert!(parse_value("yesterday", &TimeZonePolicy::utc()).is_none());
    }
}
