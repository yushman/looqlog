//! Integration tests against the fixtures in `tests/fixtures/`, covering the
//! Given/When/Then scenarios from the `log-parsing`, `format-detection` and
//! `field-extraction` specs (openspec/changes/log-parsing-core/specs/).

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use looq_core::diagnostics::DiagnosticReason;
use looq_core::entry::{Entry, FieldValue};
use looq_core::format::Format;
use looq_core::level::Level;
use looq_core::parser::Parser;
use looq_core::timestamp::{ParseContext, TimeZonePolicy};

fn fixture(name: &str) -> Vec<u8> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    fs::read(&path).unwrap_or_else(|e| panic!("reading fixture {name}: {e}"))
}

/// Parse `data` in one shot (single `feed` + `finish`).
fn parse_whole(data: &[u8], format: Option<Format>) -> Vec<Entry> {
    let mut parser = Parser::new(format, TimeZonePolicy::utc());
    let mut entries = parser.feed(data);
    entries.extend(parser.finish());
    entries
}

/// Feed `data` through a fresh parser in fixed-size byte chunks — small enough to
/// guarantee splits mid-line and mid-multi-byte-character — and collect all entries.
fn parse_in_chunks(data: &[u8], chunk_size: usize, format: Option<Format>) -> Vec<Entry> {
    let mut parser = Parser::new(format, TimeZonePolicy::utc());
    let mut entries = Vec::new();
    for chunk in data.chunks(chunk_size) {
        entries.extend(parser.feed(chunk));
    }
    entries.extend(parser.finish());
    entries
}

// ---------------------------------------------------------------------------
// 1.2 Chunk-split harness: whole vs adversarial splits must agree exactly.
// ---------------------------------------------------------------------------

#[test]
fn chunk_splits_match_whole_input_json() {
    let data = fixture("format-json.jsonl");
    let whole = parse_whole(&data, Some(Format::Json));
    for chunk_size in [1, 3, 7, 17, 64] {
        let chunked = parse_in_chunks(&data, chunk_size, Some(Format::Json));
        assert_eq!(chunked, whole, "chunk_size={chunk_size}");
    }
}

#[test]
fn chunk_splits_match_whole_input_logfmt() {
    let data = fixture("format-logfmt.log");
    let whole = parse_whole(&data, Some(Format::Logfmt));
    for chunk_size in [1, 5, 11, 31] {
        let chunked = parse_in_chunks(&data, chunk_size, Some(Format::Logfmt));
        assert_eq!(chunked, whole, "chunk_size={chunk_size}");
    }
}

#[test]
fn chunk_splits_match_whole_input_plain() {
    let data = fixture("format-plain.log");
    let whole = parse_whole(&data, Some(Format::Plain));
    for chunk_size in [1, 4, 9, 23] {
        let chunked = parse_in_chunks(&data, chunk_size, Some(Format::Plain));
        assert_eq!(chunked, whole, "chunk_size={chunk_size}");
    }
}

#[test]
fn chunk_split_mid_multibyte_character() {
    let data = fixture("multibyte-utf8.log");
    let whole = parse_whole(&data, Some(Format::Logfmt));
    // Byte-size-1 chunking guarantees every multi-byte UTF-8 sequence (Cyrillic,
    // emoji) gets split across `feed` calls.
    for chunk_size in [1, 2, 3] {
        let chunked = parse_in_chunks(&data, chunk_size, Some(Format::Logfmt));
        assert_eq!(chunked, whole, "chunk_size={chunk_size}");
        // No encoding-fallback diagnostic: this fixture is valid UTF-8 throughout.
    }
    let mut parser = Parser::new(Some(Format::Logfmt), TimeZonePolicy::utc());
    for chunk in data.chunks(1) {
        parser.feed(chunk);
    }
    parser.finish();
    assert_eq!(
        parser
            .diagnostics()
            .count_for(DiagnosticReason::EncodingFallback),
        0
    );
}

#[test]
fn trailing_line_without_newline_is_emitted() {
    let mut parser = Parser::new(Some(Format::Plain), TimeZonePolicy::utc());
    let mut entries = parser.feed(b"first line\nsecond line no newline");
    assert_eq!(entries.len(), 1);
    entries.extend(parser.finish());
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[1].message, "second line no newline");
}

