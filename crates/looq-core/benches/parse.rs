//! Parse hot-path benchmark (log-parsing spec; design.md D9, tasks 1.3/6.2). Feeds a
//! deterministic ~1MB fixture per format through `Parser` end to end (feed + finish)
//! and reports throughput. Mirrors the shape of `scripts/gen-bench-fixture.py` (the
//! day-4 skeleton's JSON generator) so the JSON number is comparable across changes.

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use looq_core::format::Format;
use looq_core::parser::Parser;
use looq_core::timestamp::TimeZonePolicy;

const TARGET_BYTES: usize = 1_000_000;

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

fn gen_json_fixture() -> String {
    let mut out = String::with_capacity(TARGET_BYTES + 4096);
    let mut i: u64 = 0;
    while out.len() < TARGET_BYTES {
        let level = LEVELS[(i as usize) % LEVELS.len()];
        let service = SERVICES[(i as usize) % SERVICES.len()];
        let message = MESSAGES[(i as usize) % MESSAGES.len()];
        let ts_seconds = 1_754_668_800 + i;
        out.push_str(&format!(
            "{{\"timestamp\":\"{ts_seconds}\",\"level\":\"{level}\",\"service\":\"{service}\",\"message\":\"{message}\",\"seq\":{i},\"request_id\":\"req-{i:06}\"}}\n"
        ));
        i += 1;
    }
    out
}

fn gen_logfmt_fixture() -> String {
    let mut out = String::with_capacity(TARGET_BYTES + 4096);
    let mut i: u64 = 0;
    while out.len() < TARGET_BYTES {
        let level = LEVELS[(i as usize) % LEVELS.len()];
        let service = SERVICES[(i as usize) % SERVICES.len()];
        let message = MESSAGES[(i as usize) % MESSAGES.len()];
        let ts_seconds = 1_754_668_800 + i;
        out.push_str(&format!(
            "ts={ts_seconds} level={level} service={service} msg=\"{message}\" seq={i} request_id=req-{i:06}\n"
        ));
        i += 1;
    }
    out
}

fn gen_plain_fixture() -> String {
    let mut out = String::with_capacity(TARGET_BYTES + 4096);
    let mut i: u64 = 0;
    while out.len() < TARGET_BYTES {
        let level = LEVELS[(i as usize) % LEVELS.len()];
        let service = SERVICES[(i as usize) % SERVICES.len()];
        let message = MESSAGES[(i as usize) % MESSAGES.len()];
        let ts_seconds = 1_754_668_800 + i;
        // Render as an RFC 3339 instant so the leading-timestamp path is exercised.
        let dt = chrono::DateTime::from_timestamp(ts_seconds as i64, 0).unwrap();
        out.push_str(&format!(
            "{} {service} {level} {message} (seq {i})\n",
            dt.to_rfc3339()
        ));
        i += 1;
    }
    out
}

fn bench_format(c: &mut Criterion, name: &str, format: Format, fixture: &str) {
    let mut group = c.benchmark_group("parse_1mb");
    group.throughput(Throughput::Bytes(fixture.len() as u64));
    group.bench_with_input(
        BenchmarkId::new(name, fixture.len()),
        fixture,
        |b, input| {
            b.iter(|| {
                let mut parser = Parser::new(Some(format), TimeZonePolicy::utc());
                let entries = parser.feed(input.as_bytes());
                let tail = parser.finish();
                std::hint::black_box((entries.len(), tail.len()))
            });
        },
    );
    group.finish();
}

fn bench_json(c: &mut Criterion) {
    let fixture = gen_json_fixture();
    bench_format(c, "json", Format::Json, &fixture);
}

fn bench_logfmt(c: &mut Criterion) {
    let fixture = gen_logfmt_fixture();
    bench_format(c, "logfmt", Format::Logfmt, &fixture);
}

fn bench_plain(c: &mut Criterion) {
    let fixture = gen_plain_fixture();
    bench_format(c, "plain", Format::Plain, &fixture);
}

criterion_group!(benches, bench_json, bench_logfmt, bench_plain);
criterion_main!(benches);
