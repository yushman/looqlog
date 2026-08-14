//! Stdin reader task, the ring buffer it fills, and the live fan-out `/ws` handlers
//! subscribe to (`stdin-stream` spec, ADR-0004).
//!
//! Two bounded structures, deliberately different sizes and different jobs:
//!
//! - [`StdinBuffer`]: one shared ring buffer, sized by `--max-lines` (default
//!   100,000), filled from process start regardless of whether any client is
//!   connected. Its only job is snapshot-on-connect (D3) — a late-connecting or
//!   reloading client reads it once, then switches to live streaming.
//! - The `tokio::sync::broadcast` channel: the live fan-out. Its capacity is the
//!   per-client bounded outbound buffer (D2). This is not a resizing of the same
//!   buffer under two names — `broadcast`'s ring-buffer-with-lagged-receivers
//!   behaviour already *is* exactly "bounded per-client channel, drop-oldest under
//!   backpressure, tell the receiver how many it missed" (`RecvError::Lagged(n)`),
//!   so it is reused for that purpose rather than reimplemented.
//!
//! The stdin reader never touches a client socket directly (that is `server.rs`'s
//! job, one task per connection) and `broadcast::Sender::send` never blocks
//! regardless of how many receivers exist or how far behind they are — so the
//! producer is never blocked by a slow or absent client (`stdin-stream` spec,
//! "Slow client does not stall the producer").

use std::collections::VecDeque;
use std::sync::Mutex;

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::broadcast;

/// An event on the live fan-out. Every line carries the monotonic sequence number
/// assigned when it was read (D2) — a client can detect a gap by comparing
/// consecutive `seq` values even if the `Gap` message describing it is itself lost
/// to further backpressure downstream.
#[derive(Debug, Clone)]
pub enum StdinEvent {
    Line { seq: u64, text: String },
    Ended,
}

/// Capacity of the broadcast channel — the per-client live buffer, not the history
/// buffer. Deliberately much smaller than `--max-lines`: this bounds how far a slow
/// client can lag before it starts losing live messages (and getting `Gap` events),
/// independent of how much history the ring buffer retains for snapshots.
const CHANNEL_CAPACITY: usize = 1024;

/// One retained line: its sequence number and text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BufferedLine {
    pub seq: u64,
    pub text: String,
}

/// The shared ring buffer (ADR-0004). `max_lines` lines are retained; the oldest is
/// dropped once the buffer is full. Guarded by a plain (non-async) `Mutex` — every
/// critical section is a `VecDeque` push/pop, never an `.await`.
pub struct StdinBuffer {
    lines: VecDeque<BufferedLine>,
    max_lines: usize,
    next_seq: u64,
    /// Set once stdin has closed (EOF or read error). Needed because `Ended` is a
    /// one-shot broadcast (`stdin.rs` module doc) with no history: a client that
    /// connects *after* stdin has already closed would otherwise never learn the
    /// stream is over — its `rx.recv()` would just wait forever, since the reader
    /// task that would have sent `Ended` has already exited. `read_snapshot`
    /// reports this alongside the buffer contents so `server.rs` can send `Ended`
    /// immediately after the snapshot instead of only relying on the broadcast.
    ended: bool,
}

impl StdinBuffer {
    pub fn new(max_lines: usize) -> Self {
        Self {
            lines: VecDeque::new(),
            max_lines: max_lines.max(1),
            next_seq: 1,
            ended: false,
        }
    }

    /// Assigns the next sequence number to `text`, retains it (evicting the oldest
    /// line if the buffer is full), and returns the assigned sequence number.
    fn push(&mut self, text: String) -> u64 {
        let seq = self.next_seq;
        self.next_seq += 1;
        self.lines.push_back(BufferedLine { seq, text });
        while self.lines.len() > self.max_lines {
            self.lines.pop_front();
        }
        seq
    }

    fn mark_ended(&mut self) {
        self.ended = true;
    }

    /// A snapshot of the current buffer contents, the sequence number of its last
    /// line (0 if the buffer is empty and nothing has been read yet), and whether
    /// stdin has already closed. `last_seq` is `next_seq - 1`, not
    /// `lines.back().seq` — the latter would be wrong (too low) if the buffer is
    /// currently empty, which cannot happen once any line has been read, but is
    /// correct at t=0 either way since both are 0.
    fn snapshot(&self) -> (Vec<BufferedLine>, u64, bool) {
        (
            self.lines.iter().cloned().collect(),
            self.next_seq - 1,
            self.ended,
        )
    }
}

