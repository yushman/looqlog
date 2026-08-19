// Hand-written TypeScript mirror of crates/looq-wasm/src/dto.rs (wasm-bridge spec,
// design.md D2 open question resolved: hand-written, not tsify-generated — see
// docs/devlog.md for the reasoning). Kept in sync with the Rust DTOs by convention;
// `tsc --noEmit` in CI catches stale field usages left behind by a rename on either
// side (web/scripts/verify-rename-check.sh proves this mechanism actually fires).
//
// Field names are camelCase here because `crates/looq-wasm/src/lib.rs`'s
// `to_js()` serializer is configured with `#[serde(rename_all = "camelCase")]` on
// every DTO struct.

export type FieldValueDto =
  | { kind: "string"; value: string }
  | { kind: "number"; value: string }
  | { kind: "bool"; value: boolean }
  | { kind: "null" }
  | { kind: "json"; value: string };

export interface EntryDto {
  ordinal: number;
  /** RFC 3339, or `null` when the line carried no recognisable timestamp. */
  timestamp: string | null;
  timestampUsedDefaultTz: boolean;
  /** True when the timestamp's shape carried no year (syslog RFC 3164, klog) and one
   * was inferred from the reference instant the worker supplies. An entry dated by
   * assumption must be visibly distinguishable from one dated by the log. */
  timestampYearInferred: boolean;
  /** Canonical uppercase level name (`"INFO"`, `"ERROR"`, ...), or `null`. */
  level: string | null;
  message: string;
  fields: Record<string, FieldValueDto>;
}

/** No third variant for prefixed plain text: it reports `"threshold"` like any other
 * match, so the fallback warning stops firing on input that parsed fine (design D9 of
 * the prefix-and-payload-parsing change). */
export type DetectionOutcome = "threshold" | "fallback";

export type TimestampShapeKey =
  | "iso8601"
  | "slash-date"
  | "klog"
  | "syslog3164"
  | "clf"
  | "epoch";

export interface DetectionResultDto {
  format: string;
  matchFraction: number;
  outcome: DetectionOutcome;
  /** Which timestamp shape the sampled plain-text lines matched, and at which head
   * offset — `null` for structured formats and for input with no recognised prefixes. */
  timestampShape: TimestampShapeKey | null;
  timestampOffset: number | null;
}

export type DiagnosticReasonKey =
  | "invalid_json"
  | "non_object_json"
  | "unparsable_timestamp"
  | "encoding_fallback";

export interface DiagnosticDto {
  line: number;
  reason: DiagnosticReasonKey;
  reasonLabel: string;
  detail: string;
}

export interface DiagnosticsSummaryDto {
  total: number;
  cap: number;
  counts: Partial<Record<DiagnosticReasonKey, number>>;
  examples: DiagnosticDto[];
}

export interface FieldStatsDto {
  count: number;
  values: Record<string, number>;
  highCardinality: boolean;
}

export interface FieldInventoryDto {
  cap: number;
  fields: Record<string, FieldStatsDto>;
}
