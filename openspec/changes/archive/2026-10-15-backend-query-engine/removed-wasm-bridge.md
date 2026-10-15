## REMOVED Requirements

### Requirement: Parsing runs off the main thread
**Reason**: Parsing runs in the backend process (ADR-0010); there is no worker.
**Migration**: The page stays responsive because it only renders query results (`query-api`).

### Requirement: Typed values cross the boundary
**Reason**: No JS↔WASM boundary remains.
**Migration**: Row and result shapes cross as JSON over `/ws` (`query-api`); their TypeScript
types are checked by `tsc --noEmit` against a Rust-side round-trip test.

### Requirement: Files are fed in chunks with progress
**Reason**: Chunking and progress move to the backend.
**Migration**: `file-ingest`, "Ingest reports progress".

### Requirement: A parse can be cancelled
**Reason**: Cancellation moves to the backend.
**Migration**: `file-ingest`, "A new ingest cancels the previous one".

### Requirement: Each file gets a fresh parser instance
**Reason**: Parser lifecycle moves to the backend.
**Migration**: `file-ingest`, "A new ingest cancels the previous one" — the new file's session
carries nothing from the previous one.
