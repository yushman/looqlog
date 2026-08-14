## Context

Two halves exist and do not touch: a Rust parser that returns entries, diagnostics and a field
inventory, and a browser page that can pick a file and call one hardcoded function. This change
is the seam. It is also the last moment where the frontend has no structure — every UI change
after it inherits whatever component model, state ownership and build setup get chosen here.

The constraint that shapes most decisions: WASM parsing is synchronous and can run for seconds
on a large file. On the main thread that is a frozen tab, which is the first thing a user
notices and the last thing they forgive.

## Goals / Non-Goals

**Goals:**
- A typed, testable boundary between TypeScript and Rust.
- A parse that never freezes the page and can be cancelled.
- Detection results and diagnostics visible on screen, since a parser that reports problems
  only to a console has not really reported them.
- A component structure and build the rest of the UI can be built inside.

**Non-Goals:**
- Visual design. Legible, not styled; themes are `release-hardening`.
- Timeline, virtual scrolling, filters, search, URL state — each has its own change.
- Live tail. `/ws` exists but nothing in this change consumes it.
- Streaming render of entries as they parse: entries become available incrementally, but the
  provisional table may render once at the end.

## Decisions

### D1 — Worker plus `comlink`, not main-thread WASM

The parser lives in a Web Worker and the main thread talks to it through `comlink`'s proxy.
The alternative — instantiate WASM on the main thread, which is simpler and avoids a transfer
cost per chunk — was rejected because the freeze it causes scales with file size, and the file
sizes this product targets are exactly where it becomes unacceptable. Retrofitting a worker
later means moving all parser state across a boundary that the UI has meanwhile been written to
call synchronously.

Cost to accept: every parser call becomes async, and the entry data gets copied across the
worker boundary. The copy is measured in this change, and if it dominates the parse time, the
fallback is transferring entries in batches rather than per chunk.

### D2 — Typed interop with a CI type check, not hand-written glue

`serde-wasm-bindgen` on the Rust side, generated or hand-maintained TypeScript types on the JS
side, and `tsc --noEmit` in CI. TDR §4 already lists this as the risk "tsify ↔ WASM
compatibility"; the mitigation only works if the check actually runs on every change. Without
it, a renamed field in Rust becomes `undefined` in the UI at runtime, on some code path nobody
opened during development.

### D3 — Chunked feeding, with the chunk size measured

The bridge reads the file with `Blob.slice` and feeds chunks to the incremental parser from
`log-parsing-core`. Reading the whole file into one JS string first would double peak memory
and delay the first entry until the last byte is read. Chunk size is a tunable with a measured
default, not a guess: too small and per-call overhead dominates, too large and the worker stops
yielding progress.

### D4 — Cancellation by parser instance, not by flag

Cancelling a parse discards the parser instance rather than setting a "stop" flag the parse
loop checks. It is simpler to reason about, guarantees no stale entries arrive from the old
parse, and matches the fresh-instance-per-file rule that `log-parsing-core` needs for its state
not to leak between files.

### D5 — Diagnostics are a first-class UI surface

Skipped lines, encoding fallbacks and entries without timestamps get displayed with counts and
examples, not logged. This is the change where the project's silent-failure list stops being a
parser concern and becomes a product one: a log viewer that quietly shows 999 of 1000 lines is
worse than one that refuses to open the file, because the user draws conclusions from what they
see.

### D6 — Provisional rendering, named as provisional

Entries render as a plain unstyled list or table. It exists to prove the pipeline and to be
deleted by `timeline-and-table`. Naming it provisional in the spec is what stops it from
quietly becoming the real table by accretion.

### D7 — State lives in the shell, not in components

The parse result, detection, diagnostics and the current file are owned by the shell element
and passed down; child components stay presentational. With filters, search, a timeline range
and URL state all arriving in later changes, state scattered across components is the thing
that makes each of those changes harder than the one before.

## Risks / Trade-offs

- **Worker transfer cost eats the parse budget** → Measure the boundary cost separately from
  parse time in this change; batch transfers if it dominates.
- **Bundle size budget (<200KB gzipped, TDR §5) breaks once Vite, comlink and the shell land** →
  Measure after each addition; `comlink` is small, but the budget has no slack recorded yet.
- **Vite output not byte-reproducible, breaking ADR-0008's freshness check** → Pin versions,
  verify two consecutive builds match, and fall back to a normalised-hash comparison if not.
- **Async everywhere makes ordering bugs easy** → A cancelled parse must never deliver entries;
  covered by an explicit test that opens a second file mid-parse.
- **The provisional table becomes permanent** → `timeline-and-table` deletes it; the spec says
  so, and the task list there starts with that deletion.

## Open Questions

- Should entries stream into the UI as they parse, or land once at the end? Streaming is better
  for large files and worse for the provisional renderer, and the real answer depends on the
  virtual table that does not exist yet.
- Where do TypeScript types for the WASM structures come from — generated by `tsify`, or written
  by hand and guarded by the CI check? Generation is less duplication, hand-written is fewer
  moving parts in the build that ADR-0008 requires to stay reproducible.
- Does the worker get created eagerly at page load or lazily at first file open? Eager hides
  WASM instantiation latency, lazy avoids paying it for a user who never opens a file.
