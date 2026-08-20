//! JSON Lines parser (log-parsing spec, "JSON Lines parsing").

use std::collections::BTreeMap;

use serde_json::Value;

use super::{extract, plain, Extracted};
use crate::entry::FieldValue;
use crate::timestamp::{self, ParseContext};

/// Why a line failed to become an entry under the JSON format.
#[derive(Debug, Clone, PartialEq)]
pub enum MalformedReason {
    /// Not valid JSON at all.
    InvalidJson(String),
    /// Valid JSON, but the top level is not an object.
    NonObjectJson,
}

fn json_value_to_field(value: &Value) -> FieldValue {
    match value {
        Value::String(s) => FieldValue::String(s.clone()),
        Value::Number(n) => FieldValue::Number(n.to_string()),
        Value::Bool(b) => FieldValue::Bool(*b),
        Value::Null => FieldValue::Null,
        Value::Object(_) | Value::Array(_) => FieldValue::Json(value.to_string()),
    }
}

/// A line whose top level is a JSON object becomes an entry carrying its members
/// (log-parsing spec "Object line"). A line that is valid JSON but not an object is
/// malformed rather than an entry with no fields ("Non-object JSON").
pub fn parse_line(line: &str, ctx: &ParseContext) -> Result<Extracted, MalformedReason> {
    parse_object(line, ctx, true)
}

/// The same parser for a JSON payload found *behind* a plain-text prefix. The Docker
/// unwrap is off here: the unwrap re-parses its `log` member as a whole line, which
/// can land back in this parser, and design.md D6 caps nesting at one level.
pub(crate) fn parse_payload(line: &str, ctx: &ParseContext) -> Result<Extracted, MalformedReason> {
    parse_object(line, ctx, false)
}

fn parse_object(
    line: &str,
    ctx: &ParseContext,
    unwrap_docker: bool,
) -> Result<Extracted, MalformedReason> {
    let trimmed = line.trim();
    let value: Value =
        serde_json::from_str(trimmed).map_err(|e| MalformedReason::InvalidJson(e.to_string()))?;
    let obj = value.as_object().ok_or(MalformedReason::NonObjectJson)?;

    if unwrap_docker {
        if let Some(extracted) = unwrap_docker_record(obj, ctx) {
            return Ok(extracted);
        }
    }

    let raw: BTreeMap<String, FieldValue> = obj
        .iter()
        .map(|(k, v)| (k.clone(), json_value_to_field(v)))
        .collect();

    let mut extracted = extract(raw, ctx);
    if extracted.message.is_none() {
        // No recognised message key: default to empty rather than duplicating the
        // whole line (every member is already available as a field).
        extracted.message = Some(String::new());
    }
    Ok(extracted)
}

/// Docker's json-file logging driver writes `{"log":…,"stream":…,"time":…}` around
/// whatever the container printed. Left alone, the application's own line surfaces as
/// an escaped blob in a field called `log`; unwrapped, it is parsed as the line it is
/// (design.md D8).
///
/// The member set must be *exactly* those three — an application log that happens to
/// carry a `log` key is not a Docker record — which is also why this lives here rather
/// than becoming a `Format` variant: Docker lines are valid JSON, so a candidate below
/// JSON in the detection chain could never win (D1).
fn unwrap_docker_record(
    obj: &serde_json::Map<String, Value>,
    ctx: &ParseContext,
) -> Option<Extracted> {
    if obj.len() != 3 {
        return None;
    }
    let log = obj.get("log")?.as_str()?;
    let stream = obj.get("stream")?.as_str()?;
    let time = obj.get("time")?.as_str()?;

    let mut extracted = plain::parse_line(log.trim_end_matches(['\n', '\r']), ctx);
    if extracted.timestamp.is_none() {
        if let Some(parsed) = timestamp::parse_value(time, ctx.tz()) {
            extracted.timestamp = Some(parsed.instant);
            extracted.timestamp_used_default_tz = parsed.used_default_tz;
            extracted.timestamp_year_inferred = parsed.year_inferred;
        }
    }
    extracted
        .fields
        .insert("stream".to_string(), FieldValue::String(stream.to_string()));
    Some(extracted)
}

