//! `Entry`: the structured shape every parsed line becomes (field-extraction spec,
//! "Entry shape").

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};

use crate::level::Level;

/// A field value extracted from a line. Nested JSON objects/arrays are kept as their
/// raw JSON text rather than flattened (design.md D8) — flattening needs decisions
/// on separators, array indices, depth caps and dotted-key collisions that are out
/// of scope for this change.
#[derive(Debug, Clone, PartialEq)]
pub enum FieldValue {
    String(String),
    /// Kept as the original textual representation to avoid float round-trip loss.
    Number(String),
    Bool(bool),
    Null,
    /// Raw JSON text of a nested object or array (D8).
    Json(String),
}

impl FieldValue {
    /// A canonical display string, used both for message text and for cardinality
    /// tracking in the field inventory.
    pub fn display(&self) -> String {
        match self {
            FieldValue::String(s) => s.clone(),
            FieldValue::Number(n) => n.clone(),
            FieldValue::Bool(b) => b.to_string(),
            FieldValue::Null => "null".to_string(),
            FieldValue::Json(s) => s.clone(),
        }
    }
}

/// One parsed log line. Entries are returned in input order; the parser never sorts
/// or reorders them (field-extraction spec).
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    /// 1-based position of the source line in the input, for pointing diagnostics
    /// and the UI back at the original line.
    pub ordinal: usize,
    pub timestamp: Option<DateTime<Utc>>,
    /// True when `timestamp` is present but came from a naive value that had no
    /// explicit offset, so the caller's timezone policy (default UTC) was applied.
    pub timestamp_used_default_tz: bool,
    /// True when `timestamp` is present but its shape carried no year (syslog RFC
    /// 3164, klog), so one was inferred from the caller's reference instant
    /// (`prefix-and-payload-parsing` design.md D4). Carried alongside
    /// `timestamp_used_default_tz` for the same reason: an entry dated by assumption
    /// must be distinguishable from one dated by the log.
    pub timestamp_year_inferred: bool,
    pub level: Option<Level>,
    pub message: String,
    /// Arbitrary fields other than the recognised timestamp/level/message keys.
    pub fields: BTreeMap<String, FieldValue>,
}
