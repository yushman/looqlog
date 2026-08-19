//! Parse hot-path benchmark (log-parsing spec; design.md D9, tasks 1.3/6.2). Feeds a
//! deterministic ~1MB fixture per format through `Parser` end to end (feed + finish)
//! and reports throughput. Mirrors the shape of `scripts/gen-bench-fixture.py` (the
//! day-4 skeleton's JSON generator) so the JSON number is comparable across changes.

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use looq_core::format::Format;
use looq_core::parser::Parser;
use looq_core::timestamp::{ParseContext, TimeZonePolicy};

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

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// The pathological case for the sticky prefix choice (`prefix-and-payload-parsing`
/// design.md D3, task 9.1): every line uses a different timestamp shape at a different
/// head offset from the line before it, so the recorded hint misses on essentially
/// every line and the parser pays for the full sweep each time. Whatever this number
/// is, it is the floor the D3 optimisation can degrade to — TDR §11's <200 ms/MB has
/// to hold here, not only on the uniform fixture.
fn gen_mixed_prefix_fixture() -> String {
    let mut out = String::with_capacity(TARGET_BYTES + 4096);
    let mut i: u64 = 0;
    while out.len() < TARGET_BYTES {
        let level = LEVELS[(i as usize) % LEVELS.len()];
        let service = SERVICES[(i as usize) % SERVICES.len()];
        let message = MESSAGES[(i as usize) % MESSAGES.len()];
        let ts_seconds = 1_754_668_800 + i;
        let dt = chrono::DateTime::from_timestamp(ts_seconds as i64, 0).unwrap();
        let month = MONTHS[(dt.format("%m").to_string().parse::<usize>().unwrap()) - 1];
        let hms = dt.format("%H:%M:%S");
        let day = dt.format("%d");
        let line = match i % 7 {
            // ISO at offset 0.
            0 => format!("{} {service} {level} {message}\n", dt.to_rfc3339()),
            // syslog RFC 3164 at offset 0 — year-less, so it also exercises the
            // reference-instant path.
            1 => format!("{month} {day} {hms} host {service}[{i}]: {message}\n"),
            // Apache/CLF, deliberately *not* at offset 0.
            2 => format!(
                "10.0.{}.{} - - [{day}/{month}/{}:{hms} +0000] \"GET /{service} HTTP/1.1\" 500 12\n",
                i % 256,
                (i / 256) % 256,
                dt.format("%Y")
            ),
            // klog, behind its severity letter.
            3 => format!(
                "{}{}{day} {hms}.123456       1 {service}.go:{i}] {message}\n",
                &level[..1],
                dt.format("%m")
            ),
            // slash-date at offset 0.
            4 => format!("{}/{}/{day} {hms} {level} {message}\n", dt.format("%Y"), dt.format("%m")),
            // logcat (`logcat-and-payload-precision` task 7.1): the shape whose
            // severity letter and tag sit *behind* two or three columns, so the
            // whole record has to match before anything is consumed. Rotating the
            // column layout on `i % 4` against the `i % 7` shape cycle means every
            // observed layout is exercised, and — because the line before a logcat
            // line is never logcat — the sticky hint misses on every one of them,
            // which is the case this shape actually risks.
            5 => {
                let columns = match i % 4 {
                    0 => format!("{:>5} {:>5} {:>5}", 1000, 806, 995),
                    1 => format!("{:>5} {:>5} {:>5}", "root", 511, 614),
                    2 => format!("{:>5} {:>5}", 806, 29149),
                    _ => format!("{:>5} {:>5} {:>5}", "u0_a2", 1906, 1915),
                };
                format!(
                    "{}-{day} {hms}.198 {columns} {} {service}Service: {message}\n",
                    dt.format("%m"),
                    &level[..1]
                )
            }
            // bare epoch at offset 0.
            _ => format!("{ts_seconds} {service} {level} {message}\n"),
        };
        out.push_str(&line);
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

fn bench_plain_mixed_shapes(c: &mut Criterion) {
    let fixture = gen_mixed_prefix_fixture();
    // A reference instant is required for the year-less shapes (syslog 3164, klog) to
    // be recognised at all; `looq-core` never reads the clock itself (ADR-0005), so a
    // fixed one is supplied here — which also keeps the benchmark deterministic.
    let reference = chrono::DateTime::from_timestamp(1_755_000_000, 0).unwrap();
    let mut group = c.benchmark_group("parse_1mb");
    group.throughput(Throughput::Bytes(fixture.len() as u64));
    group.bench_with_input(
        BenchmarkId::new("plain_mixed_shapes", fixture.len()),
        &fixture,
        |b, input| {
            b.iter(|| {
                let ctx = ParseContext::utc().with_reference(reference);
                let mut parser = Parser::with_context(Some(Format::Plain), ctx);
                let entries = parser.feed(input.as_bytes());
                let tail = parser.finish();
                std::hint::black_box((entries.len(), tail.len()))
            });
        },
    );
    group.finish();
}

criterion_group!(
    benches,
    bench_json,
    bench_logfmt,
    bench_plain,
    bench_plain_mixed_shapes
);
criterion_main!(benches);
