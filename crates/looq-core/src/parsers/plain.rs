//! Plain-text fallback parser (log-parsing spec, "Plain-text fallback"). Always
//! produces one entry per non-empty line and never reports a line as malformed.

use super::Extracted;
use crate::level;
use crate::timestamp::{self, TimeZonePolicy};

/// One entry, always. Extracts a leading timestamp and a level from the message
/// text when recognisable patterns are present; both stay absent otherwise.
pub fn parse_line(line: &str, tz: &TimeZonePolicy) -> Extracted {
    if let Some((parsed, rest)) = timestamp::extract_leading(line, tz) {
        let level = level::scan_message(rest);
        Extracted {
            timestamp: Some(parsed.instant),
            timestamp_used_default_tz: parsed.used_default_tz,
            timestamp_diagnostic: None,
            level,
            message: Some(rest.to_string()),
            fields: Default::default(),
        }
    } else {
        let level = level::scan_message(line);
        Extracted {
            timestamp: None,
            timestamp_used_default_tz: false,
            timestamp_diagnostic: None,
            level,
            message: Some(line.to_string()),
            fields: Default::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unstructured_line_still_becomes_an_entry() {
        let tz = TimeZonePolicy::utc();
        let extracted = parse_line("something happened that matches no pattern at all", &tz);
        assert!(extracted.timestamp.is_none());
        assert!(extracted.level.is_none());
        assert_eq!(
            extracted.message.as_deref(),
            Some("something happened that matches no pattern at all")
        );
    }

    #[test]
    fn leading_timestamp_and_level_extracted() {
        let tz = TimeZonePolicy::utc();
        let extracted = parse_line("2026-08-08T17:42:01Z ERROR connection refused", &tz);
        assert!(extracted.timestamp.is_some());
        assert_eq!(
            extracted.level,
            Some(crate::level::normalize("error").unwrap())
        );
        assert_eq!(
            extracted.message.as_deref(),
            Some("ERROR connection refused")
        );
    }
}
