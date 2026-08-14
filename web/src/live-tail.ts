// Stream session logic for live tail (`stdin-stream`/`live-tail-ui` specs,
// `entry-index`/`timeline`/`entry-table` specs, design.md D1-D8). No DOM here —
// `components/looq-live-tail.ts` owns rendering and polls this class on a timer
// (D6: batched rendering, decoupled from the lines/sec counter).
//
// The entry index mirrors the backend's two-buffer split (`crates/looq/src/
// stdin.rs`'s module doc): it is this session's own entry retention, evicted at
// `maxLines` (D4) independent of the WebSocket transport; the WebSocket layer below
// only ever forwards what the backend sends, unbuffered on this side beyond the
// browser's own socket buffers. Lost lines (gaps) are tracked as a running total
// rather than positioned rows — the virtual-scrolled table (`entry-table` spec)
// has no notion of a non-entry row, so a gap surfaces as a persistent count in the
// component instead of an inline marker the old capped list used to show.

import { EntryIndex } from "./entry-index";
import type { FinishResult } from "./parser-api";
import { spawnParserWorker, WorkerInitError, type ParserWorkerHandle } from "./parser-worker-client";
import type { DetectionResultDto, DiagnosticsSummaryDto, EntryDto, FieldInventoryDto } from "./wasm-types";
import type { ServerMessageDto } from "./ws-types";

export { WorkerInitError };

export type ConnectionState = "connecting" | "live" | "ended" | "disconnected";

export interface StreamSummary {
  detection: DetectionResultDto | null;
  format: string | undefined;
  diagnostics: DiagnosticsSummaryDto | null;
  fieldInventory: FieldInventoryDto | null;
  entriesEmitted: number;
  blankLines: number;
  totalLines: number;
}

/**
 * One long-lived parser instance for the life of the stream (D7) — unlike
 * `bridge.ts`'s `ParserBridge`, which discards and reconstructs a fresh instance
 * per file. Lines are fed one at a time through the same worker API
 * (`parser-api.ts`); the underlying `looq_core::Parser` already holds opening lines
 * until its detection sample fills (D5's "hold" is free), so the only thing this
 * class adds is the timeout escape hatch: forcing `finish()` early flushes and
 * finalizes detection on whatever arrived, without ending the session — nothing in
 * the worker API marks a session "closed", so `feed()` calls after a `finish()`
 * keep working (verified against `crates/looq-core/src/parser.rs`: `finish()` only
 * flushes a trailing partial line and finalizes detection if still sampling; it does
 * not reset or invalidate the instance).
 */
export class StreamParserSession {
  private handle: ParserWorkerHandle;
  private sessionId: Promise<number>;
  private queue: Promise<void> = Promise.resolve();
  private detectionResolved = false;
  /** Set the moment `feedLine` is first called (not when it *completes* — being
   * queued is enough to prove real data is on its way), and the timestamp of that
   * first call. Guards `doRefresh` against forcing detection on a still-empty
   * sample: `looq_core::Parser::finalize_detection` on an empty sample is a
   * *permanent* fallback to plain text (state can never re-enter Sampling), so
   * `doRefresh` must never call `finish()` while zero lines have ever been fed —
   * not even after a timeout, because "no data yet" and "never going to get data"
   * are indistinguishable from a fixed deadline, and guessing wrong locks the
   * stream into misparsing every line that arrives afterward. D5's timeout is for
   * "a few lines, then quiet" (design.md: "a producer emits only two lines and
   * then goes quiet" — two, not zero), not for "nothing has arrived yet". A first
   * implementation used a deadline that fired regardless of `everFed` and
   * silently misdetected any stream whose first line arrived after it — caught via
   * a real-server Playwright run during this change's own verification (see
   * docs/devlog.md). */
  private everFed = false;
  private firstFedAt: number | null = null;
  /** Once at least one line has been fed, wait this long for a few more to arrive
   * before forcing detection — a compromise between "detect immediately off one
   * line" (works, but a bigger sample is a better sample) and D5's "short" bound. */
  private static readonly SETTLE_MS = 300;

  constructor(
    private readonly onEntries: (entries: EntryDto[]) => void,
    private readonly onSummary: (summary: StreamSummary) => void,
  ) {
    this.handle = spawnParserWorker();
    this.sessionId = this.handle.api.startSession();
  }

