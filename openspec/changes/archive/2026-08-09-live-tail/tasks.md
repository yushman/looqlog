## 1. Wire protocol

- [x] 1.1 JSON envelope with `line`, `snapshot`, `gap`, `ended` types; raw unlabelled text no longer sent
- [x] 1.2 Monotonic sequence number on every line, carried in every message that delivers lines
- [x] 1.3 Test that a log line whose text resembles an envelope is delivered as content, not interpreted
- [x] 1.4 Write the text-versus-binary decision and its revisit trigger back into TDR §16

## 2. Ring buffer and backpressure

- [x] 2.1 Ring buffer sized by `--max-lines`, filled from process start regardless of connected clients
- [x] 2.2 Snapshot on connect as a single message carrying buffer contents and the last sequence number
- [x] 2.3 Bounded per-client outbound channel with drop-oldest; stdin reader never blocked
- [x] 2.4 Gap event per drop, carrying the exact number of lines lost
- [x] 2.5 Synthetic fast-producer/slow-consumer harness proving the reader keeps pace and gap events fire with correct counts
- [x] 2.6 Test that no line is delivered twice when lines arrive during snapshot delivery
- [x] 2.7 Memory test: ten times `--max-lines` written, process memory stable
- [x] 2.8 Measure snapshot size and delivery time at the default `--max-lines`; chunk the snapshot if it stalls the connection

## 3. Stream parsing in the browser

- [x] 3.1 Long-lived parser instance per stream, fed line by line through the existing worker bridge
- [x] 3.2 Hold opening lines until a detection sample or a short timeout, then detect once and parse the held lines
- [x] 3.3 Slow-producer test: two lines then silence still get displayed after the timeout
- [x] 3.4 Detected format displayed and overridable exactly as in file mode
- [x] 3.5 Field inventory reported as cumulative, with wording that does not imply it describes retained entries

## 4. Live tail UI

- [x] 4.1 Connection state machine: connecting, live, ended, disconnected, each visually distinct
- [x] 4.2 Lines-per-second counter on a fixed throttled interval, decoupled from entry rendering
- [x] 4.3 Batched entry rendering at a fixed interval under fast input
- [x] 4.4 Autoscroll following the tail, pausing on scroll-away with a pending count, and an explicit resume
- [x] 4.5 Gap marker in the entry list from gap events and from sequence discontinuity, naming the number of lost lines
- [x] 4.6 Client-side eviction at `--max-lines` with a visible indication that earlier entries are no longer retained
- [x] 4.7 Reconnect with capped exponential backoff; snapshot merged by sequence number without duplicates
- [x] 4.8 Mode indicator wording: stdin mode states the local process boundary, file mode states the file never left the browser (TDR §12)

## 5. End-to-end

- [x] 5.1 PRD Flow 2: `myapp | looq --open` shows the live indicator, the counter and streaming entries
- [x] 5.2 Pipe first, open the browser after several seconds, verify the pre-connection lines appear
- [x] 5.3 Reload mid-stream, verify history and continuity
- [x] 5.4 Kill the backend, verify the disconnected state appears instead of a frozen live view
- [x] 5.5 Measure end-to-end line latency against the <100ms target in TDR §11 and record it with the method

## 6. Wrap-up

- [x] 6.1 Resolve the design open questions: retention limit ownership, snapshot compression, file-open during an active stream
- [x] 6.2 Both READMEs describe live tail and its distinct privacy wording
- [x] 6.3 Devlog entries with the measured latency, snapshot size and memory numbers
- [x] 6.4 `openspec validate live-tail --strict` passes
- [x] 6.5 Archive the change
