//! Serializable result shapes of [`crate::session::Session`] (`query-api` spec,
//! ADR-0010). camelCase, string-tagged enums: the same shape the former WASM bridge
//! produced, so the browser's types carried over. `web/src/query-types.ts` is the
//! hand-written TypeScript mirror.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::detect::{DetectionOutcome, DetectionResult};
use crate::diagnostics::{Diagnostic, DiagnosticReason, Diagnostics};
use crate::entry::FieldValue;
use crate::fields::{FieldInventory, FieldStats};
use crate::format::Format;

#[derive(Debug, Clone, Serialize, PartialEq)]
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

/// One table row, or the entry the detail pane shows.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RowDto {
    pub ordinal: usize,
    /// RFC 3339, `null` when the line carried no recognisable timestamp.
    pub timestamp: Option<String>,
    pub timestamp_used_default_tz: bool,
    pub timestamp_year_inferred: bool,
    /// Canonical uppercase level name, `null` when absent.
    pub level: Option<String>,
    pub message: String,
    pub fields: BTreeMap<String, FieldValueDto>,
    /// Ordinal of the chain root this line continues, or `null`.
    pub continuation_of: Option<usize>,
    /// Lines in the chain this row roots (itself included), counted within the
    /// current view; `0` when it roots no visible chain.
    pub chain_size: usize,
    /// Whether the chain this row roots is expanded in the current view.
    pub expanded: bool,
    /// Whether this row is shown as a member of a visible chain.
    pub chain_member: bool,
    /// Search matches in `message`, as `[start, end)` UTF-16 offsets.
    pub match_ranges: Vec<(usize, usize)>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DetectionResultDto {
    pub format: String,
    pub match_fraction: f64,
    /// `"threshold"` or `"fallback"`.
    pub outcome: String,
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

/// Stable machine-readable key for a [`DiagnosticReason`], distinct from its label.
fn reason_key(reason: DiagnosticReason) -> &'static str {
    match reason {
        DiagnosticReason::InvalidJson => "invalid_json",
        DiagnosticReason::NonObjectJson => "non_object_json",
        DiagnosticReason::UnparsableTimestamp => "unparsable_timestamp",
        DiagnosticReason::EncodingFallback => "encoding_fallback",
        DiagnosticReason::ChainTruncated => "chain_truncated",
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
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

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticsSummaryDto {
    pub total: usize,
    pub cap: usize,
    pub counts: BTreeMap<String, usize>,
    pub examples: Vec<DiagnosticDto>,
}

impl From<&Diagnostics> for DiagnosticsSummaryDto {
    fn from(diagnostics: &Diagnostics) -> Self {
        let counts = [
            DiagnosticReason::InvalidJson,
            DiagnosticReason::NonObjectJson,
            DiagnosticReason::UnparsableTimestamp,
            DiagnosticReason::EncodingFallback,
            DiagnosticReason::ChainTruncated,
        ]
        .into_iter()
        .filter_map(|r| {
            let count = diagnostics.count_for(r);
            (count > 0).then(|| (reason_key(r).to_string(), count))
        })
        .collect();
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

#[derive(Debug, Clone, Serialize, PartialEq)]
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

#[derive(Debug, Clone, Serialize, PartialEq)]
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

/// Everything about the source that is not a query result.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct StatsDto {
    /// Active format, or `null` while detection is still holding the opening lines.
    pub format: Option<String>,
    /// `null` while detecting, and when a format override skipped detection.
    pub detection: Option<DetectionResultDto>,
    pub diagnostics: DiagnosticsSummaryDto,
    pub field_inventory: FieldInventoryDto,
    /// Retained entries per canonical level name.
    pub level_counts: BTreeMap<String, usize>,
    /// Retained entries.
    pub entries: usize,
    /// Lines read so far, blank and malformed ones included.
    pub total_lines: usize,
    pub blank_lines: usize,
    /// Lines dropped from retention so far (`--max-lines`).
    pub evicted_lines: u64,
    pub bytes_ingested: u64,
    pub finished: bool,
}

/// Half-open `[start_ms, end_ms)` in epoch milliseconds.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TimeRangeDto {
    pub start_ms: i64,
    pub end_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FieldTokenDto {
    pub field: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct QueryReplyDto {
    pub generation: u64,
    pub total: usize,
    /// Entries passing the filters, search and range, before chain collapsing.
    pub matched: usize,
    /// Visible rows once collapsed chains are folded into their roots.
    pub row_count: usize,
    /// Entries passing filters and search but excluded by an active range for having
    /// no timestamp.
    pub timestampless_excluded: usize,
    /// `field=value` tokens pulled out of the search text and applied as filters.
    pub tokens: Vec<FieldTokenDto>,
    /// The search text left once `tokens` were removed.
    pub remaining_text: String,
    /// Set when the search did not compile; the previous generation stays current.
    pub compile_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WindowReplyDto {
    pub generation: u64,
    pub offset: usize,
    pub row_count: usize,
    pub matched: usize,
    pub total: usize,
    pub rows: Vec<RowDto>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BucketsReplyDto {
    pub generation: u64,
    /// `null` when no retained entry has a timestamp.
    pub start_ms: Option<i64>,
    pub bucket_ms: i64,
    /// Events per bucket, ignoring filters and search (the background series).
    pub total: Vec<u64>,
    /// Events per bucket for the current match set.
    pub filtered: Vec<u64>,
    pub timestamped: usize,
    pub timestampless: usize,
    /// Timestamped entries outside the shown span.
    pub outlier_count: usize,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EntryReplyDto {
    /// `null` when the ordinal is no longer retained.
    pub row: Option<RowDto>,
    pub raw: Option<String>,
}

pub fn format_from_str(s: &str) -> Option<Format> {
    match s {
        "json" => Some(Format::Json),
        "logfmt" => Some(Format::Logfmt),
        "plain" => Some(Format::Plain),
        _ => None,
    }
}
