// Web Worker hosting the WASM parser (wasm-bridge spec: "Parsing runs off the main
// thread"). The main thread never holds a synchronous reference to the WASM module —
// it only talks to this worker through the comlink proxy built in bridge.ts.
//
// Eager vs. lazy WASM instantiation (design.md open question): this worker
// instantiates WASM eagerly, at worker-script-evaluation time, not lazily on first
// `startSession`. The worker itself is only created lazily by the main thread (on
// first file open, see bridge.ts) — so the cost of spinning up a worker thread is
// only paid by a user who opens a file, but once that worker exists, instantiating
// WASM immediately means the ~instantiation-cost measured in docs/devlog.md is paid
// once, off the file-open critical path, rather than stacked onto the time-to-first-
// entry of whichever file happens to be opened first.

import * as Comlink from "comlink";

import type { CoreWasmModule, ParserHandleInstance } from "./core-wasm-types";
import type { FeedResult, FinishResult, ParserWorkerApi } from "./parser-api";

// Loaded via an absolute-URL dynamic import rather than a normal ESM specifier:
// `core.js`/`core.wasm` are wasm-pack output vendored into `public/wasm/` (not part
// of the Vite module graph — see docs/adr/0008-vendored-frontend-artifacts.md and
// scripts/build-frontend.sh), so Vite cannot statically resolve or bundle them.
// The specifier is read from a `const` variable rather than inlined as a string
// literal: a literal makes `tsc` attempt static module resolution on the import
// expression itself (failing with TS2307 even under an `as` cast, since a type
// assertion cannot suppress a module-resolution error); a variable of type `string`
// makes the expression's type `Promise<any>`, which the cast below then narrows to
// `CoreWasmModule` (core-wasm-types.ts) — the other half of the typed boundary
// alongside `wasm-types.ts`.
const CORE_WASM_JS_PATH: string = "/wasm/core.js";
const wasmModulePromise = import(/* @vite-ignore */ CORE_WASM_JS_PATH) as Promise<CoreWasmModule>;
// Explicit path: the glue's own default lookup (`new URL('core_bg.wasm', ...)`)
// assumes the wasm file is named `core_bg.wasm` next to `core.js`; this repo names
// it `core.wasm` to match the route `crates/looq/src/server.rs` serves and the
// `local-server` spec's "core.wasm" naming, so the path is passed explicitly.
const wasmReady: Promise<void> = wasmModulePromise
  .then((mod) => mod.default({ module_or_path: "/wasm/core.wasm" }))
  .then(() => undefined);

let nextSessionId = 1;
let active: { id: number; handle: ParserHandleInstance } | null = null;

function summarize(handle: ParserHandleInstance) {
  return {
    detection: handle.detection(),
    format: handle.format(),
  };
}

const api: ParserWorkerApi = {
  async startSession(formatOverride, tzOffsetMinutes) {
    await wasmReady;
    const { ParserHandle } = await wasmModulePromise;
    // Discard the previous instance (design.md D4) before constructing the new one.
    active?.handle.free();
    const id = nextSessionId++;
    active = { id, handle: new ParserHandle(formatOverride, tzOffsetMinutes) };
    return id;
  },

  async feed(sessionId, chunk): Promise<FeedResult | null> {
    if (active === null || active.id !== sessionId) {
      // The instance this call was meant for is gone — either cancelled or
      // superseded by a newer session. Nothing to feed into; return null rather
      // than silently parsing into the wrong session's state.
      return null;
    }
    const entries = active.handle.feed(chunk);
    const { detection, format } = summarize(active.handle);
    return { entries, detection, format };
  },

  async finish(sessionId): Promise<FinishResult | null> {
    if (active === null || active.id !== sessionId) {
      return null;
    }
    const entries = active.handle.finish();
    const { detection, format } = summarize(active.handle);
    const result: FinishResult = {
      entries,
      detection,
      format,
      diagnostics: active.handle.diagnosticsSummary(),
      fieldInventory: active.handle.fieldInventory(),
      entriesEmitted: active.handle.entriesEmitted(),
      blankLines: active.handle.blankLines(),
      totalLines: active.handle.totalLines(),
    };
    return result;
  },

  async cancel(sessionId) {
    if (active !== null && active.id === sessionId) {
      active.handle.free();
      active = null;
    }
  },
};

Comlink.expose(api);
