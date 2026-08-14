//! Standalone memory probe for task 6.1 (choose the diagnostic-retention and
//! per-field distinct-value caps from a measurement, not a guess). Not part of the
//! library's public surface — a throwaway tool run once, with its result recorded in
//! docs/devlog.md, to intentionally NOT wire a general-purpose memory-profiling
//! dependency into the crate that ships.
//!
//! Usage:
//!   cargo run -p looq-core --release --example mem_probe -- diagnostics 1000000
//!   cargo run -p looq-core --release --example mem_probe -- fields 1000000
//! Wrap with `/usr/bin/time -l` (macOS) to capture peak RSS.

use looq_core::diagnostics::{DiagnosticReason, Diagnostics};
use looq_core::fields::FieldInventory;

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
        other => panic!("unknown mode {other}, expected 'diagnostics' or 'fields'"),
    }
}
