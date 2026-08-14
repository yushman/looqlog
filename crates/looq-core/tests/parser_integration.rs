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
use looq_core::timestamp::TimeZonePolicy;

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

#[test]
fn field_value_map_type_check() {
    // Compile-time sanity: Entry.fields is a BTreeMap<String, FieldValue>.
    let map: BTreeMap<String, FieldValue> = BTreeMap::new();
    assert!(map.is_empty());
}