#[test]
fn line_at_a_time_feeding_matches_whole_parse() {
    let data = fixture("format-logfmt.log");
    let whole = parse_whole(&data, Some(Format::Logfmt));

    let mut parser = Parser::new(Some(Format::Logfmt), TimeZonePolicy::utc());
    let mut streamed = Vec::new();
    for line in String::from_utf8(data).unwrap().lines() {
        let mut with_newline = line.to_string();
        with_newline.push('\n');
        let produced = parser.feed(with_newline.as_bytes());
        assert!(
            produced.len() <= 1,
            "one feed call yields at most one entry"
        );
        streamed.extend(produced);
    }
    streamed.extend(parser.finish());
    assert_eq!(streamed, whole);
}

// ---------------------------------------------------------------------------
// 3. Format parsers, driven through the fixtures.
// ---------------------------------------------------------------------------

#[test]
fn json_fixture_all_lines_become_entries() {
    let data = fixture("format-json.jsonl");
    let entries = parse_whole(&data, Some(Format::Json));
    assert_eq!(entries.len(), 10);
    assert_eq!(entries[3].level, Some(Level::Error));
    assert_eq!(entries[3].message, "connection refused");
}

#[test]
fn logfmt_fixture_all_lines_become_entries() {
    let data = fixture("format-logfmt.log");
    let entries = parse_whole(&data, Some(Format::Logfmt));
    assert_eq!(entries.len(), 10);
    assert_eq!(entries[2].level, Some(Level::Warn)); // "warning" alias
    assert_eq!(entries[2].message, "slow query detected");
}

#[test]
fn plain_fixture_never_malformed() {
    let data = fixture("format-plain.log");
    let mut parser = Parser::new(Some(Format::Plain), TimeZonePolicy::utc());
    let mut entries = parser.feed(&data);
    entries.extend(parser.finish());
    assert_eq!(entries.len(), 10);
    assert_eq!(parser.diagnostics().total(), 0);
}

#[test]
fn stack_trace_becomes_one_entry_per_line_no_diagnostics() {
    let data = fixture("stack-trace.log");
    let mut parser = Parser::new(Some(Format::Plain), TimeZonePolicy::utc());
    let mut entries = parser.feed(&data);
    entries.extend(parser.finish());
    assert_eq!(entries.len(), 5);
    assert_eq!(parser.diagnostics().total(), 0);
}

#[test]
fn malformed_fixture_skips_bad_lines_and_reports_them() {
    let data = fixture("malformed.jsonl");
    let mut parser = Parser::new(Some(Format::Json), TimeZonePolicy::utc());
    let mut entries = parser.feed(&data);
    entries.extend(parser.finish());
    // 6 lines total: 4 valid objects, "not json at all, just text" (invalid),
    // "[1,2,3]" (non-object).
    assert_eq!(entries.len(), 4);
    assert_eq!(
        parser
            .diagnostics()
            .count_for(DiagnosticReason::InvalidJson),
        1
    );
    assert_eq!(
        parser
            .diagnostics()
            .count_for(DiagnosticReason::NonObjectJson),
        1
    );
    // Every skipped line is accounted for (log-parsing spec).
    assert_eq!(
        entries.len() + parser.diagnostics().total() + parser.blank_lines(),
        parser.total_lines()
    );
}

#[test]
fn custom_fields_are_extracted_and_inventoried() {
    let data = fixture("custom-fields.log");
    let mut parser = Parser::new(Some(Format::Logfmt), TimeZonePolicy::utc());
    let mut entries = parser.feed(&data);
    entries.extend(parser.finish());
    assert_eq!(entries.len(), 5);
    match entries[0].fields.get("service").unwrap() {
        FieldValue::String(s) => assert_eq!(s, "api"),
        other => panic!("unexpected {other:?}"),
    }
    let inventory = parser.field_inventory();
    let region = inventory.get("region").unwrap();
    assert_eq!(region.count, 5);
    assert!(region.values.contains_key("us-east"));
    assert!(region.values.contains_key("us-west"));
    assert!(region.values.contains_key("eu-central"));
}

