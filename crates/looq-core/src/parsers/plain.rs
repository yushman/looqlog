//! Plain-text fallback parser (log-parsing spec, "Plain-text fallback"). Always
//! produces one entry per non-empty line and never reports a line as malformed.
//!
//! Since `prefix-and-payload-parsing` this is also where the prefix/payload split
//! lives: a recognised timestamp prefix (any of `TimestampShape`, anywhere in the
//! head window) is consumed together with the level token that follows it, and
//! whatever remains is handed to the JSON or logfmt parser when it looks structured
//! (design.md D6). Everything else about the format stays what it was — one entry per
//! line, no diagnostics, ever.

use std::collections::BTreeMap;

use super::{json, logfmt, Extracted};
use crate::entry::FieldValue;
use crate::level;
use crate::timestamp::{self, LogcatRecord, ParseContext};

/// Field the prefix timestamp is kept under when a parsed payload disagrees with it
/// (design.md D7). The disagreement stays inspectable and filterable instead of being
/// quietly resolved in the payload's favour.
const PREFIX_TIMESTAMP_FIELD: &str = "prefix_ts";

/// A logcat column as a field value: `Number` when it is one (`806`, `29149`),
/// `String` when it is a name (`root`, `u0_a2`), so the field inventory does not
/// present `root` as a number.
fn column_value(raw: &str) -> FieldValue {
    if !raw.is_empty() && raw.bytes().all(|b| b.is_ascii_digit()) {
        FieldValue::Number(raw.to_string())
    } else {
        FieldValue::String(raw.to_string())
    }
}

/// The fields a recognised logcat record contributes (field-extraction spec, "logcat
/// columns become fields"). Ordinary fields, filterable like any other — `tag` is the
/// valuable one, 334 distinct values across 32,712 lines in the measured bugreport:
/// low cardinality, high selectivity, exactly what a filter chip is for. `uid` only
/// exists in the three-column layout, so it is absent rather than empty in the other.
fn logcat_fields(record: &LogcatRecord<'_>) -> BTreeMap<String, FieldValue> {
    let mut fields = BTreeMap::new();
    fields.insert(
        "tag".to_string(),
        FieldValue::String(record.tag.to_string()),
    );
    fields.insert("pid".to_string(), column_value(record.pid));
    fields.insert("tid".to_string(), column_value(record.tid));
    if let Some(uid) = record.uid {
        fields.insert("uid".to_string(), column_value(uid));
    }
    fields
}

/// One entry, always. Extracts a leading timestamp, a level and a structured payload
/// when recognisable patterns are present; all three stay absent otherwise.
pub fn parse_line(line: &str, ctx: &ParseContext) -> Extracted {
    let Some(matched) = timestamp::extract_leading(line, ctx) else {
        // No recognised prefix: today's behavior, unchanged. The whole line is the
        // message and the level scan is the only thing that runs.
        return Extracted {
            timestamp: None,
            timestamp_used_default_tz: false,
            timestamp_year_inferred: false,
            timestamp_diagnostic: None,
            level: level::scan_message(line),
            message: Some(line.to_string()),
            fields: Default::default(),
        };
    };

    // Level by position (design.md D5): the klog/logcat severity letter the timestamp
    // scanner had to consume, else the token immediately after the prefix.
    let mut rest = matched.rest;
    let mut prefix_level = matched.level_letter.and_then(level::from_letter);
    if prefix_level.is_none() {
        if let Some((level, after_token)) = level::match_positional(rest) {
            prefix_level = Some(level);
            rest = after_token;
        }
    }

    // A logcat record's columns and tag are fields, not message text
    // (field-extraction spec, "logcat columns become fields"). Built here so both the
    // payload and the no-payload path get them: a logcat message that is itself
    // structured contributes its own fields *as well*.
    let logcat_fields = matched
        .logcat
        .map(|record| logcat_fields(&record))
        .unwrap_or_default();

    let Some(payload) = dispatch_payload(rest, ctx) else {
        let message = join_head(matched.head, rest);
        // Only when the positional token yielded nothing does the whole-message scan
        // run — that ordering is the behavior change D5 calls out.
        let level = prefix_level.or_else(|| level::scan_message(&message));
        return Extracted {
            timestamp: Some(matched.parsed.instant),
            timestamp_used_default_tz: matched.parsed.used_default_tz,
            timestamp_year_inferred: matched.parsed.year_inferred,
            timestamp_diagnostic: None,
            level,
            message: Some(message),
            fields: logcat_fields,
        };
    };

    // Payload wins conflicts (design.md D7): the prefix is stamped by a collector or
    // container runtime *around* the application's own line, so the payload is the
    // more authoritative record of what was said and when. Same rule for a payload
    // key that collides with a logcat column name.
    let mut fields = logcat_fields;
    fields.extend(payload.fields);
    let (timestamp, used_default_tz, year_inferred) = match payload.timestamp {
        Some(instant) => {
            if instant != matched.parsed.instant {
                fields.insert(
                    PREFIX_TIMESTAMP_FIELD.to_string(),
                    FieldValue::String(matched.parsed.instant.to_rfc3339()),
                );
            }
            (
                Some(instant),
                payload.timestamp_used_default_tz,
                payload.timestamp_year_inferred,
            )
        }
        None => (
            Some(matched.parsed.instant),
            matched.parsed.used_default_tz,
            matched.parsed.year_inferred,
        ),
    };

    Extracted {
        timestamp,
        timestamp_used_default_tz: used_default_tz,
        timestamp_year_inferred: year_inferred,
        // A payload's own timestamp complaint is not a plain-text file's problem: the
        // prefix already produced a usable entry, and the log-parsing spec forbids
        // plain text from emitting diagnostics at all (design.md D6).
        timestamp_diagnostic: None,
        level: payload.level.or(prefix_level),
        message: Some(payload.message.unwrap_or_default()),
        fields,
    }
}

