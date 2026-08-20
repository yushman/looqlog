## Why

A stack trace, a Python traceback and a pretty-printed JSON payload are each one
event that happens to span several physical lines. Today every one of those lines
becomes its own `Entry`, so a single `SocketTimeoutException` arrives as ~60 rows,
inflates the timeline into a spike that looks like sixty failures, and — for the
prefix-less case — arrives without a timestamp or a level at all. This is PRD §14
Open Question #3, open since the beginning and marked P2, and it is the direct debt
left by the Android bugreport work: `prefix-and-payload-parsing` and
`logcat-and-payload-precision` taught the parser to read one line well, but never
taught it that two lines can belong together.

## What Changes

- `Entry` gains `continuation_of: Option<usize>` — the ordinal of the **root** of the
  chain this line belongs to, or `None` for a line that starts its own entry. One
  `Entry` per physical line is preserved; nothing is merged inside `looq-core`.
- Three continuation recognizers in the plain-text path, all lookbehind-only:
  - **R1 — prefix-less frame.** No recognised timestamp prefix, and the line matches an
    explicit marker (`at `, `Caused by:`, `... N more`, `Suppressed:`,
    `Traceback (most recent call last):`, `File "…", line N`).
  - **R2 — logcat identity.** A logcat record whose `(pid, tid, level, tag)` equal the
    chain root's, whose message additionally carries a continuation signal.
  - **R3 — open brace.** The chain root's message ends with unclosed `{` depth; the
    chain runs until depth returns to zero.
- Shared guards on every recognizer: the chain root must be an entry with a recognised
  prefix; a blank line breaks any open chain; a chain longer than a cap is closed and
  the truncation is reported as a diagnostic; only `Format::Plain` participates.
- `looq-wasm` DTO gains `continuationOf: number | null`.
- Timeline bucket counts exclude continuation entries, so a 60-frame trace reads as one
  event rather than a burst of sixty.
- Filtering and search treat a chain as indivisible: the predicate is evaluated against
  the chain root, and a match on any member surfaces the root.
- The entry table renders a chain as a collapsible group — the root row carries a line
  count and an expand control, members render indented beneath it.

No **BREAKING** changes: `continuation_of` is `None` for every line that parses the way
it does today, and the existing
`entries_emitted + blank_lines + diagnostics.total() == total_lines` invariant is
untouched because continuation lines still emit entries.

## Capabilities

### New Capabilities
- `entry-continuations`: what makes a line a continuation of the entry before it, the
  guards that stop a chain from running away, and the reporting obligation when a chain
  is truncated.

### Modified Capabilities
- `field-extraction`: `Entry` shape gains the continuation link.
- `wasm-bridge`: the entry DTO carries the continuation link across the JS boundary.
- `timeline`: continuation entries are excluded from bucket counts.
- `filtering`: a chain is filtered as a unit, not per line.
- `search`: a match inside a chain member surfaces the chain root.
- `entry-table`: a chain renders as a collapsible group.

## Impact

- `crates/looq-core/src/entry.rs` — new `Entry` field.
- `crates/looq-core/src/parser.rs` — chain state (`Option<ChainState>`), blank-line
  break, cap enforcement and the truncation diagnostic; `DiagnosticReason` gains a
  variant.
- `crates/looq-core/src/parsers/plain.rs` — the three recognizers; `dispatch_payload`
  is deliberately **not** touched.
- `crates/looq-core/src/timestamp.rs` — `LogcatRecord` identity comparison for R2.
- `crates/looq-wasm/src/dto.rs`, `web/src/wasm-types.ts` — DTO field, kept in sync.
- `web/src/entry-index.ts` — bucket counts and predicate evaluation over chains.
- `web/src/predicate.ts`, `web/src/components/looq-entry-table.ts` — chain-aware
  filtering and the collapsible group.
- `crates/looq/assets/` — rebuilt `core.wasm` (TDR §5 budget: 210,165 of ~300,000 bytes
  today) via `./scripts/build-frontend.sh`, committed in the same change.
- Fixtures: R1 has no instance in the measured bugreport corpus, so synthetic fixtures
  for it are mandatory rather than optional.