#[test]
fn latin1_fallback_line_readable_and_flagged() {
    let data = fixture("latin1.log");
    let mut parser = Parser::new(Some(Format::Logfmt), TimeZonePolicy::utc());
    let mut entries = parser.feed(&data);
    entries.extend(parser.finish());
    assert_eq!(entries.len(), 3);
    assert_eq!(entries[1].message, "Café latte served");
    assert_eq!(
        parser
            .diagnostics()
            .count_for(DiagnosticReason::EncodingFallback),
        1
    );
}

#[test]
fn out_of_order_and_missing_timestamps_preserved_in_input_order() {
    let data = fixture("out-of-order-timestamps.jsonl");
    let mut parser = Parser::new(Some(Format::Json), TimeZonePolicy::utc());
    let mut entries = parser.feed(&data);
    entries.extend(parser.finish());
    assert_eq!(entries.len(), 5);

    // Input order preserved despite the second line's timestamp preceding the first's.
    assert!(entries[0].timestamp.unwrap() > entries[1].timestamp.unwrap());
    assert_eq!(entries[0].message, "second event");
    assert_eq!(entries[1].message, "first event, arrived late");

    // Missing timestamp: entry kept, timestamp absent.
    assert_eq!(entries[2].message, "no timestamp at all");
    assert!(entries[2].timestamp.is_none());

    // Unparsable timestamp: entry kept, ts remains an ordinary field, diagnostic
    // recorded.
    assert!(entries[4].timestamp.is_none());
    match entries[4].fields.get("ts").unwrap() {
        FieldValue::String(s) => assert_eq!(s, "yesterday"),
        other => panic!("unexpected {other:?}"),
    }
    assert_eq!(
        parser
            .diagnostics()
            .count_for(DiagnosticReason::UnparsableTimestamp),
        1
    );

    // Ordinals point back at the original 1-based line numbers.
    for (i, entry) in entries.iter().enumerate() {
        assert_eq!(entry.ordinal, i + 1);
    }
}

// ---------------------------------------------------------------------------
// 4. Detection.
// ---------------------------------------------------------------------------

#[test]
fn three_fixtures_auto_detect_correctly() {
    let json = fixture("format-json.jsonl");
    let mut p = Parser::new(None, TimeZonePolicy::utc());
    p.feed(&json);
    p.finish();
    assert_eq!(p.detection().unwrap().format, Format::Json);

    let logfmt = fixture("format-logfmt.log");
    let mut p = Parser::new(None, TimeZonePolicy::utc());
    p.feed(&logfmt);
    p.finish();
    assert_eq!(p.detection().unwrap().format, Format::Logfmt);

    let plain = fixture("format-plain.log");
    let mut p = Parser::new(None, TimeZonePolicy::utc());
    p.feed(&plain);
    p.finish();
    assert_eq!(p.detection().unwrap().format, Format::Plain);
}

#[test]
fn single_json_line_among_plain_text_does_not_flip_to_json() {
    let mut data = String::new();
    data.push_str("{\"level\":\"info\",\"message\":\"looks like json\"}\n");
    for i in 0..19 {
        data.push_str(&format!("plain text log line number {i}\n"));
    }
    let mut parser = Parser::new(None, TimeZonePolicy::utc());
    let mut entries = parser.feed(data.as_bytes());
    entries.extend(parser.finish());
    assert_eq!(parser.detection().unwrap().format, Format::Plain);
    // Every line still becomes an entry: plain-text never rejects anything, so the
    // one JSON-shaped line doesn't disappear either — it becomes a plain entry.
    assert_eq!(entries.len(), 20);
}

#[test]
fn explicit_override_skips_detection_and_reports_mismatch() {
    let plain = fixture("format-plain.log");
    let mut parser = Parser::new(Some(Format::Json), TimeZonePolicy::utc());
    let mut entries = parser.feed(&plain);
    entries.extend(parser.finish());
    assert!(parser.detection().is_none());
    // Most (all, in this fixture) lines fail to parse as JSON: the mismatch is
    // visible in diagnostics rather than the file looking empty.
    assert!(entries.len() < 10);
    assert!(parser.diagnostics().total() > 0);
}

