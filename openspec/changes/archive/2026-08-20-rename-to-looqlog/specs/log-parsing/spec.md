## MODIFIED Requirements

### Requirement: Incremental parse API
`looqlog-core` SHALL expose a parser that accepts input as successive byte chunks and returns
the entries completed by each chunk, so that a file read in pieces and a stdin stream
arriving line by line use the same code path. The parser SHALL hold back an incomplete
trailing line until the next chunk or an explicit finish call, and SHALL be target-agnostic:
no `wasm-bindgen`, no `web-sys`, no filesystem access (ADR-0005).

#### Scenario: Chunk boundary splits a line
- **WHEN** a log is fed in two chunks that split a line in the middle of its bytes
- **THEN** the parser produces exactly the same entries as feeding the whole input at once

#### Scenario: Trailing line without a newline
- **WHEN** input ends with a complete log line that has no trailing newline and the caller
  signals end of input
- **THEN** that line is emitted as an entry

#### Scenario: Line-at-a-time feeding
- **WHEN** the caller feeds one line per call, as stdin mode does
- **THEN** each call returns at most one entry and the result matches whole-input parsing
