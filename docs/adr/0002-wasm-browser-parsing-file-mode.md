# 0002. Parse log files in browser-side WASM; backend never reads the file in file mode

- **Status:** Accepted
- **Date:** 2026-08-08

## Context

"Privacy first" is Product Principle #1 (PRD §4): the file must not leave the user's
machine, and for the security/privacy persona (PRD §3) this has to be *verifiable*, not
just claimed — US-6's acceptance criteria is literally "Network tab in DevTools is empty
after page load." The backend could instead parse the file natively in Rust: faster, no
`wasm-bindgen` overhead, no browser sandbox constraints, a much more mature ecosystem.

## Decision

For file mode, the backend never reads the log file's contents. The browser loads the
file via `<input type="file">` / File API, and a WASM module (compiled from the
target-agnostic core crate, ADR-0005) parses it entirely client-side. The backend serves
only static assets (`index.html`, JS bundle, `core.wasm`), embedded via `include_bytes!`.

## Alternatives considered

### Backend parses natively, streams parsed entries to the browser

Simpler, faster, avoids WASM entirely for file mode. Rejected: the file's content would
cross a process boundary even on localhost, which makes the "file never leaves the
browser" claim false and the empty-Network-tab test in US-6 fail — this is the single
guarantee that most differentiates the product from server-based viewers (PRD §2).

### Hybrid: opt-in server-side parsing for very large files

Already reserved in TDR §12 as `--enable-server-side-parse`, explicitly **not** default
and **not** in MVP scope. Kept as a documented escape hatch rather than folded into the
default path, so the privacy guarantee stays unconditional unless the user opts out
by name.

## Consequences

**Good:** file contents provably never leave the browser process in file mode;
testable by the user themselves (empty Network tab); continues working with the network
disabled after the page has loaded (PRD §12 privacy guarantee).

**Bad / accepted cost:** WASM parsing throughput becomes a hard constraint, not a nice-
to-have — PRD §12 flags this High-impact ("WASM парсинг медленнее нативного"), and TDR
§11 sets numeric targets (<200ms/MB JSON parse, <50ms filter on 10k lines) that must be
hit via columnar layout / SIMD. `wasm32` linear memory is effectively capped well below
the nominal 4GB (TDR §14), so files above roughly 100MB need a hard cap and/or a
windowed/streaming index — not solved in MVP, only warned about.

**What would make us revisit:** a benchmark showing WASM throughput cannot meet the
targets even after optimization — would force `--enable-server-side-parse` into MVP
scope, which would need its own explicit privacy-tradeoff warning UX before shipping.