#[test]
fn forced_logfmt_on_json_looking_input_is_honoured() {
    // logfmt never fails a line, so forcing it on JSON input just extracts the top
    // bare/kv structure rather than raising diagnostics — proves the override wins
    // over what detection would have chosen (JSON).
    let json = fixture("format-json.jsonl");
    let mut parser = Parser::new(Some(Format::Logfmt), TimeZonePolicy::utc());
    let mut entries = parser.feed(&json);
    entries.extend(parser.finish());
    assert_eq!(entries.len(), 10);
    assert!(parser.detection().is_none());
}

// ---------------------------------------------------------------------------
// 6.5 Memory/cap tests.
// ---------------------------------------------------------------------------

#[test]
fn one_million_unparsable_lines_stays_within_diagnostic_cap() {
    let mut parser = Parser::new(Some(Format::Json), TimeZonePolicy::utc());
    let cap = parser.diagnostics().cap();
    let mut buf = String::with_capacity(64 * 1024);
    let total = 1_000_000usize;
    for i in 0..total {
        buf.push_str("not json line ");
        buf.push_str(&i.to_string());
        buf.push('\n');
        if buf.len() > 60_000 {
            parser.feed(buf.as_bytes());
            buf.clear();
        }
    }
    parser.feed(buf.as_bytes());
    parser.finish();

    assert_eq!(parser.diagnostics().total(), total);
    assert!(parser.diagnostics().retained().len() <= cap);
}

#[test]
fn fifty_thousand_distinct_request_ids_stays_within_inventory_cap() {
    let mut parser = Parser::new(Some(Format::Json), TimeZonePolicy::utc());
    let cap = parser.field_inventory().cap();
    let mut buf = String::with_capacity(64 * 1024);
    let total = 50_000usize;
    for i in 0..total {
        buf.push_str(&format!("{{\"request_id\":\"req-{i}\"}}\n"));
        if buf.len() > 60_000 {
            parser.feed(buf.as_bytes());
            buf.clear();
        }
    }
    parser.feed(buf.as_bytes());
    parser.finish();

    let stats = parser.field_inventory().get("request_id").unwrap();
    assert_eq!(stats.count, total);
    assert!(stats.high_cardinality);
    assert!(stats.values.len() <= cap);
}

// ---------------------------------------------------------------------------
// Design.md risk: fresh parser instances don't leak state across inputs.
// ---------------------------------------------------------------------------

#[test]
fn fresh_instances_detect_independently() {
    let json = fixture("format-json.jsonl");
    let logfmt = fixture("format-logfmt.log");

    let mut p1 = Parser::new(None, TimeZonePolicy::utc());
    p1.feed(&json);
    p1.finish();

    let mut p2 = Parser::new(None, TimeZonePolicy::utc());
    p2.feed(&logfmt);
    p2.finish();

    assert_eq!(p1.detection().unwrap().format, Format::Json);
    assert_eq!(p2.detection().unwrap().format, Format::Logfmt);
    assert!(p1.field_inventory().get("service").is_some());
    assert!(p2.field_inventory().get("service").is_some());
}

// ---------------------------------------------------------------------------
// prefix-and-payload-parsing task 7.4: the sticky shape/offset choice is an
// ordering optimisation and must never change which entries come out.
// ---------------------------------------------------------------------------

/// Lines deliberately alternating timestamp shapes and offsets, so a sticky choice
/// misses on most of them and gets re-selected part-way through.
const MIXED_SHAPES: &[&str] = &[
    "2026-08-08T17:42:01Z ERROR connection refused",
    "Aug  8 17:42:02 host app[123]: connection refused",
    r#"1.2.3.4 - - [08/Aug/2026:17:42:03 +0000] "GET /x HTTP/1.1" 500 12"#,
    "I0808 17:42:04.123456       1 main.go:10] starting",
    "2026/08/08 17:42:05 WARN slow query detected",
    "1754668806 worker started",
    "a line with no timestamp at all",
    "2026-08-08 17:42:07 INFO {\"status\":500,\"path\":\"/x\"}",
];

