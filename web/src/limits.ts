// File-size thresholds for browser-side parsing (`error-states` spec, TDR §14,
// design.md D3). Both derived from measurement, not guessed — see the
// `release-hardening` entry in docs/devlog.md for the exact numbers and the command
// (Chrome DevTools Protocol `Performance.getMetrics` around a forced
// `HeapProfiler.collectGarbage`, before/after opening `bench-{50,100,200}mb.jsonl`
// through the real file picker) that produced them.
//
// Measured main-thread JS heap growth was a strikingly consistent ~3.4x the raw
// file size at every size tried (50MB: 3.40x, 100MB: 3.40x, 200MB: 3.39x) — the
// `entries: EntryDto[]` array plus `EntryIndex` this app keeps as its working set.
// Note this is a *JS-heap* measurement, not wasm32 linear memory: the WASM side
// stays roughly constant regardless of file size, because its own retained state
// (diagnostics, field inventory) is capped by `looq-core`'s
// `DEFAULT_DIAGNOSTIC_CAP`/`DEFAULT_FIELD_VALUE_CAP` — see `log-parsing-core`'s
// devlog entry — not because wasm32's address space itself is the binding
// constraint here. Parse time scaled linearly too, ~80ms/MB at these sizes (well
// under the <200ms/MB TDR §11 target even at 200MB: 15,970.6ms measured).
//
// HARD_CAP_BYTES is chosen so the resulting JS heap (cap × ~3.4) stays comfortably
// inside a single tab's practical heap budget on an ordinary desktop browser
// running alongside other tabs — not the nominal multi-GB ceiling a 64-bit browser
// process could reach in isolation.

/** Above this, parsing is allowed but the user is warned first (continue/cancel):
 * ~3s+ parses and a few hundred MB of heap start here (measured: 50MB → 2,989.1ms,
 * ~170MB heap growth). */
export const WARN_THRESHOLD_BYTES = 50 * 1024 * 1024;

/** Above this, the application refuses to start parsing at all rather than
 * starting one that would risk an unrecoverable out-of-memory failure partway
 * through. At the measured ~3.4x ratio, 400MB raw extrapolates to ~1.36GB of JS
 * heap and ~32s of parse time — a real wait, but nowhere near where this
 * environment's measurements suggested trouble (200MB completed cleanly at 678MB
 * heap with no signs of degradation). */
export const HARD_CAP_BYTES = 400 * 1024 * 1024;

export function formatBytes(bytes: number): string {
  if (bytes < 1024) {
    return `${bytes} B`;
  }
  const units = ["KB", "MB", "GB"];
  let value = bytes / 1024;
  let unitIndex = 0;
  while (value >= 1024 && unitIndex < units.length - 1) {
    value /= 1024;
    unitIndex += 1;
  }
  return `${value.toFixed(1)} ${units[unitIndex]}`;
}
