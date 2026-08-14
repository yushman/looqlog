// Shared worker-boot logic for anything that talks to the parser worker
// (worker.ts) through comlink — factored out of bridge.ts so `ParserBridge`
// (one worker per opened file, `wasm-bridge` spec) and `LiveTailSession`
// (`live-tail-ui` spec: one long-lived worker per stream, design.md D7) don't
// duplicate the worker-crash race handling.

import * as Comlink from "comlink";

import type { ParserWorkerApi } from "./parser-api";

/** Thrown when the worker fails to start or the WASM module fails to instantiate
 * (wasm-bridge spec, "Worker failure is reported, not silent"). */
export class WorkerInitError extends Error {}

export interface ParserWorkerHandle {
  worker: Worker;
  api: Comlink.Remote<ParserWorkerApi>;
  /** Rejects with `WorkerInitError` if the worker crashes at any point after
   * creation. Callers `Promise.race` every worker call against this so a crash
   * mid-call surfaces immediately instead of hanging. Pre-`.catch()`'d so an idle
   * crash (no in-flight race) doesn't log as an unhandled rejection. */
  workerError: Promise<never>;
}

/** Spawns a fresh parser worker and wraps it with comlink. Throws
 * `WorkerInitError` synchronously if the `Worker` constructor itself fails (e.g.
 * blocked by a CSP); a crash *after* construction is instead delivered through
 * the returned handle's `workerError`. */
export function spawnParserWorker(): ParserWorkerHandle {
  let worker: Worker;
  try {
    worker = new Worker(new URL("./worker.ts", import.meta.url), { type: "module" });
  } catch (err) {
    throw new WorkerInitError(`could not start the parser worker: ${String(err)}`);
  }
  const workerError = new Promise<never>((_resolve, reject) => {
    worker.addEventListener("error", (event) => {
      reject(new WorkerInitError(`parser worker crashed: ${event.message || "unknown error"}`));
    });
  });
  workerError.catch(() => undefined);
  const api = Comlink.wrap<ParserWorkerApi>(worker);
  return { worker, api, workerError };
}
