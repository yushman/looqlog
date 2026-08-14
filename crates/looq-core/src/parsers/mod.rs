//! Format parsers, all producing the same intermediate `Extracted` shape so
//! timestamp/level/message recognition (the "which field is the timestamp" logic)
//! lives in one place rather than being reimplemented per format (design.md D1's
//! "one implementation" spirit applied inside the parser layer too).

pub mod json;
pub mod logfmt;
pub mod plain;

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};

use crate::entry::FieldValue;
use crate::level::{self, Level};
use crate::timestamp::{self, TimeZonePolicy, TIMESTAMP_FIELDS};

/// Field names checked for the level, in precedence order (field-extraction spec).
const LEVEL_FIELDS: &[&str] = &["level", "lvl", "severity"];

/// Field names checked for the message, in precedence order. Not explicit in the
/// spec text (which only states the *result* is "a message"); `msg` is inferred
/// from the log-parsing spec's own JSON example (`"msg":"boom"`) and the logfmt
/// "bare tokens fold into the message" scenario alongside an explicit `msg=` key.
const MESSAGE_FIELDS: &[&str] = &["message", "msg"];

/// A parsed line before it becomes an `Entry`: same shape regardless of which
/// format produced it.
#[derive(Debug)]
pub struct Extracted {
    pub timestamp: Option<DateTime<Utc>>,
    pub timestamp_used_default_tz: bool,
    /// Set when a recognised timestamp field was present but unparsable — the field
    /// stays in `fields` (not consumed) and this detail is turned into a diagnostic
    /// by the caller.
    pub timestamp_diagnostic: Option<String>,
    pub level: Option<Level>,
    pub message: Option<String>,
    pub fields: BTreeMap<String, FieldValue>,
}

/// Pull the recognised timestamp/level/message keys out of `raw` by precedence,
/// leaving everything else as arbitrary fields. Shared by the JSON and logfmt
/// parsers, which differ only in how they produce the initial `raw` map.
pub fn extract(mut raw: BTreeMap<String, FieldValue>, tz: &TimeZonePolicy) -> Extracted {
    let mut timestamp = None;
    let mut timestamp_used_default_tz = false;
    let mut timestamp_diagnostic = None;
    for name in TIMESTAMP_FIELDS {
        if let Some(value) = raw.get(*name) {
            let text = value.display();
            match timestamp::parse_value(&text, tz) {
                Some(parsed) => {
                    timestamp = Some(parsed.instant);
                    timestamp_used_default_tz = parsed.used_default_tz;
                    raw.remove(*name);
                }
                None => {
                    timestamp_diagnostic = Some(format!(
                        "field '{name}' value '{text}' is not a parsable timestamp"
                    ));
                    // Field found but unparsable: stop searching lower-precedence
                    // names (this one is authoritative) and leave it as an ordinary
                    // field (field-extraction spec: "ts remains available").
                }
            }
            break;
        }
    }

    let mut level_value = None;
    for name in LEVEL_FIELDS {
        if let Some(value) = raw.remove(*name) {
            level_value = level::normalize(&value.display());
            break;
        }
    }

    let mut message = None;
    for name in MESSAGE_FIELDS {
        if let Some(value) = raw.remove(*name) {
            message = Some(value.display());
            break;
        }
    }

    Extracted {
        timestamp,
        timestamp_used_default_tz,
        timestamp_diagnostic,
        level: level_value,
        message,
        fields: raw,
    }
}
