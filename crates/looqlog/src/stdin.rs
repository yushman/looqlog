//! Stdin reader (`stdin-stream` spec, ADR-0004, ADR-0010). Reads stdin on its own
//! thread from process start, whether or not any client is connected, and feeds each
//! chunk into its source's session, whose `--max-lines` limit evicts the oldest lines.
//! It never touches a client socket, so a slow or absent client cannot block it.

use std::io::Read;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::source::{lock, Kind, Registry, Shared, Source, SourceId, CHUNK_BYTES};

/// How long the opening lines are held for format detection once the first bytes
/// arrived, before detection runs on whatever came (`live-tail-ui` spec, "Stream format
/// detection holds the opening lines"). Never forced before any data: detection on an
/// empty sample would lock the stream into plain text.
pub const DETECTION_HOLD: Duration = Duration::from_millis(300);

/// Adds the stdin source and starts reading it. Its retained bytes count toward the
/// budget other ingests see, but never trip it (ADR-0011): `--max-lines` bounds it.
pub fn spawn_stdin_reader(registry: &Arc<Mutex<Registry>>, max_lines: usize) -> SourceId {
    let (id, shared) = registry
        .lock()
        .expect("registry lock poisoned")
        .add(Source::new(Kind::Stdin, "stdin".into(), Some(max_lines)));
    let reader = shared.clone();
    let registry = registry.clone();
    let reader_id = id.clone();
    std::thread::spawn(move || {
        let mut stdin = std::io::stdin().lock();
        let mut buf = vec![0u8; CHUNK_BYTES];
        loop {
            let read = stdin.read(&mut buf);
            let (retained, done) = {
                let mut src = lock(&reader);
                let done = match read {
                    Ok(0) => {
                        tracing::info!("stdin closed (EOF)");
                        true
                    }
                    Ok(n) => {
                        src.first_data_at.get_or_insert_with(Instant::now);
                        src.session.ingest(&buf[..n]);
                        false
                    }
                    Err(err) if err.kind() == std::io::ErrorKind::Interrupted => false,
                    Err(err) => {
                        tracing::warn!("stdin read error: {err}");
                        true
                    }
                };
                if done {
                    src.session.finish();
                    src.ended = true;
                }
                (src.session.retained_bytes(), done)
            };
            registry
                .lock()
                .expect("registry lock poisoned")
                .record(&reader_id, retained);
            if done {
                return;
            }
        }
    });
    spawn_detection_hold(shared);
    id
}

fn spawn_detection_hold(shared: Shared) {
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
