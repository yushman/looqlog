## Why

`bootstrap-cli-and-wasm-skeleton` deliberately shipped `/ws` as a bare pipe: lines emitted
before a browser connects are lost, `--max-lines` governs nothing, and a slow client silently
loses data. ADR-0004 describes what it should be instead — a bounded ring buffer with
snapshot-on-connect and drop-oldest under backpressure — and PRD US-2's actual workflow is
`myapp | looq` followed a few seconds later by opening the browser, which the current pipe
cannot serve.

This change makes live tail real on both sides: the buffer and backpressure handling on the
backend, and the indicator, counter, autoscroll and gap marker in the UI. Days 11, 12 and 21 of
`docs/mvp-plan.md` are one change because a dropped line is only correct behaviour if the user
can see that it happened — the backend gap event and the UI gap marker are two halves of one
requirement, and building them ten days apart is how the marker ends up unimplemented while the
data path claims to be done.

## What Changes

- Bounded ring buffer on the backend sized by `--max-lines` (default 100,000), filled
  regardless of whether any client is connected (ADR-0004).
- Snapshot on connect: a new or reloading client receives the current buffer contents, then
  switches to live streaming.
- Bounded per-client channel with drop-oldest under backpressure, so the stdin reader is never
  blocked by a slow browser.
- Explicit gap events carrying how many lines were dropped, plus monotonic sequence numbers on
  every line so a client can detect a gap independently.
- A defined WebSocket message envelope — line, snapshot, gap, stream-ended — replacing the
  provisional raw-text framing and closing TDR §16's text-versus-binary question for MVP.
- Client-side eviction mirroring `--max-lines`, so a browser left open on a busy stream does not
  grow without bound.
- Live tail UI: a `LIVE` indicator, a lines-per-second counter, autoscroll with throttling, a
  pause that engages when the user scrolls away from the tail, and a visible gap marker in the
  entry list where lines were dropped.
- Connection state in the UI: connecting, live, stream ended, disconnected — with reconnect and
  backoff, and a snapshot-aware merge so a reconnect does not duplicate entries.
- Format detection for a stream, which cannot re-read its input: a short hold of the opening
  lines before detection, then parsing of the held lines and everything after.
- A mode indicator distinguishing stdin mode's privacy guarantee from file mode's, per TDR §12
  and the project rule that the two must never be described with the same words.

Not in this change: timeline and virtual table behaviour under a growing dataset (that is
`timeline-and-table`, which must handle eviction), filters and search over a live stream,
`--host` exposure beyond the warning that already exists, and any authentication on `/ws`
(`release-hardening`).

## Capabilities

### New Capabilities

- `live-tail-ui`: the browser-side surfaces of a live stream — indicator, counter, autoscroll
  and pause, gap markers, connection state, and the stdin-mode privacy indicator.

### Modified Capabilities

- `stdin-stream`: the buffer-less pipe is replaced by the ring buffer, snapshot-on-connect,
  bounded per-client channel with drop-oldest, gap events, sequence numbers and a structured
  message envelope. `--max-lines` now governs real behaviour.

### Ordering note

Assumes `bootstrap-cli-and-wasm-skeleton` (for `stdin-stream`) and `browser-app-shell` (for the
worker parser the stream feeds) are archived first.

## Impact

- `crates/looq`: ring buffer, broadcast with per-client bounded channels, message envelope,
  sequence numbering.
- `web/`: a live-tail component set inside the existing shell, plus stream-mode parser handling.
- The parser instance for a stream is long-lived, unlike the per-file instance from
  `browser-app-shell`, so its field inventory and diagnostics accumulate for the life of the
  stream and interact with client-side eviction.
- Closes TDR §16's WebSocket format question; the decision gets written back there.
- Constrains later changes: every entry-consuming component must tolerate entries being evicted
  from the front of the dataset, not only appended to the end.