fn reference_instant() -> chrono::DateTime<chrono::Utc> {
    "2026-08-20T00:00:00Z".parse().unwrap()
}

fn plain_context() -> ParseContext {
    ParseContext::utc().with_reference(reference_instant())
}

#[test]
fn sticky_prefix_choice_does_not_change_entries() {
    // Long enough to cross the sticky miss limit several times over.
    let mut input = String::new();
    for _ in 0..30 {
        for line in MIXED_SHAPES {
            input.push_str(line);
            input.push('\n');
        }
    }

    // One parser for the whole input: the sticky hint is adopted and re-selected as
    // the shapes alternate.
    let mut parser = Parser::with_context(Some(Format::Plain), plain_context());
    let mut sticky = parser.feed(input.as_bytes());
    sticky.extend(parser.finish());

    // Reference run: a fresh context per line, so no hint is ever carried across
    // lines and every line pays for the full sweep.
    let mut swept: Vec<Entry> = Vec::new();
    for line in input.lines() {
        let mut per_line = Parser::with_context(Some(Format::Plain), plain_context());
        let mut entries = per_line.feed(line.as_bytes());
        entries.extend(per_line.finish());
        swept.extend(entries);
    }

    assert_eq!(sticky.len(), swept.len());
    for (i, (with_hint, without_hint)) in sticky.iter().zip(swept.iter()).enumerate() {
        // Ordinals differ by construction (the reference run restarts per line).
        assert_eq!(with_hint.timestamp, without_hint.timestamp, "line {i}");
        assert_eq!(
            with_hint.timestamp_used_default_tz, without_hint.timestamp_used_default_tz,
            "line {i}"
        );
        assert_eq!(
            with_hint.timestamp_year_inferred, without_hint.timestamp_year_inferred,
            "line {i}"
        );
        assert_eq!(with_hint.level, without_hint.level, "line {i}");
        assert_eq!(with_hint.message, without_hint.message, "line {i}");
        assert_eq!(with_hint.fields, without_hint.fields, "line {i}");
    }
}

#[test]
fn every_mixed_shape_line_but_the_bare_one_gets_a_timestamp() {
    // Guards the test above against passing vacuously: if the scanners stopped
    // recognising these shapes, both runs would agree on "no timestamp".
    let mut parser = Parser::with_context(Some(Format::Plain), plain_context());
    let mut entries = parser.feed(MIXED_SHAPES.join("\n").as_bytes());
    entries.extend(parser.finish());
    assert_eq!(entries.len(), MIXED_SHAPES.len());
    let dated = entries.iter().filter(|e| e.timestamp.is_some()).count();
    assert_eq!(dated, MIXED_SHAPES.len() - 1);
}

// ---------------------------------------------------------------------------
// prefix-and-payload-parsing task 9.3: one fixture file per newly recognised
// shape, parsed through auto-detection exactly as a browser would — so a
// regression that only shows up on a whole file (detection outcome, the sticky
// choice, per-line message splitting) is caught, not just the unit-level scanner.
// ---------------------------------------------------------------------------

/// Auto-detected parse with a fixed reference instant, so year inference is
/// deterministic. `looq-core` never reads the clock (ADR-0005).
fn parse_fixture_auto(name: &str) -> (Vec<Entry>, Parser) {
    let data = fixture(name);
    let mut parser = Parser::with_context(None, plain_context());
    let mut entries = parser.feed(&data);
    entries.extend(parser.finish());
    (entries, parser)
}

