## Context

ADR-0004 already decided the shape of stdin buffering: bounded ring buffer, snapshot on connect,
drop-oldest under backpressure, gap indicator in the UI. What it did not decide is the wire
format, how a client knows a gap happened, what the browser does when its own memory fills, and
how a stream — which cannot be re-read — gets its format detected.

Those are the questions this change answers, and they are why the backend days (11, 12) and the
UI day (21) belong together. A gap event with no marker on screen is a feature that reports
data loss to nobody.

## Goals / Non-Goals

**Goals:**
- PRD US-2's real workflow: pipe first, open the browser seconds later, lose nothing.
- A stdin reader that never blocks, whatever the browser does.
- Data loss that is always visible, from either a gap event or a sequence discontinuity.
- Bounded memory on both sides of the socket.
- An honest privacy indicator for the mode whose guarantee is weaker.

**Non-Goals:**
- Guaranteed delivery. ADR-0004 accepts best-effort; a disk-backed buffer is out of scope.
- Authentication on `/ws` — `release-hardening`.
- Filters, search and timeline behaviour over a live stream beyond not breaking.
- Multi-producer or multi-file streams.

## Decisions

### D1 — JSON envelope over text frames, closing TDR §16

Messages are JSON objects with a type tag: `line`, `snapshot`, `gap`, `ended`. Binary framing
would be smaller and faster to parse, and is the obvious P2 optimisation, but it is premature
here: the volume that would justify it is also the volume where the ring buffer is dropping
lines anyway, and a text protocol is inspectable with `wscat`, which is how every test in this
change is written. Recorded as a decision with a measured trigger for revisiting: if envelope
parsing shows up in a profile at target throughput, switch.

Raw line text is never sent unlabelled, which also removes the class of bug where a log line
that happens to look like a control message is interpreted as one.

### D2 — Sequence numbers as the gap ground truth

Every line carries a monotonic sequence number. Gap events say how many lines were dropped, but
the client also checks continuity itself. Belt and braces, because the gap event travels through
the same channel that is dropping messages: relying on it alone means the one message that must
never be lost is delivered by the mechanism that is currently losing messages.

### D3 — Snapshot as one message, not a replay

A connecting client gets a single snapshot message carrying the buffer contents and the sequence
number of its last line, then live messages resume from there. The alternative — replaying the
buffer as individual line messages — is simpler on the backend and worse everywhere else: the
client cannot tell history from live, the UI would animate a hundred thousand arrivals, and
reconnect deduplication would have nothing to anchor on.

Cost: a snapshot at `--max-lines` = 100,000 is a large single message. Its size is measured, and
chunking the snapshot is the fallback if it stalls the connection.

### D4 — The client evicts too, mirroring `--max-lines`

The backend's bound protects the CLI process; nothing yet protects the browser tab, which holds
parsed entries that are several times larger than the raw lines (TDR §14). So the client evicts
the oldest entries at the same limit and says that it has done so. Making eviction visible
matters more than it sounds: an entry count that silently means "the most recent 100,000" is the
kind of number people quote in incident reviews.

### D5 — Stream detection holds the opening lines, briefly

A file can be sampled and re-read; a stream cannot. So the client holds opening lines until it
has a detection sample or a short timeout expires, detects once, then parses the held lines and
everything after. The timeout exists because a service that emits two lines and goes quiet must
still display them.

Accepted limitation, already flagged in `log-parsing-core`: a process whose output changes shape
mid-stream — a plain-text banner followed by JSON — is classified by its opening. The override
is the escape hatch; automatic re-detection is not in MVP.

### D6 — Autoscroll pauses on scroll-away, with a pending count

Following the tail is right until the user is reading something, at which point it is actively
hostile. Scroll away pauses, an explicit control resumes and reports how many entries arrived
while paused. Rendering is batched on a fixed interval regardless, because a fast producer
otherwise turns every frame into a layout pass.

### D7 — One long-lived parser instance per stream

`browser-app-shell` creates a parser per file. A stream gets one instance for its lifetime, so
its field inventory and diagnostics accumulate. Two consequences worth stating: the inventory's
counts describe everything ever seen rather than what is currently retained after eviction, and
diagnostic caps are reached by long-running streams in a way they never are for a single file.
The first is a known inconsistency left open in `log-parsing-core`; this change is where it
becomes observable, and the decision is to report inventory counts as cumulative rather than to
try to decrement them on eviction.

### D8 — The privacy indicator is UI, not documentation

CLAUDE.md and TDR §12 both require that stdin mode's guarantee never be worded like file mode's.
A README paragraph does not reach a user looking at a screen full of production logs, so the
mode indicator lives in the top bar and states which of the two guarantees currently applies.

## Risks / Trade-offs

- **Gap markers untested because they need backpressure to appear** → A synthetic
  fast-producer/slow-consumer harness is a task in this change, not a manual experiment.
- **Snapshot at 100,000 lines stalls the connection** → Measure snapshot size and delivery time
  at the default `--max-lines`; chunk if needed.
- **Client eviction fights the timeline and table** → Those components arrive in
  `timeline-and-table`, which must handle front-eviction; stated in this change's impact so it
  is not discovered there.
- **Reconnect storms against a dead backend** → Exponential backoff with a cap, and a
  disconnected state that stops pretending.
- **Cumulative field inventory misleads after eviction** → Reported as cumulative in the UI
  wording, and revisited if users read it as current.
- **Lines-per-second counter itself causes re-render churn** → Throttled to a fixed interval,
  decoupled from the entry rendering batch.

## Open Questions

- Should `--max-lines` be sent to the client, or should the client choose its own retention?
  Sharing one number keeps the two sides consistent; letting the browser pick lets a weak
  machine protect itself.
- Does the snapshot need compression at the default limit, or is the message small enough that
  the complexity is not worth it?
- What should happen when a stream is active and the user also opens a file — replace the view,
  refuse, or show both? Not decided anywhere yet, and the answer shapes the shell's state model.