/// Whether `line` looks like JSON Lines, for format detection: valid JSON whose top
/// level is an object.
pub fn matches(line: &str) -> bool {
    serde_json::from_str::<Value>(line.trim())
        .map(|v| v.is_object())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn object_line_carries_all_members() {
        let tz = ParseContext::utc();
        let extracted = parse_line(
            r#"{"ts":"2026-08-08T17:42:01Z","level":"error","msg":"boom"}"#,
            &tz,
        )
        .unwrap();
        assert!(extracted.timestamp.is_some());
        assert_eq!(
            extracted.level,
            Some(crate::level::normalize("error").unwrap())
        );
        assert_eq!(extracted.message.as_deref(), Some("boom"));
    }

    #[test]
    fn non_object_json_is_malformed() {
        let tz = ParseContext::utc();
        let err = parse_line("[1,2,3]", &tz).unwrap_err();
        assert_eq!(err, MalformedReason::NonObjectJson);
    }

    #[test]
    fn invalid_json_is_malformed() {
        let tz = ParseContext::utc();
        let err = parse_line("not json at all", &tz).unwrap_err();
        assert!(matches!(err, MalformedReason::InvalidJson(_)));
    }

    // --- Docker json-file wrapper (task 6) -----------------------------------

    #[test]
    fn docker_record_is_unwrapped_and_parsed_as_a_line() {
        let tz = ParseContext::utc();
        let extracted = parse_line(
            r#"{"log":"2026-08-08T17:42:01Z ERROR boom\n","stream":"stdout","time":"2026-08-08T17:42:02Z"}"#,
            &tz,
        )
        .unwrap();
        assert_eq!(extracted.level, Some(crate::level::Level::Error));
        assert_eq!(extracted.message.as_deref(), Some("boom"));
        // The inner line's own timestamp wins over the wrapper's `time`.
        assert_eq!(
            extracted.timestamp.unwrap().to_rfc3339(),
            "2026-08-08T17:42:01+00:00"
        );
        match extracted.fields.get("stream").unwrap() {
            FieldValue::String(s) => assert_eq!(s, "stdout"),
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn docker_wrapper_time_fills_in_for_an_undated_inner_line() {
        let tz = ParseContext::utc();
        let extracted = parse_line(
            r#"{"log":"boom with no timestamp\n","stream":"stderr","time":"2026-08-08T17:42:02Z"}"#,
            &tz,
        )
        .unwrap();
        assert_eq!(
            extracted.timestamp.unwrap().to_rfc3339(),
            "2026-08-08T17:42:02+00:00"
        );
        assert_eq!(extracted.message.as_deref(), Some("boom with no timestamp"));
    }

    #[test]
    fn ordinary_object_with_a_log_member_is_not_unwrapped() {
        let tz = ParseContext::utc();
        let extracted = parse_line(r#"{"log":"hi","level":"info","service":"api"}"#, &tz).unwrap();
        assert_eq!(extracted.level, Some(crate::level::Level::Info));
        match extracted.fields.get("log").unwrap() {
            FieldValue::String(s) => assert_eq!(s, "hi"),
            other => panic!("unexpected {other:?}"),
        }
        assert!(extracted.fields.contains_key("service"));
    }

    #[test]
    fn three_member_object_that_is_not_a_docker_record_is_left_alone() {
        let tz = ParseContext::utc();
        let extracted =
            parse_line(r#"{"log":"hi","stream":"stdout","level":"warn"}"#, &tz).unwrap();
        assert_eq!(extracted.level, Some(crate::level::Level::Warn));
        assert!(extracted.fields.contains_key("log"));
    }

    #[test]
    fn nested_object_kept_as_json_text() {
        let tz = ParseContext::utc();
        let extracted = parse_line(r#"{"http":{"status":500}}"#, &tz).unwrap();
        match extracted.fields.get("http").unwrap() {
            FieldValue::Json(text) => assert_eq!(text, r#"{"status":500}"#),
            other => panic!("expected Json field value, got {other:?}"),
        }
    }
}
