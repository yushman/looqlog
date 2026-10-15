//! Standalone memory probe for task 6.1 (choose the diagnostic-retention and
//! per-field distinct-value caps from a measurement, not a guess). Not part of the
//! library's public surface — a throwaway tool run once, with its result recorded in
//! docs/devlog.md, to intentionally NOT wire a general-purpose memory-profiling
//! dependency into the crate that ships.
//!
//! Usage:
//!   cargo run -p looqlog-core --release --example mem_probe -- diagnostics 1000000
//!   cargo run -p looqlog-core --release --example mem_probe -- fields 1000000
//!   cargo run -p looqlog-core --release --example mem_probe -- session 64
//! Wrap with `/usr/bin/time -l` (macOS) to capture peak RSS. `session` reports the live
//! heap a `Session` retains per MiB of ingested input, per format (ADR-0010 size cap).

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

use looqlog_core::diagnostics::{DiagnosticReason, Diagnostics};
use looqlog_core::fields::FieldInventory;
use looqlog_core::session::{Session, SessionConfig};

struct Counting;

static LIVE: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        LIVE.fetch_add(layout.size(), Ordering::Relaxed);
        System.alloc(layout)
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        LIVE.fetch_sub(layout.size(), Ordering::Relaxed);
        System.dealloc(ptr, layout)
    }
}

#[global_allocator]
static ALLOC: Counting = Counting;

fn session_line(format: &str, i: usize) -> String {
    let levels = ["INFO", "DEBUG", "WARN", "ERROR"];
    let services = ["api", "db", "worker", "auth", "cache"];
    let (level, service, ts) = (levels[i % 4], services[i % 5], 1_754_668_800 + i as i64);
    match format {
        "json" => format!(
            "{{\"timestamp\":\"{ts}\",\"level\":\"{level}\",\"service\":\"{service}\",\"message\":\"request completed\",\"seq\":{i},\"request_id\":\"req-{i:06}\"}}\n"
        ),
        "logfmt" => format!(
            "ts={ts} level={level} service={service} msg=\"request completed\" seq={i} request_id=req-{i:06}\n"
        ),
        _ => format!(
            "{} {service} {level} request completed (seq {i})\n",
            chrono::DateTime::from_timestamp(ts, 0).unwrap().to_rfc3339()
        ),
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mode = args.get(1).map(String::as_str).unwrap_or("diagnostics");
    let n: usize = args
        .get(2)
        .and_then(|s| s.parse().ok())
        .unwrap_or(1_000_000);

    match mode {
        "diagnostics" => {
            // Uncapped (cap = n) so nothing is dropped: measures the true per-item
            // cost of retaining one Diagnostic each.
            let mut diagnostics = Diagnostics::new(n);
            for i in 0..n {
                diagnostics.record(
                    i,
                    DiagnosticReason::InvalidJson,
                    format!("line {i}: expected value at line 1 column 1"),
                );
            }
            println!(
                "diagnostics: retained={} total={}",
                diagnostics.retained().len(),
                diagnostics.total()
            );
        }
        "fields" => {
            let mut inventory = FieldInventory::new(n);
            for i in 0..n {
                inventory.record("request_id", &format!("req-{i:08}"));
            }
            let stats = inventory.get("request_id").unwrap();
            println!(
                "fields: distinct_values={} high_cardinality={}",
                stats.values.len(),
                stats.high_cardinality
            );
        }
        "session" => {
            let mib = n.min(4096);
            for format in ["json", "logfmt", "plain"] {
                let before = LIVE.load(Ordering::Relaxed);
                let mut session = Session::new(SessionConfig::default());
                let (mut bytes, mut i, mut chunk) = (0usize, 0usize, String::new());
                while bytes < mib << 20 {
                    chunk.push_str(&session_line(format, i));
                    i += 1;
                    if chunk.len() >= 64 * 1024 {
                        bytes += chunk.len();
                        session.ingest(chunk.as_bytes());
                        chunk.clear();
                    }
                }
                session.finish();
                let retained = LIVE.load(Ordering::Relaxed) - before;
                println!(
                    "session {format}: {mib} MiB in, {} entries, {} raw bytes, heap {} bytes = {:.2} bytes per input byte",
                    session.total(),
                    session.retained_raw_bytes(),
                    retained,
                    retained as f64 / bytes as f64
                );
            }
        }
        other => panic!("unknown mode {other}, expected 'diagnostics', 'fields' or 'session'"),
    }
}