  /** Runs `fn` after every previously queued feed/gap task has settled — the single
   * ordering primitive that keeps gap markers (pushed synchronously by
   * `LiveTailSession`) interleaved correctly with the entries a `feedLine` call
   * produces asynchronously later. */
  enqueue(fn: () => void | Promise<void>): void {
    this.queue = this.queue.then(fn).catch((err) => {
      // A worker crash surfaces through `workerError`, awaited inside `doFeed`
      // below; anything else reaching here is unexpected and shouldn't wedge the
      // queue silently.
      console.error("live-tail parser queue task failed:", err);
    });
  }

  feedLine(text: string): void {
    if (!this.everFed) {
      this.everFed = true;
      this.firstFedAt = Date.now();
    }
    this.enqueue(() => this.doFeed(text));
  }

  /** Forces detection to finalize now if it is still sampling (D5's timeout
   * escape hatch), and refreshes the cumulative summary (diagnostics, field
   * inventory, counts) — safe to call repeatedly; a no-op beyond the summary
   * refresh once detection has already resolved. No-ops entirely (does not call
   * the worker at all) if nothing has been fed yet at all, or if it's within
   * `SETTLE_MS` of the first line — see `everFed`'s doc comment. */
  refreshSummary(): void {
    this.enqueue(() => this.doRefresh());
  }

  private async doFeed(text: string): Promise<void> {
    const id = await this.sessionId;
    const bytes = new TextEncoder().encode(text + "\n");
    const result = await Promise.race([this.handle.api.feed(id, bytes), this.handle.workerError]);
    if (result === null) {
      return;
    }
    if (result.entries.length > 0) {
      this.onEntries(result.entries);
    }
    if (result.detection) {
      this.detectionResolved = true;
    }
  }

  private async doRefresh(): Promise<void> {
    if (!this.everFed) {
      // Nothing has ever been fed — never force detection on an empty sample (see
      // `everFed`'s doc comment). Once real data exists this stops applying, for
      // the life of the session; until then, "detecting..." stays on screen
      // honestly rather than a guessed answer.
      return;
    }
    if (Date.now() - this.firstFedAt! < StreamParserSession.SETTLE_MS) {
      // Give a few more lines a chance to land in the same sample before forcing.
      return;
    }
    const id = await this.sessionId;
    const result: FinishResult | null = await Promise.race([
      this.handle.api.finish(id),
      this.handle.workerError,
    ]);
    if (result === null) {
      return;
    }
    if (result.entries.length > 0) {
      this.onEntries(result.entries);
    }
    if (result.detection) {
      this.detectionResolved = true;
    }
    this.onSummary({
      detection: result.detection,
      format: result.format,
      diagnostics: result.diagnostics,
      fieldInventory: result.fieldInventory,
      entriesEmitted: result.entriesEmitted,
      blankLines: result.blankLines,
      totalLines: result.totalLines,
    });
  }

  get hasDetectionResolved(): boolean {
    return this.detectionResolved;
  }

  dispose(): void {
    void this.sessionId
      .then((id) => this.handle.api.cancel(id))
      .catch(() => undefined);
    this.handle.worker.terminate();
  }
}

const RECONNECT_BASE_MS = 500;
const RECONNECT_MAX_MS = 10_000;

/**
 * Owns the `/ws` connection lifecycle (connect, reconnect with capped exponential
 * backoff), sequence-number bookkeeping (gap detection from two independent
 * sources — explicit `gap` messages and sequence discontinuity, D2), client-side
 * eviction at `maxLines` (D4), and the one long-lived parser instance for the
 * stream (D7). No rendering — `looq-live-tail.ts` polls `getIndex()`/`getSummary()`
 * on its own timers.
 */
export class LiveTailSession {
  private ws: WebSocket | null = null;
  private reconnectAttempt = 0;
  private reconnectTimer: ReturnType<typeof setTimeout> | null = null;
  private manuallyClosed = false;
  private state: ConnectionState = "connecting";

  private entryCount = 0;
  private evictedTotal = 0;
  private gapTotal = 0;
  private lastSeqSeen: number | null = null;
  private linesSinceRateSample = 0;