#[test]
fn syslog_3164_fixture_is_dated_flagged_and_detected_as_a_match() {
    let (entries, parser) = parse_fixture_auto("prefix-syslog3164.log");
    let detection = parser.detection().unwrap();
    assert_eq!(detection.format, Format::Plain);
    // Recognised prefixes are a match, not a fallback (format-detection spec).
    assert_eq!(detection.outcome, looq_core::DetectionOutcome::Threshold);
    assert_eq!(
        detection.timestamp_shape,
        Some(looq_core::TimestampShape::Syslog3164)
    );
    assert!(entries.iter().all(|e| e.timestamp.is_some()));
    // RFC 3164 carries no year, so every one of these is dated by assumption.
    assert!(entries.iter().all(|e| e.timestamp_year_inferred));
    // The text before/after the timestamp survives (log-parsing spec).
    assert!(entries[0].message.starts_with("host sshd[1234]:"));
    // December against an August reference steps back a year rather than dating
    // the entry in the future (design D4).
    assert_eq!(
        entries[4].timestamp.unwrap().to_rfc3339(),
        "2025-12-31T23:59:59+00:00"
    );
    assert_eq!(parser.diagnostics().total(), 0);
}

#[test]
fn klog_fixture_takes_its_level_from_the_severity_letter() {
    let (entries, parser) = parse_fixture_auto("prefix-klog.log");
    let detection = parser.detection().unwrap();
    assert_eq!(
        detection.timestamp_shape,
        Some(looq_core::TimestampShape::Klog)
    );
    assert_eq!(detection.outcome, looq_core::DetectionOutcome::Threshold);
    let levels: Vec<Option<Level>> = entries.iter().map(|e| e.level).collect();
    assert_eq!(
        levels,
        vec![
            Some(Level::Info),
            Some(Level::Warn),
            Some(Level::Error),
            Some(Level::Debug),
            Some(Level::Fatal),
        ]
    );
    assert!(entries.iter().all(|e| e.timestamp_year_inferred));
    assert_eq!(parser.diagnostics().total(), 0);
}

#[test]
fn clf_fixture_is_found_past_offset_zero_and_keeps_the_client_address() {
    let (entries, parser) = parse_fixture_auto("prefix-clf.log");
    let detection = parser.detection().unwrap();
    assert_eq!(
        detection.timestamp_shape,
        Some(looq_core::TimestampShape::Clf)
    );
    assert_eq!(detection.timestamp_offset, Some(13));
    assert!(entries.iter().all(|e| e.timestamp.is_some()));
    // The CLF timestamp carries its own offset, so nothing is assumed about the zone
    // and nothing is assumed about the year.
    assert!(entries
        .iter()
        .all(|e| !e.timestamp_used_default_tz && !e.timestamp_year_inferred));
    // Dropping everything before the timestamp would silently lose the client address
    // on every access-log line (log-parsing spec).
    assert!(entries[0].message.contains("1.2.3.4"));
    // The explicit -0700 is honoured, not ignored.
    assert_eq!(
        entries[2].timestamp.unwrap().to_rfc3339(),
        "2026-08-09T00:42:03+00:00"
    );
}

#[test]
fn slash_date_fixture_is_dated_without_inferring_a_year() {
    let (entries, parser) = parse_fixture_auto("prefix-slash-date.log");
    assert_eq!(
        parser.detection().unwrap().timestamp_shape,
        Some(looq_core::TimestampShape::SlashDate)
    );
    assert!(entries.iter().all(|e| e.timestamp.is_some()));
    // The shape carries its year, so nothing is inferred (field-extraction spec).
    assert!(entries.iter().all(|e| !e.timestamp_year_inferred));
    assert_eq!(entries[2].level, Some(Level::Error));
    assert_eq!(entries[2].message, "connection refused talking to cache");
}

#[test]
fn epoch_fixture_is_dated_from_a_bare_leading_integer() {
    let (entries, parser) = parse_fixture_auto("prefix-epoch.log");
    assert_eq!(
        parser.detection().unwrap().timestamp_shape,
        Some(looq_core::TimestampShape::Epoch)
    );
    assert_eq!(entries[0].timestamp.unwrap().timestamp(), 1_786_000_000);
    assert!(entries.iter().all(|e| !e.timestamp_year_inferred));
}