/// Spawn the stdin-reading task. Returns the broadcast sender (`/ws` handlers
/// subscribe to it for live lines) and the shared history buffer (`/ws` handlers
/// read it once, under lock, for the snapshot). The task starts running immediately,
/// independent of whether any client is or ever will be connected.
pub fn spawn_stdin_reader(
    max_lines: usize,
) -> (
    broadcast::Sender<StdinEvent>,
    std::sync::Arc<Mutex<StdinBuffer>>,
) {
    let (tx, _rx) = broadcast::channel(CHANNEL_CAPACITY);
    let buffer = std::sync::Arc::new(Mutex::new(StdinBuffer::new(max_lines)));
    let tx_task = tx.clone();
    let buffer_task = buffer.clone();
    tokio::spawn(async move {
        let mut lines = BufReader::new(tokio::io::stdin()).lines();
        loop {
            match lines.next_line().await {
                Ok(Some(line)) => {
                    // Push-before-broadcast, in this order, for every line: a
                    // subscriber that later reads the buffer under lock and compares
                    // against a broadcast subscription taken out beforehand can never
                    // observe a line in neither place, nor lose one between the two
                    // (server.rs's connect sequencing depends on this ordering).
                    let seq = buffer_task
                        .lock()
                        .expect("stdin buffer poisoned")
                        .push(line.clone());
                    // Err here just means zero receivers right now; not an error for
                    // the producer, which must never block on client presence.
                    let _ = tx_task.send(StdinEvent::Line { seq, text: line });
                }
                Ok(None) => {
                    tracing::info!("stdin closed (EOF)");
                    buffer_task
                        .lock()
                        .expect("stdin buffer poisoned")
                        .mark_ended();
                    let _ = tx_task.send(StdinEvent::Ended);
                    break;
                }
                Err(err) => {
                    tracing::warn!("stdin read error: {err}");
                    buffer_task
                        .lock()
                        .expect("stdin buffer poisoned")
                        .mark_ended();
                    let _ = tx_task.send(StdinEvent::Ended);
                    break;
                }
            }
        }
    });
    (tx, buffer)
}

/// Read the current snapshot (lines, last sequence number, whether stdin has
/// already closed) from a shared buffer handle. Exposed as a free function so
/// `server.rs` doesn't need to know about the `Mutex` locking convention directly.
pub fn read_snapshot(buffer: &Mutex<StdinBuffer>) -> (Vec<BufferedLine>, u64, bool) {
    buffer.lock().expect("stdin buffer poisoned").snapshot()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn send_never_blocks_with_zero_receivers() {
        let (tx, _rx) = broadcast::channel::<StdinEvent>(CHANNEL_CAPACITY);
        drop(_rx);
        let result = tokio::time::timeout(Duration::from_millis(200), async {
            for i in 0..10_000 {
                let _ = tx.send(StdinEvent::Line {
                    seq: i,
                    text: format!("line {i}"),
                });
            }
        })
        .await;
        assert!(result.is_ok(), "send() blocked with no receivers");
    }

    #[tokio::test]
    async fn send_never_blocks_with_a_lagging_receiver() {
        let (tx, mut rx) = broadcast::channel::<StdinEvent>(4);
        let result = tokio::time::timeout(Duration::from_millis(200), async {
            for i in 0..10_000 {
                let _ = tx.send(StdinEvent::Line {
                    seq: i,
                    text: format!("line {i}"),
                });
            }
        })
        .await;
        assert!(result.is_ok(), "send() blocked on a slow/lagging receiver");
        match rx.recv().await {
            Err(broadcast::error::RecvError::Lagged(_)) => {}
            other => panic!("expected Lagged, got {other:?}"),
        }
    }

    #[test]
    fn buffer_evicts_oldest_once_full() {
        let mut buf = StdinBuffer::new(3);
        buf.push("a".into());
        buf.push("b".into());
        buf.push("c".into());
        buf.push("d".into());
        let (lines, last_seq, ended) = buf.snapshot();
        assert_eq!(
            lines.iter().map(|l| l.text.as_str()).collect::<Vec<_>>(),
            vec!["b", "c", "d"]
        );
        assert_eq!(last_seq, 4);
        assert!(!ended);
    }

    #[test]
    fn sequence_numbers_are_monotonic_and_start_at_one() {
        let mut buf = StdinBuffer::new(100);
        assert_eq!(buf.push("a".into()), 1);
        assert_eq!(buf.push("b".into()), 2);
        assert_eq!(buf.push("c".into()), 3);
    }

    #[test]
    fn empty_buffer_snapshot_has_last_seq_zero() {
        let buf = StdinBuffer::new(10);
        let (lines, last_seq, ended) = buf.snapshot();
        assert!(lines.is_empty());
        assert_eq!(last_seq, 0);
        assert!(!ended);
    }

    /// The bug this guards against: `Ended` is a one-shot broadcast with no
    /// history (unlike lines, which live in the buffer). Without `mark_ended`,
    /// a client connecting after stdin has already closed would see the snapshot
    /// but then wait forever for an `Ended` that already happened and will never
    /// be sent again — caught via a real-server Playwright run during this
    /// change's own verification (see docs/devlog.md).
    #[test]
    fn snapshot_reports_ended_after_mark_ended() {
        let mut buf = StdinBuffer::new(10);
        buf.push("a".into());
        let (_, _, ended_before) = buf.snapshot();
        assert!(!ended_before);
        buf.mark_ended();
        let (lines, last_seq, ended_after) = buf.snapshot();
        assert!(ended_after);
        assert_eq!(lines.len(), 1);
        assert_eq!(last_seq, 1);
    }

    /// task 2.7: ten times `--max-lines` written, buffer holds at most `max_lines`.
    #[test]
    fn buffer_stays_bounded_over_ten_times_max_lines() {
        let max_lines = 1_000;
        let mut buf = StdinBuffer::new(max_lines);
        for i in 0..(max_lines * 10) {
            buf.push(format!("line {i}"));
        }
        let (lines, last_seq, _ended) = buf.snapshot();
        assert_eq!(lines.len(), max_lines);
        assert_eq!(last_seq, (max_lines * 10) as u64);
        // The retained lines are exactly the most recent `max_lines`.
        assert_eq!(
            lines.first().unwrap().text,
            format!("line {}", max_lines * 9)
        );
        assert_eq!(
            lines.last().unwrap().text,
            format!("line {}", max_lines * 10 - 1)
        );
    }
}