  // The time-ordered index behind the timeline/table (`entry-index` spec,
  // design.md D1/D2) — this stream's one entry-retention structure, evicted at
  // `maxLines` (D4) same as before, just no longer duplicated into a separate
  // capped `rows` list for rendering.
  private index = new EntryIndex();

  private parser: StreamParserSession;
  private summary: StreamSummary = {
    detection: null,
    format: undefined,
    diagnostics: null,
    fieldInventory: null,
    entriesEmitted: 0,
    blankLines: 0,
    totalLines: 0,
  };

  constructor(
    private readonly wsUrl: string,
    private readonly maxLines: number,
    /** Sent as the first message on every connection attempt, including
     * reconnects (`security` spec, design.md D1) — the token itself never
     * appears in `wsUrl`. */
    private readonly authToken: string,
    private readonly onStateChange: (state: ConnectionState) => void,
  ) {
    this.parser = new StreamParserSession(
      (entries) => this.onParsedEntries(entries),
      (summary) => {
        this.summary = summary;
      },
    );
  }

  connect(): void {
    this.manuallyClosed = false;
    this.openSocket();
  }

  private openSocket(): void {
    this.setState("connecting");
    let ws: WebSocket;
    try {
      ws = new WebSocket(this.wsUrl);
    } catch {
      this.setState("disconnected");
      this.scheduleReconnect();
      return;
    }
    this.ws = ws;

    ws.addEventListener("open", () => {
      // The one and only message this client ever sends (`security` spec): the
      // handshake token, as the very first frame, before the server will
      // subscribe to stdin or read the snapshot. Resent on every reconnect —
      // each new WebSocket connection needs its own auth, even within the same
      // page/token lifetime.
      ws.send(JSON.stringify({ type: "auth", token: this.authToken }));
    });

    ws.addEventListener("message", (event) => {
      this.reconnectAttempt = 0;
      if (this.state !== "ended") {
        this.setState("live");
      }
      this.handleMessage(String(event.data));
    });

    ws.addEventListener("close", () => {
      this.ws = null;
      if (this.manuallyClosed || this.state === "ended") {
        return;
      }
      this.setState("disconnected");
      this.scheduleReconnect();
    });

    // 'error' is always followed by 'close' for a WebSocket; reconnect scheduling
    // lives in the close handler so it only runs once per failed connection.
    ws.addEventListener("error", () => undefined);
  }

  private scheduleReconnect(): void {
    if (this.reconnectTimer !== null) {
      return;
    }
    const delay = Math.min(RECONNECT_BASE_MS * 2 ** this.reconnectAttempt, RECONNECT_MAX_MS);
    this.reconnectAttempt += 1;
    this.reconnectTimer = setTimeout(() => {
      this.reconnectTimer = null;
      if (!this.manuallyClosed) {
        this.openSocket();
      }
    }, delay);
  }

  private setState(state: ConnectionState): void {
    if (this.state === state) {
      return;
    }
    this.state = state;
    this.onStateChange(state);
  }

  private handleMessage(raw: string): void {
    let msg: ServerMessageDto;
    try {
      msg = JSON.parse(raw) as ServerMessageDto;
    } catch {
      // A malformed frame is a protocol bug, not a reason to crash the tab — drop
      // it and keep going (project testing rule: loud in dev, never a hard crash).
      console.error("live-tail: could not parse server message:", raw);
      return;
    }
    switch (msg.type) {
      case "snapshot":
        this.handleSnapshot(msg.lines, msg.lastSeq);
        break;
      case "line":
        this.ingestLine(msg.seq, msg.text);
        this.linesSinceRateSample += 1;
        break;
      case "gap":
        this.handleGap(msg.count);
        break;
      case "ended":
        this.setState("ended");
        // Force a final summary refresh so cumulative counts reflect the last
        // lines even if the detection-hold/summary timer hasn't ticked since.
        this.parser.refreshSummary();
        break;
    }
  }

