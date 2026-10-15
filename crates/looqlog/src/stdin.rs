//! Stdin reader (`stdin-stream` spec, ADR-0004, ADR-0010). Reads stdin on its own
//! thread from process start, whether or not any client is connected, and feeds each
//! chunk into the shared session, whose `--max-lines` limit evicts the oldest lines.
//! It never touches a client socket, so a slow or absent client cannot block it.

use std::io::Read;
use std::time::{Duration, Instant};

use crate::source::{lock, Shared, CHUNK_BYTES};

/// How long the opening lines are held for format detection once the first bytes
/// arrived, before detection runs on whatever came (`live-tail-ui` spec, "Stream format
/// detection holds the opening lines"). Never forced before any data: detection on an
/// empty sample would lock the stream into plain text.
pub const DETECTION_HOLD: Duration = Duration::from_millis(300);

pub fn spawn_stdin_reader(shared: Shared) {
    let reader = shared.clone();
    std::thread::spawn(move || {
        let mut stdin = std::io::stdin().lock();
        let mut buf = vec![0u8; CHUNK_BYTES];
        loop {
            let read = stdin.read(&mut buf);
            let mut src = lock(&reader);
            match read {
                Ok(0) => {
                    tracing::info!("stdin closed (EOF)");
                    src.session.finish();
                    src.ended = true;
                    return;
                }
                Ok(n) => {
                    src.first_data_at.get_or_insert_with(Instant::now);
                    src.session.ingest(&buf[..n]);
                }
                Err(err) if err.kind() == std::io::ErrorKind::Interrupted => {}
                Err(err) => {
                    tracing::warn!("stdin read error: {err}");
                    src.session.finish();
                    src.ended = true;
                    return;
                }
            }
        }
    });

    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_millis(100));
        loop {
            tick.tick().await;
            let mut src = lock(&shared);
            let held_long_enough = src
                .first_data_at
                .is_some_and(|t| t.elapsed() >= DETECTION_HOLD);
            if held_long_enough && src.session.is_detecting() {
                src.session.force_detection();
            }
        }
    });
}
