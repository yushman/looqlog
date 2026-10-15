//! Engine benchmarks (`file-ingest` "Parse throughput is measured natively", `filtering`
//! "Filtering performance", ADR-0010): session ingest — parse, index and raw-line
//! retention together — per format, and a full re-filter at 10k and 1M entries.

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use looqlog_core::query::FieldFilters;
use looqlog_core::session::{filters_from, QueryInput, Session, SessionConfig, View};

const LEVELS: [&str; 4] = ["INFO", "DEBUG", "WARN", "ERROR"];
const SERVICES: [&str; 5] = ["api", "db", "worker", "auth", "cache"];
const MESSAGES: [&str; 7] = [
    "request completed",
    "cache miss",
    "connection retried",
    "job processed",
    "slow query detected",
    "config reloaded",
    "health check passed",
];

fn line(format: &str, i: usize) -> String {
    let level = LEVELS[i % LEVELS.len()];
    let service = SERVICES[i % SERVICES.len()];
    let message = MESSAGES[i % MESSAGES.len()];
    let ts = 1_754_668_800 + i as i64;
    match format {
        "json" => format!(
            "{{\"timestamp\":\"{ts}\",\"level\":\"{level}\",\"service\":\"{service}\",\"message\":\"{message}\",\"seq\":{i},\"request_id\":\"req-{i:06}\"}}\n"
        ),
        "logfmt" => format!(
            "ts={ts} level={level} service={service} msg=\"{message}\" seq={i} request_id=req-{i:06}\n"
        ),
        _ => format!(
            "{} {service} {level} {message} (seq {i})\n",
            chrono::DateTime::from_timestamp(ts, 0).unwrap().to_rfc3339()
        ),
    }
}

fn fixture_bytes(format: &str, target: usize) -> String {
    let mut out = String::with_capacity(target + 256);
    let mut i = 0;
    while out.len() < target {
        out.push_str(&line(format, i));
        i += 1;
    }
    out
}

fn fixture_lines(lines: usize) -> String {
    (0..lines).map(|i| line("json", i)).collect()
}

fn ingest(input: &str) -> Session {
    let mut s = Session::new(SessionConfig::default());
    for chunk in input.as_bytes().chunks(64 * 1024) {
        s.ingest(chunk);
    }
    s.finish();
    s
}

fn bench_ingest(c: &mut Criterion) {
    let mut group = c.benchmark_group("session_ingest_1mb");
    for format in ["json", "logfmt", "plain"] {
        let fixture = fixture_bytes(format, 1_000_000);
        group.throughput(Throughput::Bytes(fixture.len() as u64));
        group.bench_with_input(
            BenchmarkId::new(format, fixture.len()),
            &fixture,
            |b, input| {
                b.iter(|| std::hint::black_box(ingest(input).total()));
            },
        );
    }
    group.finish();
}

/// One filter toggle as the client sends it: rebuild the match set, then the first
/// screen of rows.
fn bench_refilter(c: &mut Criterion) {
    let mut group = c.benchmark_group("refilter");
    group.sample_size(10);
    for n in [10_000, 1_000_000] {
        let mut s = ingest(&fixture_lines(n));
        let mut v = View::new();
        let inputs = [
            QueryInput {
                filters: filters_from([("level", vec!["ERROR"])]),
                ..Default::default()
            },
            QueryInput {
                filters: FieldFilters::new(),
                search: "re:slow.*detected".to_string(),
                ..Default::default()
            },
        ];
        for (name, input) in ["level_error", "regex"].iter().zip(inputs) {
            group.bench_function(BenchmarkId::new(*name, n), |b| {
                b.iter(|| {
                    let reply = s.set_query(&mut v, input.clone());
                    let window = s.window(&mut v, 0, 100);
                    std::hint::black_box((reply.matched, window.rows.len()))
                });
            });
        }
    }
    group.finish();
}

criterion_group!(benches, bench_ingest, bench_refilter);
criterion_main!(benches);