/// Offer the text behind the prefix to a structured parser when it looks like one
/// (design.md D6). One level only: `json::parse_payload` has the Docker unwrap turned
/// off and neither structured parser re-enters this one, so a payload's own payload is
/// never parsed again.
///
/// A payload that fails to parse is not a malformed line — it simply stays message
/// text (log-parsing spec, "A payload that fails to parse is not malformed").
fn dispatch_payload(rest: &str, ctx: &ParseContext) -> Option<Extracted> {
    if rest.starts_with('{') {
        return json::parse_payload(rest, ctx).ok();
    }
    if logfmt::looks_like_payload(rest) {
        return Some(logfmt::parse_line(rest, ctx));
    }
    None
}

/// Text that preceded the timestamp inside the head window (`1.2.3.4 - -` of an
/// access-log line) is put back in front of the message rather than dropped. Only the
/// timestamp and its level token were *recognised*; silently discarding the client
/// address of every access-log line would be exactly the kind of quiet loss the rest
/// of this crate is written to avoid.
fn join_head(head: &str, rest: &str) -> String {
    if head.is_empty() {
        rest.to_string()
    } else if rest.is_empty() {
        head.to_string()
    } else {
        format!("{head} {rest}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::level::Level;
    use chrono::{DateTime, Utc};

    fn ctx_at(reference: &str) -> ParseContext {
        ParseContext::utc().with_reference(
            DateTime::parse_from_rfc3339(reference)
                .unwrap()
                .with_timezone(&Utc),
        )
    }

    #[test]
    fn unstructured_line_still_becomes_an_entry() {
        let tz = ParseContext::utc();
        let extracted = parse_line("something happened that matches no pattern at all", &tz);
        assert!(extracted.timestamp.is_none());
        assert!(extracted.level.is_none());
        assert_eq!(
            extracted.message.as_deref(),
            Some("something happened that matches no pattern at all")
        );
    }

    #[test]
    fn leading_timestamp_and_positional_level_extracted() {
        let tz = ParseContext::utc();
        let extracted = parse_line("2026-08-08T17:42:01Z ERROR connection refused", &tz);
        assert!(extracted.timestamp.is_some());
        assert_eq!(extracted.level, Some(Level::Error));
        // The level token is part of the recognised prefix now, so it leaves the
        // message (log-parsing spec, "Container stdout is parsed…" asserts the same).
        assert_eq!(extracted.message.as_deref(), Some("connection refused"));
    }

    #[test]
    fn iso_recognition_is_unchanged() {
        let tz = ParseContext::utc();
        let extracted = parse_line("2026-08-08T17:42:01Z something happened", &tz);
        assert_eq!(extracted.message.as_deref(), Some("something happened"));
        assert!(extracted.level.is_none());
    }

    // --- level precedence (task 4.4) ----------------------------------------

    #[test]
    fn positional_level_wins_over_a_later_word() {
        let tz = ParseContext::utc();
        let extracted = parse_line(
            "2026-08-08T17:42:01Z INFO retrying after ERROR response",
            &tz,
        );
        assert_eq!(extracted.level, Some(Level::Info));
        assert_eq!(
            extracted.message.as_deref(),
            Some("retrying after ERROR response")
        );
    }

    #[test]
    fn whole_message_scan_still_applies_without_a_prefix() {
        let tz = ParseContext::utc();
        let extracted = parse_line("app: ERROR something broke", &tz);
        assert_eq!(extracted.level, Some(Level::Error));
        assert!(extracted.timestamp.is_none());
    }

    #[test]
    fn whole_message_scan_still_applies_behind_a_prefix() {
        let tz = ParseContext::utc();
        // `api` is not a level, so the token is left in place and the scan runs.
        let extracted = parse_line("2026-08-08T17:42:01Z api WARN slow query", &tz);
        assert_eq!(extracted.level, Some(Level::Warn));
        assert_eq!(extracted.message.as_deref(), Some("api WARN slow query"));
    }

    #[test]
    fn klog_severity_letter_becomes_the_level() {
        let ctx = ctx_at("2026-08-20T00:00:00Z");
        let extracted = parse_line("I0808 17:42:01.123456       1 main.go:10] starting", &ctx);
        assert_eq!(extracted.level, Some(Level::Info));
        assert!(extracted.timestamp_year_inferred);
        assert_eq!(extracted.message.as_deref(), Some("1 main.go:10] starting"));
    }

    #[test]
    fn syslog_priority_in_the_positional_slot() {
        let tz = ParseContext::utc();
        let extracted = parse_line("2026-08-08T17:42:01Z <130> disk full", &tz);
        assert_eq!(extracted.level, Some(Level::Fatal));
        assert_eq!(extracted.message.as_deref(), Some("disk full"));
    }

    // --- logcat (`logcat-and-payload-precision` tasks 3.1–3.4) ---------------

    #[test]
    fn logcat_severity_letter_becomes_the_level_and_the_columns_become_fields() {
        let ctx = ctx_at("2026-08-20T00:00:00Z");
        let extracted = parse_line(
            "04-18 19:21:16.151  1000   806   995 D ActivityManager: freezing 2521 com.x",
            &ctx,
        );
        assert_eq!(extracted.level, Some(Level::Debug));
        assert!(extracted.timestamp_year_inferred);
        // Columns and tag leave the message entirely (field-extraction spec).
        assert_eq!(extracted.message.as_deref(), Some("freezing 2521 com.x"));
        assert_eq!(
            extracted.fields.get("tag"),
            Some(&FieldValue::String("ActivityManager".to_string()))
        );
        assert_eq!(
            extracted.fields.get("uid"),
            Some(&FieldValue::Number("1000".to_string()))
        );
        assert_eq!(
            extracted.fields.get("pid"),
            Some(&FieldValue::Number("806".to_string()))
        );
        assert_eq!(
            extracted.fields.get("tid"),
            Some(&FieldValue::Number("995".to_string()))
        );
    }

    #[test]
    fn logcat_two_columns_produce_no_uid() {
        let ctx = ctx_at("2026-08-20T00:00:00Z");
        let extracted = parse_line(
            "04-21 13:07:51.985   806 29149 W UsbDescriptorParser: Unrecognized len: 58",
            &ctx,
        );
        assert_eq!(extracted.level, Some(Level::Warn));
        assert_eq!(
            extracted.fields.get("pid"),
            Some(&FieldValue::Number("806".to_string()))
        );
        assert_eq!(
            extracted.fields.get("tid"),
            Some(&FieldValue::Number("29149".to_string()))
        );
        assert!(!extracted.fields.contains_key("uid"));
    }

    #[test]
    fn logcat_named_uid_stays_a_string() {
        let ctx = ctx_at("2026-08-20T00:00:00Z");
        let extracted = parse_line(
            "04-21 12:57:16.290  root   511   614 D vhdnativeservice: cmd: dumpsys cpuinfo",
            &ctx,
        );
        assert_eq!(
            extracted.fields.get("uid"),
            Some(&FieldValue::String("root".to_string()))
        );
        // The tag's colon is the anchor; the one in the message is left alone.
        assert_eq!(extracted.message.as_deref(), Some("cmd: dumpsys cpuinfo"));
    }

    #[test]
    fn logcat_silent_severity_reports_no_level() {
        let ctx = ctx_at("2026-08-20T00:00:00Z");
        let extracted = parse_line(
            "04-21 13:07:53.198  1000   806   806 S SilentTag: nothing to report here",
            &ctx,
        );
        // The record is recognised — it has a timestamp and its columns — but `S`
        // means "emit nothing", not a severity (design D2).
        assert!(extracted.timestamp.is_some());
        assert_eq!(extracted.level, None);
        assert_eq!(
            extracted.fields.get("tag"),
            Some(&FieldValue::String("SilentTag".to_string()))
        );
    }

    #[test]
    fn logcat_message_that_is_itself_structured_contributes_both_sets_of_fields() {
        let ctx = ctx_at("2026-08-20T00:00:00Z");
        let extracted = parse_line(
            r#"04-21 13:07:55.010  1000  1024  1024 I OkHttp: level=warn service=api msg="slow response""#,
            &ctx,
        );
        // The payload's own level wins over the severity letter (design D7).
        assert_eq!(extracted.level, Some(Level::Warn));
        assert_eq!(extracted.message.as_deref(), Some("slow response"));
        assert_eq!(
            extracted.fields.get("service"),
            Some(&FieldValue::String("api".to_string()))
        );
        assert_eq!(
            extracted.fields.get("tag"),
            Some(&FieldValue::String("OkHttp".to_string()))
        );
        assert_eq!(
            extracted.fields.get("pid"),
            Some(&FieldValue::Number("1024".to_string()))
        );
    }

    #[test]
    fn a_tagless_logcat_like_line_contributes_nothing() {
        let ctx = ctx_at("2026-08-20T00:00:00Z");
        let line = "04-21 13:07:51.985   806 29149 W";
        let extracted = parse_line(line, &ctx);
        // Not consumed at all: no timestamp, no fields, the whole line is the message.
        assert!(extracted.timestamp.is_none());
        assert!(extracted.fields.is_empty());
        assert_eq!(extracted.message.as_deref(), Some(line));
    }

    // --- prefix shapes end to end (task 5.1) ---------------------------------

    #[test]
    fn syslog_line_gets_a_timestamp_and_keeps_the_remainder() {
        let ctx = ctx_at("2026-08-20T00:00:00Z");
        let extracted = parse_line("Aug  8 17:42:01 host app[123]: connection refused", &ctx);
        assert!(extracted.timestamp.is_some());
        assert!(extracted.timestamp_year_inferred);
        assert_eq!(
            extracted.message.as_deref(),
            Some("host app[123]: connection refused")
        );
    }

    #[test]
    fn access_log_head_is_kept_in_the_message() {
        let tz = ParseContext::utc();
        let extracted = parse_line(
            r#"1.2.3.4 - - [08/Aug/2026:17:42:01 +0000] "GET /x HTTP/1.1" 500 12"#,
            &tz,
        );
        assert_eq!(
            extracted.timestamp.unwrap().to_rfc3339(),
            "2026-08-08T17:42:01+00:00"
        );
        assert_eq!(
            extracted.message.as_deref(),
            Some(r#"1.2.3.4 - - "GET /x HTTP/1.1" 500 12"#)
        );
    }

    #[test]
    fn a_number_late_in_the_line_is_not_a_timestamp() {
        let tz = ParseContext::utc();
        let padding = "x".repeat(crate::timestamp::HEAD_WINDOW);
        let line = format!("{padding} 2026-08-08T17:42:01Z late");
        let extracted = parse_line(&line, &tz);
        assert!(extracted.timestamp.is_none());
        assert_eq!(extracted.message.as_deref(), Some(line.as_str()));
    }

    // --- payload dispatch (tasks 5.2, 5.3, 5.5) ------------------------------

    #[test]
    fn json_payload_behind_a_plain_prefix_contributes_fields() {
        let tz = ParseContext::utc();
        let extracted = parse_line(
            r#"2026-08-08 17:42:01 INFO {"status":500,"path":"/x"}"#,
            &tz,
        );
        assert_eq!(extracted.level, Some(Level::Info));
        assert!(extracted.fields.contains_key("status"));
        assert!(extracted.fields.contains_key("path"));
        assert_eq!(extracted.message.as_deref(), Some(""));
    }

    #[test]
    fn logfmt_payload_behind_a_syslog_prefix() {
        let ctx = ctx_at("2026-08-20T00:00:00Z");
        let extracted = parse_line(
            r#"Aug  8 17:42:01 host app: level=error msg="boom" service=api"#,
            &ctx,
        );
        assert_eq!(extracted.level, Some(Level::Error));
        assert_eq!(extracted.message.as_deref(), Some("boom"));
        match extracted.fields.get("service").unwrap() {
            FieldValue::String(s) => assert_eq!(s, "api"),
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn prose_with_one_pair_is_not_mistaken_for_logfmt() {
        let tz = ParseContext::utc();
        let extracted = parse_line(
            "2026-08-08T17:42:01Z retried because mode=safe was set",
            &tz,
        );
        assert!(extracted.fields.is_empty());
        assert_eq!(
            extracted.message.as_deref(),
            Some("retried because mode=safe was set")
        );
    }

    #[test]
    fn payload_that_fails_to_parse_stays_message_text() {
        let tz = ParseContext::utc();
        let extracted = parse_line(r#"2026-08-08 17:42:01 INFO {"broken":"#, &tz);
        assert!(extracted.timestamp.is_some());
        assert_eq!(extracted.level, Some(Level::Info));
        assert_eq!(extracted.message.as_deref(), Some(r#"{"broken":"#));
        assert!(extracted.timestamp_diagnostic.is_none());
        assert!(extracted.fields.is_empty());
    }

    #[test]
    fn payload_nesting_stops_at_one_level() {
        let tz = ParseContext::utc();
        // The payload's own message value looks like a structured line; it stays text.
        let extracted = parse_line(
            r#"2026-08-08T17:42:01Z {"msg":"level=error msg=\"inner\" service=api"}"#,
            &tz,
        );
        assert_eq!(
            extracted.message.as_deref(),
            Some(r#"level=error msg="inner" service=api"#)
        );
        assert!(extracted.fields.is_empty());
        assert_eq!(extracted.level, None);
    }

    /// The defect line verbatim from the measured bugreport
    /// (`logcat-and-payload-precision` design D4). `time` and `ret` are real fields;
    /// everything inside the two braced blocks is a Java map dump that used to arrive
    /// as top-level filter chips.
    #[test]
    fn a_dumped_java_map_behind_a_prefix_does_not_become_many_fields() {
        let tz = ParseContext::utc();
        let extracted = parse_line(
            r#"2026-04-16T21:34:04.009 - PROBE_HTTP http://x time=18ms ret=204 request={Connection=[close], User-Agent=[Mozilla/5.0 (X11; Linux x86_64)]} headers={null=[HTTP/1.1 204 No Content], Alt-Svc=[h3=":443"; ma=2592000], Content-Length=[0]}"#,
            &tz,
        );
        let names: Vec<&str> = extracted.fields.keys().map(String::as_str).collect();
        assert_eq!(names, vec!["headers", "request", "ret", "time"]);
        for gone in ["Alt-Svc", "Content-Length", "User-Agent", "Connection"] {
            assert!(
                !extracted.fields.contains_key(gone),
                "{gone} is still a field"
            );
        }
    }

    // --- conflicts (task 5.4) ------------------------------------------------

    #[test]
    fn payload_timestamp_and_level_win_and_the_prefix_timestamp_is_kept() {
        let tz = ParseContext::utc();
        let extracted = parse_line(
            r#"2026-08-08 17:42:01 INFO {"ts":"2026-08-08T17:41:59Z","level":"error","msg":"boom"}"#,
            &tz,
        );
        assert_eq!(
            extracted.timestamp.unwrap().to_rfc3339(),
            "2026-08-08T17:41:59+00:00"
        );
        assert_eq!(extracted.level, Some(Level::Error));
        assert_eq!(extracted.message.as_deref(), Some("boom"));
        match extracted.fields.get(PREFIX_TIMESTAMP_FIELD).unwrap() {
            FieldValue::String(s) => assert_eq!(s, "2026-08-08T17:42:01+00:00"),
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn the_prefix_fills_what_the_payload_omits() {
        let tz = ParseContext::utc();
        let extracted = parse_line(r#"2026-08-08 17:42:01 WARN {"msg":"boom","a":1}"#, &tz);
        assert_eq!(
            extracted.timestamp.unwrap().to_rfc3339(),
            "2026-08-08T17:42:01+00:00"
        );
        assert_eq!(extracted.level, Some(Level::Warn));
        assert_eq!(extracted.message.as_deref(), Some("boom"));
        // No disagreement to record.
        assert!(!extracted.fields.contains_key(PREFIX_TIMESTAMP_FIELD));
    }
}
