//! JS-facing DTOs for the typed WASM↔JS boundary (wasm-bridge spec, design.md D2).
//!
//! These mirror `looq-core`'s types field-for-field but live entirely in this crate:
//! `looq-core` stays free of `serde::Serialize`/wasm concerns (ADR-0005), and the
//! wire shape (camelCase, string-tagged enums) is a JS-interop concern, not a core
//! parsing concern. `web/src/wasm-types.ts` is the hand-written TypeScript mirror of
//! every struct below — the two are kept in sync by convention, guarded by `tsc
//! --noEmit` catching stale field usages after a rename (see
//! `web/scripts/verify-rename-check.sh`).

use std::collections::BTreeMap;

use serde::Serialize;

use looq_core::{
    DetectionOutcome, DetectionResult, Diagnostic, DiagnosticReason, Diagnostics, Entry,
    FieldInventory, FieldStats, FieldValue, Format,
};

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FieldValueDto {
    String { value: String },
    Number { value: String },
    Bool { value: bool },
    Null,
    Json { value: String },
}

impl From<&FieldValue> for FieldValueDto {
    fn from(value: &FieldValue) -> Self {
        match value {
            FieldValue::String(s) => FieldValueDto::String { value: s.clone() },
            FieldValue::Number(n) => FieldValueDto::Number { value: n.clone() },
            FieldValue::Bool(b) => FieldValueDto::Bool { value: *b },
            FieldValue::Null => FieldValueDto::Null,
            FieldValue::Json(s) => FieldValueDto::Json { value: s.clone() },
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EntryDto {
    pub ordinal: usize,
    /// RFC 3339, `null` when the line carried no recognisable timestamp.
    pub timestamp: Option<String>,
    pub timestamp_used_default_tz: bool,
    /// True when the line's timestamp shape carried no year and one was inferred from
    /// the caller-supplied reference instant (`prefix-and-payload-parsing` design.md
    /// D4). Crosses alongside `timestamp_used_default_tz` for the same reason: an
    /// entry dated by assumption must be distinguishable in the UI from one dated by
    /// the log.
    pub timestamp_year_inferred: bool,
    /// Canonical uppercase level name (`"INFO"`, `"ERROR"`, ...), `null` when absent.
    pub level: Option<String>,
    pub message: String,
    pub fields: BTreeMap<String, FieldValueDto>,
    /// Ordinal of the root of the multi-line event this line continues, or `null` when
    /// the line starts its own entry (`multiline-entry-continuations` design D1/D2).
    /// Crosses as `number | null` and never `undefined`: the serializer is configured
    /// with `serialize_missing_as_null(true)`, so a consumer testing `=== null` for
    /// absence is correct.
    pub continuation_of: Option<usize>,
}

impl From<&Entry> for EntryDto {
    fn from(entry: &Entry) -> Self {
        EntryDto {
            ordinal: entry.ordinal,
            timestamp: entry.timestamp.map(|ts| ts.to_rfc3339()),
            timestamp_used_default_tz: entry.timestamp_used_default_tz,
            timestamp_year_inferred: entry.timestamp_year_inferred,
            level: entry.level.map(|l| l.as_str().to_string()),
            message: entry.message.clone(),
            fields: entry
                .fields
                .iter()
                .map(|(k, v)| (k.clone(), FieldValueDto::from(v)))
                .collect(),
            continuation_of: entry.continuation_of,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectionResultDto {
    pub format: String,
    pub match_fraction: f64,
    /// `"threshold"` when a candidate crossed the detection threshold outright,
    /// `"fallback"` when plain text was chosen because nothing did. No new variant was
    /// added for prefixed plain text — it reports `"threshold"` like any other match
    /// (`prefix-and-payload-parsing` design.md D9).
    pub outcome: String,
    /// Which timestamp shape the sample matched (`"iso8601"`, `"syslog3164"`,
    /// `"klog"`, `"clf"`, `"slash-date"`, `"epoch"`) and at which head offset, or
    /// `null` for structured formats and for input with no recognised prefixes. This
    /// is what lets the UI say *how* plain text matched instead of only that it did.
    pub timestamp_shape: Option<String>,
    pub timestamp_offset: Option<usize>,
}

impl From<&DetectionResult> for DetectionResultDto {
    fn from(result: &DetectionResult) -> Self {
        DetectionResultDto {
            format: result.format.as_str().to_string(),
            match_fraction: result.match_fraction,
            outcome: match result.outcome {
                DetectionOutcome::Threshold => "threshold".to_string(),
                DetectionOutcome::Fallback => "fallback".to_string(),
            },
            timestamp_shape: result.timestamp_shape.map(|s| s.as_str().to_string()),
            timestamp_offset: result.timestamp_offset,
        }
    }
}

/// Stable machine-readable key for a [`DiagnosticReason`], distinct from its
/// human-readable `label()` (which stays free to change wording without breaking
/// the JS-side `switch` a UI might build over it).
fn reason_key(reason: DiagnosticReason) -> &'static str {
    match reason {
        DiagnosticReason::InvalidJson => "invalid_json",
        DiagnosticReason::NonObjectJson => "non_object_json",
        DiagnosticReason::UnparsableTimestamp => "unparsable_timestamp",
        DiagnosticReason::EncodingFallback => "encoding_fallback",
        DiagnosticReason::ChainTruncated => "chain_truncated",
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticDto {
    pub line: usize,
    pub reason: String,
    pub reason_label: String,
    pub detail: String,
}

impl From<&Diagnostic> for DiagnosticDto {
    fn from(diag: &Diagnostic) -> Self {
        DiagnosticDto {
            line: diag.line,
            reason: reason_key(diag.reason).to_string(),
            reason_label: diag.reason.label().to_string(),
            detail: diag.detail.clone(),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticsSummaryDto {
    pub total: usize,
    pub cap: usize,
    /// Exact per-reason counts, never truncated (unlike `examples`).
    pub counts: BTreeMap<String, usize>,
    /// Retained examples, capped at `cap` (see `looq_core::Diagnostics`).
    pub examples: Vec<DiagnosticDto>,
}

impl From<&Diagnostics> for DiagnosticsSummaryDto {
    fn from(diagnostics: &Diagnostics) -> Self {
        let mut counts = BTreeMap::new();
        for reason in [
            DiagnosticReason::InvalidJson,
            DiagnosticReason::NonObjectJson,
            DiagnosticReason::UnparsableTimestamp,
            DiagnosticReason::EncodingFallback,
            DiagnosticReason::ChainTruncated,
        ] {
            let count = diagnostics.count_for(reason);
            if count > 0 {
                counts.insert(reason_key(reason).to_string(), count);
            }
        }
        DiagnosticsSummaryDto {
            total: diagnostics.total(),
            cap: diagnostics.cap(),
            counts,
            examples: diagnostics
                .retained()
                .iter()
                .map(DiagnosticDto::from)
                .collect(),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldStatsDto {
    pub count: usize,
    pub values: BTreeMap<String, usize>,
    pub high_cardinality: bool,
}

impl From<&FieldStats> for FieldStatsDto {
    fn from(stats: &FieldStats) -> Self {
        FieldStatsDto {
            count: stats.count,
            values: stats.values.clone(),
            high_cardinality: stats.high_cardinality,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldInventoryDto {
    pub cap: usize,
    pub fields: BTreeMap<String, FieldStatsDto>,
}

impl From<&FieldInventory> for FieldInventoryDto {
    fn from(inventory: &FieldInventory) -> Self {
        FieldInventoryDto {
            cap: inventory.cap(),
            fields: inventory
                .fields()
                .iter()
                .map(|(k, v)| (k.clone(), FieldStatsDto::from(v)))
                .collect(),
        }
    }
}

/// `Level`/`Format` round-trip helper for the constructor's format-override string.
pub fn format_from_str(s: &str) -> Option<Format> {
    match s {
        "json" => Some(Format::Json),
        "logfmt" => Some(Format::Logfmt),
        "plain" => Some(Format::Plain),
        _ => None,
    }
}
