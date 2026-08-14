## Why

`log-parsing-core` produces real entries in Rust; nothing in the browser can reach them. The
skeleton page from `bootstrap-cli-and-wasm-skeleton` still calls a hardcoded single-format stub
through ad hoc glue and renders a number. This change builds the layer between them: typed
JS↔WASM exchange, a worker so parsing does not freeze the page, and a real TypeScript app shell
with the parse result, the detected format and the parser's diagnostics actually visible.

Days 9, 10 and 14 of `docs/mvp-plan.md` are one change because they are one boundary. Day 9
builds the typed bridge, day 10 proves it end to end on three formats, day 14 replaces the
inline script with the Vite/Web Components shell — splitting them means writing the ad hoc glue
twice, since day 14's shell rewrites whatever day 9 wired into the skeleton page.

## What Changes

- Vite project with TypeScript in strict mode, producing the bundle that
  `bootstrap-cli-and-wasm-skeleton` embeds; the vendored-artifact workflow from ADR-0008 now
  covers a real build rather than a hand-written file.
- Web Components shell replacing the skeleton's inline script: file drop target, header, and a
  results area, with no styling ambitions beyond legibility.
- Typed WASM interop through `serde-wasm-bindgen`: entries, the detection result and the
  diagnostics cross the boundary as typed values instead of console logs.
- The parser runs in a Web Worker behind `comlink`, so parsing a large file leaves the page
  responsive and cancellable.
- The file is fed to the parser in chunks with progress reported, matching the incremental API
  decided in `log-parsing-core`.
- The detected format, its match fraction and the override state are displayed, so a
  misclassification is visible to the user rather than only present in the data.
- Parser diagnostics — skipped lines, encoding fallbacks, unparsable timestamps — are surfaced
  in the UI with their counts and examples.
- An unstyled table dump of parsed entries, enough to prove the pipeline and to be replaced by
  the real virtual table in `timeline-and-table`.
- Opening a second file uses a fresh parser instance, with the previous result discarded.

Not in this change: timeline, virtual scrolling, filters, search, URL hash, themes, live tail
over `/ws`, and any styling beyond what makes the output readable.

## Capabilities

### New Capabilities

- `wasm-bridge`: the typed JS↔WASM boundary, the worker it runs in, chunked feeding, progress,
  cancellation, and parser instance lifecycle.
- `app-shell`: the TypeScript/Web Components application structure, its build, and the surfaces
  that display parse results, detection and diagnostics.

### Modified Capabilities

- `browser-file-loading`: the provisional single-format WASM entry point from
  `bootstrap-cli-and-wasm-skeleton` is replaced by the real multi-format parser reached through
  the worker; the file-picker and no-network guarantees are unchanged.

## Impact

- New: `web/` Vite project, `crates/looq-wasm` grows the typed interop surface, vendored bundle
  artifacts get rebuilt by a real toolchain.
- Dependencies: `vite`, `typescript`, `comlink`, `serde-wasm-bindgen`. Bundle size is measured
  against the <200KB gzipped budget in TDR §5 as they land.
- The skeleton page and its inline script are deleted, not kept alongside.
- Constrains later changes: every UI change after this one is a Web Component in this shell, and
  every parser call goes through the worker, including live tail's per-line feeding.

### Ordering note

This change assumes `bootstrap-cli-and-wasm-skeleton` and `log-parsing-core` are archived first,
so `openspec/specs/browser-file-loading/` exists for the delta above to modify. If the order
changes, the delta has to be re-expressed against whatever is in `openspec/specs/` at the time.
