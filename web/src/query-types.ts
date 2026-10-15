// Hand-written TypeScript mirror of the `/ws` protocol (`crates/looqlog/src/protocol.rs`)
// and the reply shapes it carries (`crates/looqlog-core/src/dto.rs`), `query-api` spec.
// Kept in sync by convention; `tsc --noEmit` catches stale field usages after a rename.

export type FieldValueDto =
  | { kind: "string"; value: string }
  | { kind: "number"; value: string }
  | { kind: "bool"; value: boolean }
  | { kind: "null" }
  | { kind: "json"; value: string };

export interface RowDto {
  ordinal: number;
  /** RFC 3339, or `null` when the line carried no recognisable timestamp. */
  timestamp: string | null;
  timestampUsedDefaultTz: boolean;
  /** True when the timestamp shape carried no year and one was inferred. */
  timestampYearInferred: boolean;
  /** Canonical uppercase level name, or `null`. */
  level: string | null;
  message: string;
  fields: Record<string, FieldValueDto>;
  /** Ordinal of the chain root this line continues, or `null`. */
  continuationOf: number | null;
  /** Lines in the chain this row roots (itself included) within the current view; 0
   * when it roots no visible chain. */
  chainSize: number;
  expanded: boolean;
  chainMember: boolean;
  /** Search matches in `message`, `[start, end)` in UTF-16 units. */
  matchRanges: Array<[number, number]>;
}

export type DetectionOutcome = "threshold" | "fallback";

export type TimestampShapeKey = "iso8601" | "slash-date" | "klog" | "syslog3164" | "clf" | "epoch";

export interface DetectionResultDto {
  format: string;
  matchFraction: number;
  outcome: DetectionOutcome;
  timestampShape: TimestampShapeKey | null;
  timestampOffset: number | null;
}

export type DiagnosticReasonKey =
  | "invalid_json"
  | "non_object_json"
  | "unparsable_timestamp"
  | "encoding_fallback"
  | "chain_truncated";

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

export interface StatsDto {
  format: string | null;
  detection: DetectionResultDto | null;
  diagnostics: DiagnosticsSummaryDto;
  fieldInventory: FieldInventoryDto;
  levelCounts: Record<string, number>;
  entries: number;
  totalLines: number;
  blankLines: number;
  evictedLines: number;
  bytesIngested: number;
  finished: boolean;
}

export interface SourceInfoDto {
  kind: "file" | "stdin";
  name: string | null;
  totalBytes: number | null;
  error: string | null;
  warning: string | null;
  ended: boolean;
  warnBytes: number;
  capBytes: number;
}

export interface StatsReplyDto {
  source: SourceInfoDto;
  stats: StatsDto;
}

export interface TimeRangeDto {
  startMs: number;
  endMs: number;
}

export interface QueryReplyDto {
  generation: number;
  total: number;
  matched: number;
  rowCount: number;
  timestamplessExcluded: number;
  tokens: Array<{ field: string; value: string }>;
  remainingText: string;
  compileError: string | null;
}

export interface WindowReplyDto {
  generation: number;
  offset: number;
  rowCount: number;
  matched: number;
  total: number;
  rows: RowDto[];
}

export interface BucketsReplyDto {
  generation: number;
  startMs: number | null;
  bucketMs: number;
  total: number[];
  filtered: number[];
  timestamped: number;
  timestampless: number;
  outlierCount: number;
}

export interface EntryReplyDto {
  row: RowDto | null;
  raw: string | null;
}

export type ClientRequest =
  | {
      type: "query";
      filters: Record<string, string[]>;
      search: string;
      range: TimeRangeDto | null;
      toggled?: number[];
    }
  | { type: "window"; offset: number; limit: number; anchor?: number }
  | { type: "entry"; ordinal: number }
  | { type: "buckets"; span?: TimeRangeDto; fullSpan: boolean }
  | { type: "set_format"; format: string | null; tzOffsetMinutes: number | null }
  | { type: "toggle_chain"; root: number }
  | { type: "stats" };

export type Notification =
  | { type: "progress"; bytes: number; total: number; done: boolean }
  | { type: "grown"; total: number; matched: number; rowCount: number }
  | { type: "evicted"; count: number }
  | { type: "ended" }
  | { type: "reset" };

export type ServerMessage =
  | { type: "reply"; id: number; data: unknown }
  | { type: "error"; id: number; message: string }
  | Notification;