#[test]
fn payload_fixture_covers_dispatch_conflict_and_the_non_malformed_failure() {
    let (entries, parser) = parse_fixture_auto("prefix-payload.log");
    assert_eq!(entries.len(), 5);

    // JSON payload behind a plain prefix contributes filterable fields.
    assert_eq!(
        entries[0].fields.get("status"),
        Some(&FieldValue::Number("500".to_string()))
    );
    assert_eq!(
        entries[0].fields.get("path"),
        Some(&FieldValue::String("/x".to_string()))
    );

    // logfmt payload behind a syslog prefix.
    assert_eq!(entries[1].level, Some(Level::Error));
    assert_eq!(entries[1].message, "boom");
    assert_eq!(
        entries[1].fields.get("service"),
        Some(&FieldValue::String("api".to_string()))
    );

    // Conflict: the payload wins, and the prefix timestamp survives as a field so the
    // disagreement stays inspectable (design D7).
    assert_eq!(
        entries[2].timestamp.unwrap().to_rfc3339(),
        "2026-08-08T17:41:59+00:00"
    );
    assert_eq!(entries[2].level, Some(Level::Error));
    assert_eq!(entries[2].message, "conflicting");
    assert!(entries[2].fields.contains_key("prefix_ts"));

    // Truncated JSON payload: still an entry with the prefix's timestamp and level,
    // the raw text as its message, and NO diagnostic — a plain-text file must never
    // emit malformed-line diagnostics (log-parsing spec).
    assert_eq!(entries[3].level, Some(Level::Info));
    assert_eq!(entries[3].message, "{\"broken\":");

    // One `foo=bar` in prose is below the two-pair threshold (design D6).
    assert!(entries[4].fields.is_empty());
    assert!(entries[4].message.contains("foo=bar"));

    assert_eq!(parser.diagnostics().total(), 0);
    assert!(parser.field_inventory().get("status").is_some());
}

// ---------------------------------------------------------------------------
// logcat-and-payload-precision tasks 2.6 / 3.x / 6.3: the logcat shape through a
// whole file, and the bugreport case where detection's 100-line sample sees only
// the unstructured preamble and no sticky hint is ever available.
// ---------------------------------------------------------------------------

#[test]
fn logcat_fixture_covers_every_observed_column_layout() {
    let (entries, parser) = parse_fixture_auto("prefix-logcat.log");
    let detection = parser.detection().unwrap();
    assert_eq!(detection.format, Format::Plain);
    // Six of the seven lines are records; the seventh is the tagless negative, which
    // still clears the 80% threshold.
    assert_eq!(detection.outcome, looq_core::DetectionOutcome::Threshold);
    assert_eq!(
        detection.timestamp_shape,
        Some(looq_core::TimestampShape::Logcat)
    );
    assert_eq!(entries.len(), 7);

    // `uid pid tid`, numeric uid.
    assert_eq!(entries[0].level, Some(Level::Debug));
    assert_eq!(entries[0].message, "freezing 2521 com.x");
    assert_eq!(
        entries[0].fields.get("tag"),
        Some(&FieldValue::String("ActivityManager".to_string()))
    );
    assert_eq!(
        entries[0].fields.get("uid"),
        Some(&FieldValue::Number("1000".to_string()))
    );

    // `name pid tid` — and the message keeps its own colon.
    assert_eq!(
        entries[1].fields.get("uid"),
        Some(&FieldValue::String("root".to_string()))
    );
    assert_eq!(entries[1].message, "cmd: dumpsys cpuinfo");

    // `pid tid` — no uid at all.
    assert_eq!(entries[2].level, Some(Level::Warn));
    assert!(!entries[2].fields.contains_key("uid"));
    assert_eq!(
        entries[2].fields.get("pid"),
        Some(&FieldValue::Number("806".to_string()))
    );

    // `u0_aNN pid tid`.
    assert_eq!(
        entries[3].fields.get("uid"),
        Some(&FieldValue::String("u0_a2".to_string()))
    );
    assert_eq!(
        entries[3].fields.get("tag"),
        Some(&FieldValue::String("d.process.medi".to_string()))
    );

    // `S` is silent, not a severity — the record is still dated and still has fields.
    assert!(entries[4].timestamp.is_some());
    assert_eq!(entries[4].level, None);

    // A record whose message is itself logfmt contributes both sets of fields, and
    // the payload's level wins over the severity letter (design D7).
    assert_eq!(entries[5].level, Some(Level::Warn));
    assert_eq!(entries[5].message, "slow response");
    assert_eq!(
        entries[5].fields.get("service"),
        Some(&FieldValue::String("api".to_string()))
    );
    assert_eq!(
        entries[5].fields.get("tag"),
        Some(&FieldValue::String("OkHttp".to_string()))
    );

    // The safety property (design D1): columns with no `Tag:` are not consumed, so
    // the line has no timestamp, no fields and its whole text as the message.
    assert!(entries[6].timestamp.is_none());
    assert!(entries[6].fields.is_empty());
    assert_eq!(entries[6].message, "04-21 13:07:51.985   806 29149 W");

    // Every record carries an inferred year, and none of this is a malformed line.
    assert!(entries[..6].iter().all(|e| e.timestamp_year_inferred));
    assert_eq!(parser.diagnostics().total(), 0);

    // The tag is what a filter chip is for.
    let tags = parser.field_inventory().get("tag").unwrap();
    assert_eq!(tags.values.len(), 6);
}

