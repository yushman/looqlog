# 0005. Target-agnostic core parser crate, shared by the WASM and future MCP adapters; `rmcp` for MCP transport

- **Status:** Accepted
- **Date:** 2026-08-08

## Context

MCP server mode (US-8, F-16) is P2, but it needs the *same* parsing logic — format
detection, field extraction, timestamp parsing — as the browser WASM core, running
natively instead. A `.wasm` artifact built for `wasm32-unknown-unknown` with
`wasm-bindgen`/`web-sys` cannot run outside a browser or a WASM runtime such as
`wasmtime`, and `wasmtime` is not otherwise part of the stack (TDR §17). This decision
is made now, before any MVP core code exists, specifically to avoid a rewrite of the
parsing logic when MCP mode is built later.

## Decision

The parsing logic lives in a target-agnostic Rust crate with no `wasm-bindgen`/`web-sys`
dependency, plus two thin adapters: a `wasm-bindgen` adapter for the browser build, and
a native (`std::fs`-based) adapter for the future MCP server. When MCP mode is built,
its stdio JSON-RPC transport uses the `rmcp` crate (the official Rust MCP SDK, crates
`rmcp` + `rmcp-macros`) rather than a hand-rolled implementation, with a minimal custom
JSON-RPC/`Content-Length` framing (~200 lines) documented as the fallback if `rmcp`
proves unstable.

## Alternatives considered

### Write parsing once inside the wasm-bindgen crate, rewrite for MCP later

Simplest for MVP, since only the browser path is P0/P1 scope. Rejected: guarantees
either a rewrite or a behavioral divergence between the two parsers (format detection,
field extraction) exactly at the point where correctness matters most — an AI agent
making unsupervised queries over `looq_query`/`looq_summarize` (PRD US-8).

### Run the same `.wasm` module natively via `wasmtime` for MCP mode

Reuses a single build artifact instead of splitting the crate. Rejected: pulls a full
WASM runtime into the CLI binary for a P2 feature, and the `wasm-bindgen`/`web-sys` glue
(DOM/JS bindings) has no meaningful native host to bind to anyway — no real reuse, only
added dependency weight.

### Hand-rolled JSON-RPC/stdio framing instead of `rmcp`

Full control, no dependency on an early-stage SDK. Rejected as the default because
`rmcp` is the official SDK and gets protocol-version updates for free; kept explicitly
as the documented fallback (TDR §17 risk table), not discarded.

## Consequences

**Good:** the MVP core crate is correctly split by target from day one, at near-zero
added cost during the M2 milestone (it was already going to be its own lib crate); P2
MCP work reuses parsing behavior exactly, so what the browser shows and what an agent
reads cannot silently diverge.

**Bad / accepted cost:** slightly more Cargo workspace boilerplate during M2 than a
single crate would need — a workspace with a core crate + a wasm adapter, even though
only the wasm adapter ships in MVP. `rmcp` is chosen before any code exercises it, so
its real API stability is unverified until P2 implementation starts.

**What would make us revisit:** if `rmcp` turns out unmaintained or its API churns
heavily by the time P2 starts, fall back to the already-documented minimal hand-rolled
JSON-RPC implementation — a substitution, not a redesign.
