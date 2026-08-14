## 1. Frontend build

- [x] 1.1 Vite project under `web/` with TypeScript strict mode, producing the bundle embedded by the binary
- [x] 1.2 `tsc --noEmit` and the frontend build wired into CI
- [x] 1.3 Rebuild command from ADR-0008 now drives the real Vite build; confirm two consecutive builds are byte-identical and the freshness check still holds
- [x] 1.4 Measure the gzipped bundle against the <200KB budget in TDR §5 after the toolchain lands

## 2. WASM bridge

- [x] 2.1 `serde-wasm-bindgen` interop in `crates/looq-wasm`: entries, detection result, field inventory, diagnostics
- [x] 2.2 TypeScript types for those structures, with a deliberate mismatch test proving CI catches a renamed field
- [x] 2.3 Worker hosting the WASM module, reached through `comlink`; decide eager versus lazy creation from a measured instantiation cost
- [x] 2.4 Chunked file reading via `Blob.slice` feeding the incremental parser, with progress as a fraction of bytes consumed
- [x] 2.5 Chunk size chosen from measurement, with the number and method recorded in `docs/devlog.md`
- [x] 2.6 Cancellation by discarding the parser instance; test that opening a second file mid-parse delivers no entries from the first
- [x] 2.7 Fresh parser instance per file; test that a JSON file followed by a logfmt file detects correctly and shares no field inventory
- [x] 2.8 Worker and instantiation failures surface as a named UI error, not an empty result

## 3. App shell

- [x] 3.1 Web Components shell owning parse result, detection, diagnostics and current file; child components presentational only
- [x] 3.2 Delete the skeleton's inline script and page; the served HTML only loads the bundle
- [x] 3.3 File drop target and picker wired to the bridge, preserving the ADR-0007 hint text and the ADR-0002 no-network guarantee
- [x] 3.4 Detection surface: format, match fraction, and a visually distinct treatment for fallback or low-confidence results
- [x] 3.5 Diagnostics surface: skipped-line counts by reason with examples, encoding fallbacks, count of entries with no usable timestamp
- [x] 3.6 Provisional entry rendering — timestamp, level, message — explicitly marked as replaced by `timeline-and-table`
- [x] 3.7 Distinct initial and zero-entry states

## 4. End-to-end proof

- [x] 4.1 JSON, logfmt and plain fixtures each open and render with no fixture-specific handling and no console errors
- [x] 4.2 Displayed entries plus reported skipped lines account for every non-empty line in each fixture
- [x] 4.3 Malformed-line fixture reports its skipped line, reason and line number on screen
- [x] 4.4 Network panel stays empty after page load while opening files (US-6 still holds)
- [x] 4.5 Page stays responsive and cancellable during a 50MB parse

## 5. Measurement

- [x] 5.1 Re-measure the ~1MB browser parse number including the worker boundary; record it in `docs/devlog.md` superseding the previous figure
- [x] 5.2 Measure the worker transfer cost separately from parse time; if it dominates, batch transfers and record the change
- [x] 5.3 Resolve the design open questions: streaming versus end-of-parse rendering, generated versus hand-written types, eager versus lazy worker

## 6. Wrap-up

- [x] 6.1 Both READMEs updated if any user-visible behaviour changed
- [x] 6.2 Devlog entries per working day with the measured numbers
- [x] 6.3 `openspec validate browser-app-shell --strict` passes
- [x] 6.4 Archive the change