/// A bugreport opens with ~1,370 lines of `dumpstate` preamble and section banners,
/// so detection's 100-line sample never sees a logcat record and no sticky hint is
/// recorded. That is correct and harmless (design D6): plain text is still chosen and
/// every line falls back to the full sweep, which still recognises every record.
#[test]
fn bugreport_shaped_input_parses_its_logcat_lines_with_no_sticky_hint() {
    let mut input = String::new();
    for i in 0..120 {
        input.push_str(&format!("------ DUMPSYS SECTION {i} ------\n"));
    }
    for i in 0..40 {
        input.push_str(&format!(
            "04-18 19:21:16.151  1000   806   {i} D ActivityManager: freezing {i} com.x\n"
        ));
    }

    let mut parser = Parser::with_context(None, plain_context());
    let mut entries = parser.feed(input.as_bytes());
    entries.extend(parser.finish());

    let detection = parser.detection().unwrap();
    assert_eq!(detection.format, Format::Plain);
    // Nothing in the sampled head carries a prefix, so this is a genuine fallback and
    // no shape is recorded.
    assert_eq!(detection.outcome, looq_core::DetectionOutcome::Fallback);
    assert_eq!(detection.timestamp_shape, None);

    assert_eq!(entries.len(), 160);
    let dated = entries.iter().filter(|e| e.timestamp.is_some()).count();
    assert_eq!(dated, 40);
    assert!(entries[120..]
        .iter()
        .all(|e| e.level == Some(Level::Debug) && e.fields.contains_key("tag")));
    assert_eq!(parser.diagnostics().total(), 0);
}

#[test]
fn docker_wrapper_fixture_unwraps_only_the_exact_member_set() {
    let (entries, parser) = parse_fixture_auto("docker-wrapper.jsonl");
    assert_eq!(parser.detection().unwrap().format, Format::Json);
    assert_eq!(entries.len(), 3);

    // Unwrapped: level/message/timestamp come from the inner line, `stream` is kept.
    assert_eq!(entries[0].level, Some(Level::Error));
    assert_eq!(entries[0].message, "boom");
    assert_eq!(
        entries[0].timestamp.unwrap().to_rfc3339(),
        "2026-08-08T17:42:01+00:00"
    );
    assert_eq!(
        entries[0].fields.get("stream"),
        Some(&FieldValue::String("stdout".to_string()))
    );

    // Inner line with no timestamp of its own falls back to the wrapper's `time`.
    assert_eq!(
        entries[1].timestamp.unwrap().to_rfc3339(),
        "2026-08-08T17:42:03+00:00"
    );
    assert_eq!(entries[1].message, "no timestamp in here at all");

    // An ordinary object that merely *contains* `log` is left alone (design D8).
    assert_eq!(
        entries[2].fields.get("log"),
        Some(&FieldValue::String("hi".to_string()))
    );
    assert_eq!(entries[2].level, Some(Level::Info));
}

#[test]
fn field_value_map_type_check() {
    // Compile-time sanity: Entry.fields is a BTreeMap<String, FieldValue>.
    let map: BTreeMap<String, FieldValue> = BTreeMap::new();
    assert!(map.is_empty());
}