  private handleSnapshot(lines: { seq: number; text: string }[], lastSeq: number): void {
    if (this.lastSeqSeen !== null && lastSeq < this.lastSeqSeen) {
      // Sequence numbers are monotonic for the life of one backend process
      // (stdin.rs's `StdinBuffer`) — a *lower* last_seq than what this client
      // already saw can only mean the backend process itself restarted (a fresh
      // `looq --stdin` run, not a network blip reconnecting to the same one).
      // design.md's reconnect scenarios all assume the same running process; a
      // full restart is a narrower edge case they don't cover. Treating this
      // snapshot as first-connect (reprocess everything in it, keep existing rows
      // rather than erasing real data the user already saw) avoids the
      // alternative — dedup-by-seq comparing two different processes' numbering
      // spaces, which would silently treat all of this snapshot's lines as
      // already-seen and drop them.
      this.lastSeqSeen = null;
    }
    const first = this.lastSeqSeen === null;
    for (const line of lines) {
      if (!first && line.seq <= this.lastSeqSeen!) {
        continue; // already delivered before a reconnect (D3 dedup)
      }
      this.ingestLine(line.seq, line.text);
    }
    if (first) {
      this.lastSeqSeen = lastSeq;
    } else if (lastSeq > this.lastSeqSeen!) {
      // The buffer's start moved past what this client already had while it was
      // disconnected (ring buffer eviction, not a live backpressure drop) — still
      // real data loss from the client's point of view, so it gets a gap marker
      // too (the empty-snapshot-lines edge case the per-line loop above can't see).
      this.enqueueGapMarker(lastSeq - this.lastSeqSeen!);
      this.lastSeqSeen = lastSeq;
    }
  }

  private ingestLine(seq: number, text: string): void {
    if (this.lastSeqSeen !== null && seq > this.lastSeqSeen + 1) {
      this.enqueueGapMarker(seq - this.lastSeqSeen - 1);
    }
    this.lastSeqSeen = seq;
    this.parser.feedLine(text);
  }

  private handleGap(count: number): void {
    if (count <= 0) {
      return;
    }
    this.enqueueGapMarker(count);
    if (this.lastSeqSeen !== null) {
      // Advance the baseline by the reported count so the *next* line's sequence
      // number lines up with no further discontinuity — this is what prevents the
      // explicit gap message and the sequence-discontinuity check from double-
      // counting the same drop (D2: two independent sources, one marker).
      this.lastSeqSeen += count;
    }
  }

  private enqueueGapMarker(count: number): void {
    if (count <= 0) {
      return;
    }
    // A running total has no ordering requirement the way an interleaved row list
    // would — unlike the old capped DOM list, nothing here needs the gap
    // positioned relative to entries still resolving asynchronously in the
    // parser's queue, so this can just increment directly.
    this.gapTotal += count;
  }

  private onParsedEntries(entries: EntryDto[]): void {
    this.index.append(entries);
    this.entryCount += entries.length;
    if (this.entryCount > this.maxLines) {
      const overflow = this.entryCount - this.maxLines;
      this.index.evictFront(overflow);
      this.entryCount -= overflow;
      this.evictedTotal += overflow;
    }
  }

  // ---- read-only state for the component's render timers -----------------------

  /** The time-ordered index behind the timeline/table (`entry-index` spec). */
  getIndex(): EntryIndex {
    return this.index;
  }

  getSummary(): StreamSummary {
    return this.summary;
  }

  getGapTotal(): number {
    return this.gapTotal;
  }

  getEvictedTotal(): number {
    return this.evictedTotal;
  }

  getConnectionState(): ConnectionState {
    return this.state;
  }

  /** Forces a detection/summary refresh (D5's hold-timeout escape hatch); called by
   * the component shortly after connecting and periodically thereafter. */
  refreshSummary(): void {
    this.parser.refreshSummary();
  }

  /** Lines received live (not from a snapshot) since the last call; resets the
   * counter. Used for the throttled lines/sec indicator, on its own timer,
   * independent of the entry-rendering batch (D6/task 4.2). */
  takeLineCount(): number {
    const n = this.linesSinceRateSample;
    this.linesSinceRateSample = 0;
    return n;
  }

  dispose(): void {
    this.manuallyClosed = true;
    if (this.reconnectTimer !== null) {
      clearTimeout(this.reconnectTimer);
      this.reconnectTimer = null;
    }
    this.ws?.close();
    this.ws = null;
    this.parser.dispose();
  }
}
