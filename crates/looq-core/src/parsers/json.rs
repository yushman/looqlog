//! JSON Lines parser (log-parsing spec, "JSON Lines parsing").

use std::collections::BTreeMap;

use serde_json::Value;

use super::{extract, Extracted};
use crate::entry::FieldValue;
use crate::timestamp::TimeZonePolicy;

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
pub fn parse_line(line: &str, tz: &TimeZonePolicy) -> Result<Extracted, MalformedReason> {
    let trimmed = line.trim();
    let value: Value =
        serde_json::from_str(trimmed).map_err(|e| MalformedReason::InvalidJson(e.to_string()))?;
    let obj = value.as_object().ok_or(MalformedReason::NonObjectJson)?;

    let raw: BTreeMap<String, FieldValue> = obj
        .iter()
        .map(|(k, v)| (k.clone(), json_value_to_field(v)))
        .collect();

    let mut extracted = extract(raw, tz);
    if extracted.message.is_none() {
        // No recognised message key: default to empty rather than duplicating the
        // whole line (every member is already available as a field).
        extracted.message = Some(String::new());
    }
    Ok(extracted)
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
        let tz = TimeZonePolicy::utc();
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
        let tz = TimeZonePolicy::utc();
        let err = parse_line("[1,2,3]", &tz).unwrap_err();
        assert_eq!(err, MalformedReason::NonObjectJson);
    }

    #[test]
    fn invalid_json_is_malformed() {
        let tz = TimeZonePolicy::utc();
        let err = parse_line("not json at all", &tz).unwrap_err();
        assert!(matches!(err, MalformedReason::InvalidJson(_)));
    }

    #[test]
    fn nested_object_kept_as_json_text() {
        let tz = TimeZonePolicy::utc();
        let extracted = parse_line(r#"{"http":{"status":500}}"#, &tz).unwrap();
        match extracted.fields.get("http").unwrap() {
            FieldValue::Json(text) => assert_eq!(text, r#"{"status":500}"#),
            other => panic!("expected Json field value, got {other:?}"),
        }
    }
}
