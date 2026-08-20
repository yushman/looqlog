// Hand-written type of the wasm-bindgen glue module vendored at build time into
// `public/wasm/core.js` (built with `--no-typescript`; see scripts/build-frontend.sh
// and docs/adr/0008-vendored-frontend-artifacts.md). It is loaded at runtime via a
// `/* @vite-ignore */` dynamic import of an absolute URL, which Vite cannot resolve
// statically and which TypeScript's ambient `declare module "/path"` syntax refuses
// outright (`TS2436: Ambient module declaration cannot specify relative module
// name` — a leading `/` counts as relative for this check). So instead of an ambient
// declaration, the dynamic import's result is explicitly cast to this type in
// worker.ts — still the other half of the typed boundary alongside `wasm-types.ts`:
// `ParserHandle`'s methods return the DTOs defined there, matching
// `crates/looqlog-wasm/src/lib.rs` and `dto.rs` field-for-field.

import type {
  DetectionResultDto,
  DiagnosticsSummaryDto,
  EntryDto,
  FieldInventoryDto,
} from "./wasm-types";

/** One-to-one with `crates/looqlog-wasm/src/lib.rs`'s `ParserHandle`. */
export interface ParserHandleInstance {
  feed(chunk: Uint8Array): EntryDto[];
  finish(): EntryDto[];
  detection(): DetectionResultDto | null;
  format(): string | undefined;
  diagnosticsSummary(): DiagnosticsSummaryDto;
  fieldInventory(): FieldInventoryDto;
  entriesEmitted(): number;
  blankLines(): number;
  totalLines(): number;
  /** wasm-bindgen classes must be explicitly freed to release WASM memory. */
  free(): void;
}

export interface ParserHandleConstructor {
  /** `referenceMs`: the caller's "now" as epoch milliseconds, used to infer a year for
   * timestamp shapes that carry none (syslog RFC 3164, klog). `looqlog-core` never reads
   * the clock itself (ADR-0005), so omitting this leaves those shapes unrecognised —
   * an empty timeline for syslog and k8s logs. worker.ts passes `Date.now()`. */
  new (
    formatOverride?: string,
    tzOffsetMinutes?: number,
    referenceMs?: number,
  ): ParserHandleInstance;
}

export interface CoreWasmModule {
  /** Instantiates the WASM module. Must resolve before `ParserHandle` is used.
   * Takes an options object, not a bare path — passing a string directly hits the
   * glue's own "deprecated parameters" console warning (checked, not guessed: see
   * docs/devlog.md). */
  default(options?: { module_or_path: string }): Promise<unknown>;
  ParserHandle: ParserHandleConstructor;
}
