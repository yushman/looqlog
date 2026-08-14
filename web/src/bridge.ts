// Main-thread side of the WASM boundary (wasm-bridge spec). Owns the worker's
// lifecycle, reads the selected file in chunks via `Blob.slice`, and reports
// progress — the file is never read into one JS string up front (D3).
//
// Chunk size (design.md D3, task 2.5): 256 KiB. Measured in docs/devlog.md across
// 64 KiB / 256 KiB / 1 MiB / 4 MiB on the ~1MB fixture — counter to the initial
// guess, 1 MiB and 4 MiB (which both reduce to a single `feed()` call for this
// file) were consistently ~40ms *slower* than 64/256 KiB (~127ms vs. ~85ms
// steady-state), most likely serde-wasm-bindgen serializing one large entries
// array per call vs. several smaller ones. 256 KiB was chosen over 64 KiB for
// fewer comlink round trips at the same steady-state cost, and still yields ~200
// progress updates across a 50MB file — plenty for responsiveness.
const CHUNK_BYTES = 262144;

import * as Comlink from "comlink";

import type { FeedResult, FinishResult, ParserWorkerApi } from "./parser-api";
import { spawnParserWorker, WorkerInitError } from "./parser-worker-client";
import type { EntryDto } from "./wasm-types";

export interface ParseProgress {
  bytesConsumed: number;
  totalBytes: number;
  entriesSoFar: number;
}

export interface ParseResult {
  entries: EntryDto[];
  detection: FinishResult["detection"];
  format: FinishResult["format"];
  diagnostics: FinishResult["diagnostics"];
  fieldInventory: FinishResult["fieldInventory"];
  entriesEmitted: number;
  blankLines: number;
  totalLines: number;
}

export { WorkerInitError };

/** Thrown internally when a parse is superseded by a newer one; `parseFile` callers
 * that raced a cancelled parse see this instead of a partial/stale result. */
export class ParseCancelledError extends Error {
  constructor() {
    super("parse was cancelled");
  }
}

/**
 * One `ParserBridge` per page. `parseFile` cancels any parse already in flight
 * before starting — opening a second file always wins over the first
 * (wasm-bridge spec, "Opening a second file cancels the first parse").
 */
export class ParserBridge {
  private worker: Worker | null = null;
  private api: Comlink.Remote<ParserWorkerApi> | null = null;
  private workerError: Promise<never> | null = null;
  private generation = 0;

  private ensureWorker(): Comlink.Remote<ParserWorkerApi> {
    if (this.api) {
      return this.api;
    }
    const { worker, api, workerError } = spawnParserWorker();
    this.worker = worker;
    this.workerError = workerError;
    this.api = api;
    return this.api;
  }

  /** Cancels the in-flight parse, if any. `parseFile` calls this itself; exposed so
   * the UI can offer an explicit "cancel" action too. */
  cancel(): void {
    const api = this.api;
    const staleGeneration = this.generation;
    this.generation += 1;
    if (api) {
      void api.cancel(staleGeneration).catch(() => undefined);
    }
  }

  async parseFile(
    file: File,
    options: { formatOverride?: string; tzOffsetMinutes?: number },
    onProgress?: (progress: ParseProgress) => void,
  ): Promise<ParseResult> {
    try {
      return await this.parseFileInner(file, options, onProgress);
    } catch (err) {
      if (err instanceof WorkerInitError) {
        // The worker (or its one-shot `wasmReady` promise) is poisoned — a worker
        // that failed to instantiate WASM once will fail forever on every future
        // call, since `wasmReady` is created and awaited exactly once at
        // worker-module-evaluation time (worker.ts). Tear it down so the *next*
        // `parseFile` call builds a fresh worker from scratch instead of wedging
        // the page permanently after one transient failure (e.g. a network blip).
        this.dispose();
      }
      throw err;
    }
  }

  private async parseFileInner(
    file: File,
    options: { formatOverride?: string; tzOffsetMinutes?: number },
    onProgress?: (progress: ParseProgress) => void,
  ): Promise<ParseResult> {
    this.cancel();
    const myGeneration = this.generation;
    const api = this.ensureWorker();
    const workerError = this.workerError as Promise<never>;

    const guard = (): void => {
      if (myGeneration !== this.generation) {
        throw new ParseCancelledError();
      }
    };

    let sessionId: number;
    try {
      sessionId = await Promise.race([
        api.startSession(options.formatOverride, options.tzOffsetMinutes),
        workerError,
      ]);
    } catch (err) {
      if (err instanceof WorkerInitError) {
        throw err;
      }
      throw new WorkerInitError(`failed to instantiate the WASM parser: ${String(err)}`);
    }
    guard();

    const entries: EntryDto[] = [];
    let detection: FinishResult["detection"] = null;
    let format: FinishResult["format"];
    let offset = 0;
    const totalBytes = file.size;

    while (offset < totalBytes) {
      guard();
      const end = Math.min(offset + CHUNK_BYTES, totalBytes);
      const buffer = await file.slice(offset, end).arrayBuffer();
      const chunk = new Uint8Array(buffer);
      const result: FeedResult | null = await Promise.race([api.feed(sessionId, chunk), workerError]);
      guard();
      if (result === null) {
        throw new ParseCancelledError();
      }
      entries.push(...result.entries);
      detection = result.detection;
      format = result.format;
      offset = end;
      onProgress?.({ bytesConsumed: offset, totalBytes, entriesSoFar: entries.length });
    }

    const finalResult: FinishResult | null = await Promise.race([api.finish(sessionId), workerError]);
    guard();
    if (finalResult === null) {
      throw new ParseCancelledError();
    }
    entries.push(...finalResult.entries);
    onProgress?.({ bytesConsumed: totalBytes, totalBytes, entriesSoFar: entries.length });

    return {
      entries,
      detection: finalResult.detection ?? detection,
      format: finalResult.format ?? format,
      diagnostics: finalResult.diagnostics,
      fieldInventory: finalResult.fieldInventory,
      entriesEmitted: finalResult.entriesEmitted,
      blankLines: finalResult.blankLines,
      totalLines: finalResult.totalLines,
    };
  }

  dispose(): void {
    this.worker?.terminate();
    this.worker = null;
    this.api = null;
    this.workerError = null;
  }
}
