# 0004. Bounded ring buffer with snapshot-on-connect for stdin live tail

- **Status:** Accepted
- **Date:** 2026-08-08

## Context

US-2's real workflow is `myapp | looq` followed, a few seconds later, by the user
manually opening the browser tab — lines emitted in that gap must not be lost. At the
same time, the stdin reader must never block on a slow or disconnected client
(backpressure risk, TDR §7/§14), and a long-running tail (hours or days) must not grow
memory without bound — PRD §9 sets "memory at 10k lines < 100MB" as a success metric.

## Decision

The backend holds stdin lines in a bounded ring buffer sized by `--max-lines` (default
100,000), independent of whether any client is connected. Every new WebSocket
connection (first connect or page reload) receives a full snapshot of the current
buffer, then the connection switches to live streaming. When the buffer is full, the
oldest lines are dropped, kept in sync between the backend buffer and the client-side
table (TDR §7).

## Alternatives considered

### No backend buffer — pure pass-through

Simplest possible implementation: a stdin line is forwarded to a client only if one is
currently connected. Rejected: loses every line emitted before the first browser
connection, which breaks the exact workflow (pipe first, open browser second) the
feature exists for.

### Unbounded buffer, kept until process exit

Guarantees no data loss regardless of run length. Rejected: a tail left running for
hours or days would grow memory without bound, directly conflicting with the memory
success metric (PRD §9) and the general expectation that a piped CLI tool stays boring
and safe to leave running.

## Consequences

**Good:** late-connecting or reloading browsers still see recent history; the stdin
reader is never blocked by a slow client (bounded channel, drop-oldest under
backpressure); memory is bounded by `--max-lines` regardless of run duration.

**Bad / accepted cost:** under sustained backpressure, unsent messages can be dropped
with only a "gap" indicator shown in the UI — live tail is best-effort, not
guaranteed-delivery. `--max-lines` is one global cap shared by all connected clients,
not per-client.

**What would make us revisit:** if users routinely need guaranteed-no-loss tailing
beyond what the ring buffer window covers — would require an opt-in disk-backed buffer,
explicitly out of scope today.
