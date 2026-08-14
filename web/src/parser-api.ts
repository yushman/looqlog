// The comlink-exposed contract between the main thread (bridge.ts) and the worker
// (worker.ts). Kept in its own DOM-free module so it can be type-checked under both
// tsconfig.json (main thread) and tsconfig.worker.json (worker) without a `lib`
// conflict (see web/tsconfig*.json).

import type {
  DetectionResultDto,
  DiagnosticsSummaryDto,
  EntryDto,
  FieldInventoryDto,
} from "./wasm-types";

export interface FeedResult {
  entries: EntryDto[];
  detection: DetectionResultDto | null;
  format: string | undefined;
}

export interface FinishResult {
  entries: EntryDto[];
  detection: DetectionResultDto | null;
  format: string | undefined;
  diagnostics: DiagnosticsSummaryDto;
  fieldInventory: FieldInventoryDto;
  entriesEmitted: number;
  blankLines: number;
  totalLines: number;
}

/**
 * The worker-side API, exposed through `comlink.expose()` (wasm-bridge spec: "Parsing
 * runs off the main thread", "Each file gets a fresh parser instance", "A parse can be
 * cancelled").
 *
 * Cancellation (design.md D4) is by discarding the parser instance, not a flag: a
 * `sessionId` from a call to `startSession` older than the worker's currently-held
 * instance refers to an instance that no longer exists, so `feed`/`finish` for it
 * return `null` — not because a flag says "stop", but because there is nothing left
 * to feed. See worker.ts for the implementation.
 */
export interface ParserWorkerApi {
  /** Discards any previous parser instance and constructs a fresh one. Returns the
   * new session id, which every subsequent call must carry. */
  startSession(formatOverride?: string, tzOffsetMinutes?: number): Promise<number>;
  /** `null` if `sessionId` is no longer the active session (superseded or cancelled). */
  feed(sessionId: number, chunk: Uint8Array): Promise<FeedResult | null>;
  /** `null` if `sessionId` is no longer the active session. */
  finish(sessionId: number): Promise<FinishResult | null>;
  /** Explicit cancel: discards `sessionId`'s parser instance if it is still active. */
  cancel(sessionId: number): Promise<void>;
}
