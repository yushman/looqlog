//! Plain-text fallback parser (log-parsing spec, "Plain-text fallback"). Always
//! produces one entry per non-empty line and never reports a line as malformed.
//!
//! Since `prefix-and-payload-parsing` this is also where the prefix/payload split
//! lives: a recognised timestamp prefix (any of `TimestampShape`, anywhere in the
//! head window) is consumed together with the level token that follows it, and
//! whatever remains is handed to the JSON or logfmt parser when it looks structured
//! (design.md D6). Everything else about the format stays what it was — one entry per
//! line, no diagnostics, ever.

use std::collections::BTreeMap;

use super::{json, logfmt, Extracted};
use crate::entry::FieldValue;
use crate::level;
use crate::parser::MAX_CHAIN_LINES;
use crate::timestamp::{self, LeadingMatch, LogcatRecord, ParseContext};

/// Field the prefix timestamp is kept under when a parsed payload disagrees with it
/// (design.md D7). The disagreement stays inspectable and filterable instead of being
/// quietly resolved in the payload's favour.
const PREFIX_TIMESTAMP_FIELD: &str = "prefix_ts";

/// A logcat column as a field value: `Number` when it is one (`806`, `29149`),
/// `String` when it is a name (`root`, `u0_a2`), so the field inventory does not
/// present `root` as a number.
fn column_value(raw: &str) -> FieldValue {
    if !raw.is_empty() && raw.bytes().all(|b| b.is_ascii_digit()) {
        FieldValue::Number(raw.to_string())
    } else {
        FieldValue::String(raw.to_string())
    }
}

/// The fields a recognised logcat record contributes (field-extraction spec, "logcat
/// columns become fields"). Ordinary fields, filterable like any other — `tag` is the
/// valuable one, 334 distinct values across 32,712 lines in the measured bugreport:
/// low cardinality, high selectivity, exactly what a filter chip is for. `uid` only
/// exists in the three-column layout, so it is absent rather than empty in the other.
fn logcat_fields(record: &LogcatRecord<'_>) -> BTreeMap<String, FieldValue> {
    let mut fields = BTreeMap::new();
    fields.insert(
        "tag".to_string(),
        FieldValue::String(record.tag.to_string()),
    );
    fields.insert("pid".to_string(), column_value(record.pid));
    fields.insert("tid".to_string(), column_value(record.tid));
    if let Some(uid) = record.uid {
        fields.insert("uid".to_string(), column_value(uid));
    }
    fields
}

/// One entry, always. Extracts a leading timestamp, a level and a structured payload
/// when recognisable patterns are present; all three stay absent otherwise.
pub fn parse_line(line: &str, ctx: &ParseContext) -> Extracted {
    match timestamp::extract_leading(line, ctx) {
        Some(matched) => parse_prefixed(matched, ctx),
        None => parse_unprefixed(line),
    }
}

/// No recognised prefix: today's behavior, unchanged. The whole line is the message
/// and the level scan is the only thing that runs.
fn parse_unprefixed(line: &str) -> Extracted {
    Extracted {
        timestamp: None,
        timestamp_used_default_tz: false,
        timestamp_year_inferred: false,
        timestamp_diagnostic: None,
        level: level::scan_message(line),
        message: Some(line.to_string()),
        fields: Default::default(),
    }
}

fn parse_prefixed(matched: LeadingMatch<'_>, ctx: &ParseContext) -> Extracted {
    // Level by position (design.md D5): the klog/logcat severity letter the timestamp
    // scanner had to consume, else the token immediately after the prefix.
    let mut rest = matched.rest;
    let mut prefix_level = matched.level_letter.and_then(level::from_letter);
    if prefix_level.is_none() {
        if let Some((level, after_token)) = level::match_positional(rest) {
            prefix_level = Some(level);
            rest = after_token;
        }
    }

    // A logcat record's columns and tag are fields, not message text
    // (field-extraction spec, "logcat columns become fields"). Built here so both the
    // payload and the no-payload path get them: a logcat message that is itself
    // structured contributes its own fields *as well*.
    let logcat_fields = matched
        .logcat
        .map(|record| logcat_fields(&record))
        .unwrap_or_default();

    let Some(payload) = dispatch_payload(rest, ctx) else {
        let message = join_head(matched.head, rest);
        // Only when the positional token yielded nothing does the whole-message scan
        // run — that ordering is the behavior change D5 calls out.
        let level = prefix_level.or_else(|| level::scan_message(&message));
        return Extracted {
            timestamp: Some(matched.parsed.instant),
            timestamp_used_default_tz: matched.parsed.used_default_tz,
            timestamp_year_inferred: matched.parsed.year_inferred,
            timestamp_diagnostic: None,
            level,
            message: Some(message),
            fields: logcat_fields,
        };
    };

    // Payload wins conflicts (design.md D7): the prefix is stamped by a collector or
    // container runtime *around* the application's own line, so the payload is the
    // more authoritative record of what was said and when. Same rule for a payload
    // key that collides with a logcat column name.
    let mut fields = logcat_fields;
    fields.extend(payload.fields);
    let (timestamp, used_default_tz, year_inferred) = match payload.timestamp {
        Some(instant) => {
            if instant != matched.parsed.instant {
                fields.insert(
                    PREFIX_TIMESTAMP_FIELD.to_string(),
                    FieldValue::String(matched.parsed.instant.to_rfc3339()),
                );
            }
            (
                Some(instant),
                payload.timestamp_used_default_tz,
                payload.timestamp_year_inferred,
            )
        }
        None => (
            Some(matched.parsed.instant),
            matched.parsed.used_default_tz,
            matched.parsed.year_inferred,
        ),
    };

    Extracted {
        timestamp,
        timestamp_used_default_tz: used_default_tz,
        timestamp_year_inferred: year_inferred,
        // A payload's own timestamp complaint is not a plain-text file's problem: the
        // prefix already produced a usable entry, and the log-parsing spec forbids
        // plain text from emitting diagnostics at all (design.md D6).
        timestamp_diagnostic: None,
        level: payload.level.or(prefix_level),
        message: Some(payload.message.unwrap_or_default()),
        fields,
    }
}

/// Offer the text behind the prefix to a structured parser when it looks like one
/// (design.md D6). One level only: `json::parse_payload` has the Docker unwrap turned
/// off and neither structured parser re-enters this one, so a payload's own payload is
/// never parsed again.
///
/// A payload that fails to parse is not a malformed line — it simply stays message
/// text (log-parsing spec, "A payload that fails to parse is not malformed").
fn dispatch_payload(rest: &str, ctx: &ParseContext) -> Option<Extracted> {
    if rest.starts_with('{') {
        return json::parse_payload(rest, ctx).ok();
    }
    if logfmt::looks_like_payload(rest) {
        return Some(logfmt::parse_line(rest, ctx));
    }
    None
}

/// Text that preceded the timestamp inside the head window (`1.2.3.4 - -` of an
/// access-log line) is put back in front of the message rather than dropped. Only the
/// timestamp and its level token were *recognised*; silently discarding the client
/// address of every access-log line would be exactly the kind of quiet loss the rest
/// of this crate is written to avoid.
fn join_head(head: &str, rest: &str) -> String {
    if head.is_empty() {
        rest.to_string()
    } else if rest.is_empty() {
        head.to_string()
    } else {
        format!("{head} {rest}")
    }
}

// ---------------------------------------------------------------------------
// Continuation recognition (`multiline-entry-continuations` design D3).
//
// Lookbehind only, always (design D1): "is this line a continuation?" is answered
// from the current line plus remembered state about the line before it, never from a
// line that has not arrived. That is what keeps `Parser::feed` returning the same
// entries at the same moment it does today, with no flush obligation on the caller.
// ---------------------------------------------------------------------------

/// The chain root's logcat identity, owned. `LogcatRecord` borrows from the line it
/// was scanned out of, which is gone by the time the next line arrives (design D10).
#[derive(Debug, Clone, PartialEq, Eq)]
struct LogcatIdentity {
    pid: String,
    tid: String,
    tag: String,
    /// The severity letter, which *is* the level for a logcat record. Compared as the
    /// raw letter so `S` (silent, which maps to no level at all) still distinguishes
    /// two records rather than collapsing into "both have no level".
    letter: Option<u8>,
}

impl LogcatIdentity {
    fn of(record: &LogcatRecord<'_>, letter: Option<u8>) -> Self {
        Self {
            pid: record.pid.to_string(),
            tid: record.tid.to_string(),
            tag: record.tag.to_string(),
            letter,
        }
    }

    /// Design D4: the timestamp is deliberately *not* part of the comparison. 830 of
    /// the 831 measured frames share their root's millisecond and one does not —
    /// including the timestamp would break exactly the trace long enough to cross a
    /// millisecond boundary, which is the one a user most wants grouped.
    fn matches(&self, record: &LogcatRecord<'_>, letter: Option<u8>) -> bool {
        self.pid == record.pid
            && self.tid == record.tid
            && self.tag == record.tag
            && self.letter == letter
    }
}

/// The open chain: which entry roots it, what its root looked like, and how much of
/// it has been consumed. Cleared on a blank line, on a line no recognizer accepts,
/// when the brace depth returns to zero, and when the cap is exhausted.
#[derive(Debug, Clone)]
pub struct ChainState {
    /// Ordinal (= line number) of the root entry. Every member links here, not to its
    /// immediate predecessor (design D2).
    root_ordinal: usize,
    logcat: Option<LogcatIdentity>,
    /// Unclosed `{` depth carried by the chain so far (design D5).
    depth: usize,
    /// Continuation lines linked so far, against `MAX_CHAIN_LINES` (design D8).
    members: usize,
    /// Line number of the most recent member (the root, initially). A chain only
    /// continues onto the *immediately* following line: a gap means a blank line (or
    /// a line the parser skipped) came between, which closes the chain. This is what
    /// makes the blank-line break survive the sampling replay, where blank lines are
    /// counted at read time and never re-fed to the format parsers.
    last_line: usize,
    /// The previous member was a Python `File "…", line N` frame, whose indented
    /// source line follows it and carries no marker of its own.
    after_file_frame: bool,
}

/// What the recognizers decided about one line.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ChainDecision {
    /// Root ordinal this line continues, or `None` when it starts its own entry.
    pub continuation_of: Option<usize>,
    /// Root line number of a chain closed by the cap, for the `ChainTruncated`
    /// diagnostic. Truncating silently is the defect (design D8).
    pub truncated_root: Option<usize>,
}

/// As [`parse_line`], plus the continuation decision. Only the plain-text path calls
/// this — JSON Lines and logfmt records are self-delimiting, so chaining under them
/// would let a truncated record absorb the next one instead of being reported
/// (design D6).
pub fn parse_line_chained(
    line: &str,
    line_no: usize,
    ctx: &ParseContext,
    chain: &mut Option<ChainState>,
) -> (Extracted, ChainDecision) {
    let matched = timestamp::extract_leading(line, ctx);
    // Copied out before `matched` is consumed: `LogcatRecord` and `rest_raw` borrow
    // the line, not the match, so they outlive it.
    let prefix = matched
        .as_ref()
        .map(|m| (m.logcat, m.rest_raw, m.level_letter));
    let extracted = match matched {
        Some(m) => parse_prefixed(m, ctx),
        None => parse_unprefixed(line),
    };
    let message = extracted.message.as_deref().unwrap_or("");
    let decision = resolve_chain(line, line_no, prefix, message, chain);
    (extracted, decision)
}

type PrefixInfo<'a> = (Option<LogcatRecord<'a>>, &'a str, Option<u8>);

fn resolve_chain(
    line: &str,
    line_no: usize,
    prefix: Option<PrefixInfo<'_>>,
    message: &str,
    chain: &mut Option<ChainState>,
) -> ChainDecision {
    let mut decision = ChainDecision::default();
    let mut close = false;

    if let Some(state) = chain.as_mut() {
        let accepted = line_no == state.last_line + 1
            && match &prefix {
                // R1 — a prefix-less frame, or the body of the Python `File` frame
                // above it. R3 subsumes both while the root left a brace open.
                None => {
                    state.depth > 0
                        || is_frame_marker(line)
                        || (state.after_file_frame && starts_indented(line))
                }
                // R2 — a re-stamped logcat record with the root's identity and a
                // message that carries a continuation signal of its own. Identity
                // establishes candidacy; the message establishes continuation (D4).
                Some((Some(record), rest_raw, letter)) => {
                    state
                        .logcat
                        .as_ref()
                        .is_some_and(|id| id.matches(record, *letter))
                        && (logcat_message_indented(rest_raw)
                            || is_frame_marker(message)
                            || state.depth > 0)
                }
                // A line that starts a fresh prefixed entry of its own. It closes the
                // chain and becomes the next candidate root.
                Some((None, _, _)) => false,
            };

        if accepted {
            if state.members >= MAX_CHAIN_LINES {
                // Never silently (design D8): the chain closes, this line starts a
                // fresh unlinked entry, and the truncation is reported.
                decision.truncated_root = Some(state.root_ordinal);
            } else {
                let previous_depth = state.depth;
                state.members += 1;
                state.last_line = line_no;
                state.depth = next_depth(previous_depth, message);
                state.after_file_frame = is_python_file_frame(trim_ascii_start(message));
                decision.continuation_of = Some(state.root_ordinal);
                // A payload chain ends on the line that closes its outermost brace,
                // so the next line starts a new entry.
                close = previous_depth > 0 && state.depth == 0;
            }
        }
    }

    if close {
        *chain = None;
    }

    if decision.continuation_of.is_none() {
        // Shared guard (design D3): a chain is only ever opened beneath a line that
        // carried a recognised timestamp prefix. That single rule is what keeps the
        // 134,960 unprefixed dump lines of the measured bugreport from collapsing
        // into a handful of enormous entries.
        *chain = prefix.map(|(record, _, letter)| ChainState {
            root_ordinal: line_no,
            logcat: record.map(|r| LogcatIdentity::of(&r, letter)),
            depth: next_depth(0, message),
            members: 0,
            last_line: line_no,
            after_file_frame: is_python_file_frame(trim_ascii_start(message)),
        });
    }

    decision
}

/// Blank lines close any open chain (design D3). Called by the parser, which counts
/// blanks before any format parser sees them.
pub fn close_chain(chain: &mut Option<ChainState>) {
    *chain = None;
}

/// Only `{` and `}` are counted, never `[` and `]` (design D5): logcat carries raw
/// ANSI escapes (`ESC[7m` from `top` output piped through `vhdnativeservice`), and
/// counting square brackets reads every one of those as an opening bracket — 557
/// lines left at positive depth in the measured corpus instead of 237.
fn next_depth(depth: usize, text: &str) -> usize {
    let mut depth = depth;
    for b in text.bytes() {
        match b {
            b'{' => depth += 1,
            b'}' => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    depth
}

fn starts_indented(line: &str) -> bool {
    matches!(line.as_bytes().first(), Some(b' ' | b'\t'))
}

/// A logcat message indented beyond the single space that always separates `TAG:`
/// from the message — extra indentation is the signal, the separator is not.
fn logcat_message_indented(rest_raw: &str) -> bool {
    rest_raw
        .strip_prefix(' ')
        .is_some_and(|after| matches!(after.as_bytes().first(), Some(b' ' | b'\t')))
}

/// ASCII-only leading-whitespace trim, so the returned slice is always on a `str`
/// char boundary (the byte-scanner style the rest of this crate uses; `regex` was
/// rejected for the `core.wasm` budget, see the change's design).
fn trim_ascii_start(text: &str) -> &str {
    let bytes = text.as_bytes();
    let mut i = 0;
    while matches!(bytes.get(i), Some(b' ' | b'\t')) {
        i += 1;
    }
    &text[i..]
}

/// R1's explicit marker list. Positive evidence, never the mere absence of a prefix:
/// "no prefix" fires on 134,960 lines of the measured corpus and "no prefix and
/// indented" on 92,151, while these markers under a prefixed root fire on 0.
fn is_frame_marker(text: &str) -> bool {
    let t = trim_ascii_start(text);
    t.starts_with("at ")
        || t.starts_with("Caused by:")
        || t.starts_with("Suppressed:")
        || t.starts_with("Traceback (most recent call last):")
        || is_ellipsis_more(t)
        || is_python_file_frame(t)
        || is_exception_header(t)
}

/// `... 14 more`, the tail a nested Java cause prints instead of repeating frames.
fn is_ellipsis_more(t: &str) -> bool {
    let Some(rest) = t.strip_prefix("... ") else {
        return false;
    };
    let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
    digits > 0 && rest[digits..].trim() == "more"
}

/// `File "app.py", line 12, in handler`.
fn is_python_file_frame(t: &str) -> bool {
    let Some(rest) = t.strip_prefix("File \"") else {
        return false;
    };
    let Some(at) = rest.find("\", line ") else {
        return false;
    };
    rest[at + "\", line ".len()..]
        .bytes()
        .next()
        .is_some_and(|b| b.is_ascii_digit())
}

/// An exception header: a dotted or bare identifier whose last segment ends in
/// `Exception`, `Error` or `Throwable`, optionally followed by `: message`.
///
/// Not optional, and `tests/fixtures/stack-trace.log` is why: the header sits between
/// the timestamped root and the `at …` frames with no marker of its own. Because the
/// decision is lookbehind-only it cannot be linked retroactively once the frames prove
/// what it was, so it has to be recognised in its own right — otherwise it breaks the
/// chain and the frames below it are left with no prefixed root to attach to.
fn is_exception_header(t: &str) -> bool {
    let bytes = t.as_bytes();
    let mut i = 0;
    while bytes
        .get(i)
        .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_' || *b == b'$' || *b == b'.')
    {
        i += 1;
    }
    if i == 0 {
        return false;
    }
    // Every byte consumed above is ASCII, so both slices are on char boundaries.
    let (ident, rest) = (&t[..i], &t[i..]);
    if !(rest.is_empty() || rest.starts_with(':')) {
        return false;
    }
    let last = ident.rsplit('.').next().unwrap_or(ident);
    last.ends_with("Exception") || last.ends_with("Error") || last.ends_with("Throwable")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::level::Level;
    use chrono::{DateTime, Utc};

    fn ctx_at(reference: &str) -> ParseContext {
        ParseContext::utc().with_reference(
            DateTime::parse_from_rfc3339(reference)
                .unwrap()
                .with_timezone(&Utc),
        )
    }

    #[test]
    fn unstructured_line_still_becomes_an_entry() {
        let tz = ParseContext::utc();
        let extracted = parse_line("something happened that matches no pattern at all", &tz);
        assert!(extracted.timestamp.is_none());
        assert!(extracted.level.is_none());
        assert_eq!(
            extracted.message.as_deref(),
            Some("something happened that matches no pattern at all")
        );
    }

    #[test]
    fn leading_timestamp_and_positional_level_extracted() {
        let tz = ParseContext::utc();
        let extracted = parse_line("2026-08-08T17:42:01Z ERROR connection refused", &tz);
        assert!(extracted.timestamp.is_some());
        assert_eq!(extracted.level, Some(Level::Error));
        // The level token is part of the recognised prefix now, so it leaves the
        // message (log-parsing spec, "Container stdout is parsed…" asserts the same).
        assert_eq!(extracted.message.as_deref(), Some("connection refused"));
    }

    #[test]
    fn iso_recognition_is_unchanged() {
        let tz = ParseContext::utc();
        let extracted = parse_line("2026-08-08T17:42:01Z something happened", &tz);
        assert_eq!(extracted.message.as_deref(), Some("something happened"));
        assert!(extracted.level.is_none());
    }

    // --- level precedence (task 4.4) ----------------------------------------

    #[test]
    fn positional_level_wins_over_a_later_word() {
        let tz = ParseContext::utc();
        let extracted = parse_line(
            "2026-08-08T17:42:01Z INFO retrying after ERROR response",
            &tz,
        );
        assert_eq!(extracted.level, Some(Level::Info));
        assert_eq!(
            extracted.message.as_deref(),
            Some("retrying after ERROR response")
        );
    }

    #[test]
    fn whole_message_scan_still_applies_without_a_prefix() {
        let tz = ParseContext::utc();
        let extracted = parse_line("app: ERROR something broke", &tz);
        assert_eq!(extracted.level, Some(Level::Error));
        assert!(extracted.timestamp.is_none());
    }

    #[test]
    fn whole_message_scan_still_applies_behind_a_prefix() {
        let tz = ParseContext::utc();
        // `api` is not a level, so the token is left in place and the scan runs.
        let extracted = parse_line("2026-08-08T17:42:01Z api WARN slow query", &tz);
        assert_eq!(extracted.level, Some(Level::Warn));
        assert_eq!(extracted.message.as_deref(), Some("api WARN slow query"));
    }

    #[test]
    fn klog_severity_letter_becomes_the_level() {
        let ctx = ctx_at("2026-08-20T00:00:00Z");
        let extracted = parse_line("I0808 17:42:01.123456       1 main.go:10] starting", &ctx);
        assert_eq!(extracted.level, Some(Level::Info));
        assert!(extracted.timestamp_year_inferred);
        assert_eq!(extracted.message.as_deref(), Some("1 main.go:10] starting"));
    }

    #[test]
    fn syslog_priority_in_the_positional_slot() {
        let tz = ParseContext::utc();
        let extracted = parse_line("2026-08-08T17:42:01Z <130> disk full", &tz);
        assert_eq!(extracted.level, Some(Level::Fatal));
        assert_eq!(extracted.message.as_deref(), Some("disk full"));
    }

    // --- logcat (`logcat-and-payload-precision` tasks 3.1–3.4) ---------------

    #[test]
    fn logcat_severity_letter_becomes_the_level_and_the_columns_become_fields() {
        let ctx = ctx_at("2026-08-20T00:00:00Z");
        let extracted = parse_line(
            "04-18 19:21:16.151  1000   806   995 D ActivityManager: freezing 2521 com.x",
            &ctx,
        );
        assert_eq!(extracted.level, Some(Level::Debug));
        assert!(extracted.timestamp_year_inferred);
        // Columns and tag leave the message entirely (field-extraction spec).
        assert_eq!(extracted.message.as_deref(), Some("freezing 2521 com.x"));
        assert_eq!(
            extracted.fields.get("tag"),
            Some(&FieldValue::String("ActivityManager".to_string()))
        );
        assert_eq!(
            extracted.fields.get("uid"),
            Some(&FieldValue::Number("1000".to_string()))
        );
        assert_eq!(
            extracted.fields.get("pid"),
            Some(&FieldValue::Number("806".to_string()))
        );
        assert_eq!(
            extracted.fields.get("tid"),
            Some(&FieldValue::Number("995".to_string()))
        );
    }

    #[test]
    fn logcat_two_columns_produce_no_uid() {
        let ctx = ctx_at("2026-08-20T00:00:00Z");
        let extracted = parse_line(
            "04-21 13:07:51.985   806 29149 W UsbDescriptorParser: Unrecognized len: 58",
            &ctx,
        );
        assert_eq!(extracted.level, Some(Level::Warn));
        assert_eq!(
            extracted.fields.get("pid"),
            Some(&FieldValue::Number("806".to_string()))
        );
        assert_eq!(
            extracted.fields.get("tid"),
            Some(&FieldValue::Number("29149".to_string()))
        );
        assert!(!extracted.fields.contains_key("uid"));
    }

    #[test]
    fn logcat_named_uid_stays_a_string() {
        let ctx = ctx_at("2026-08-20T00:00:00Z");
        let extracted = parse_line(
            "04-21 12:57:16.290  root   511   614 D vhdnativeservice: cmd: dumpsys cpuinfo",
            &ctx,
        );
        assert_eq!(
            extracted.fields.get("uid"),
            Some(&FieldValue::String("root".to_string()))
        );
        // The tag's colon is the anchor; the one in the message is left alone.
        assert_eq!(extracted.message.as_deref(), Some("cmd: dumpsys cpuinfo"));
    }

    #[test]
    fn logcat_silent_severity_reports_no_level() {
        let ctx = ctx_at("2026-08-20T00:00:00Z");
        let extracted = parse_line(
            "04-21 13:07:53.198  1000   806   806 S SilentTag: nothing to report here",
            &ctx,
        );
        // The record is recognised — it has a timestamp and its columns — but `S`
        // means "emit nothing", not a severity (design D2).
        assert!(extracted.timestamp.is_some());
        assert_eq!(extracted.level, None);
        assert_eq!(
            extracted.fields.get("tag"),
            Some(&FieldValue::String("SilentTag".to_string()))
        );
    }

    #[test]
    fn logcat_message_that_is_itself_structured_contributes_both_sets_of_fields() {
        let ctx = ctx_at("2026-08-20T00:00:00Z");
        let extracted = parse_line(
            r#"04-21 13:07:55.010  1000  1024  1024 I OkHttp: level=warn service=api msg="slow response""#,
            &ctx,
        );
        // The payload's own level wins over the severity letter (design D7).
        assert_eq!(extracted.level, Some(Level::Warn));
        assert_eq!(extracted.message.as_deref(), Some("slow response"));
        assert_eq!(
            extracted.fields.get("service"),
            Some(&FieldValue::String("api".to_string()))
        );
        assert_eq!(
            extracted.fields.get("tag"),
            Some(&FieldValue::String("OkHttp".to_string()))
        );
        assert_eq!(
            extracted.fields.get("pid"),
            Some(&FieldValue::Number("1024".to_string()))
        );
    }

    #[test]
    fn a_tagless_logcat_like_line_contributes_nothing() {
        let ctx = ctx_at("2026-08-20T00:00:00Z");
        let line = "04-21 13:07:51.985   806 29149 W";
        let extracted = parse_line(line, &ctx);
        // Not consumed at all: no timestamp, no fields, the whole line is the message.
        assert!(extracted.timestamp.is_none());
        assert!(extracted.fields.is_empty());
        assert_eq!(extracted.message.as_deref(), Some(line));
    }

    // --- prefix shapes end to end (task 5.1) ---------------------------------

    #[test]
    fn syslog_line_gets_a_timestamp_and_keeps_the_remainder() {
        let ctx = ctx_at("2026-08-20T00:00:00Z");
        let extracted = parse_line("Aug  8 17:42:01 host app[123]: connection refused", &ctx);
        assert!(extracted.timestamp.is_some());
        assert!(extracted.timestamp_year_inferred);
        assert_eq!(
            extracted.message.as_deref(),
            Some("host app[123]: connection refused")
        );
    }

    #[test]
    fn access_log_head_is_kept_in_the_message() {
        let tz = ParseContext::utc();
        let extracted = parse_line(
            r#"1.2.3.4 - - [08/Aug/2026:17:42:01 +0000] "GET /x HTTP/1.1" 500 12"#,
            &tz,
        );
        assert_eq!(
            extracted.timestamp.unwrap().to_rfc3339(),
            "2026-08-08T17:42:01+00:00"
        );
        assert_eq!(
            extracted.message.as_deref(),
            Some(r#"1.2.3.4 - - "GET /x HTTP/1.1" 500 12"#)
        );
    }

    #[test]
    fn a_number_late_in_the_line_is_not_a_timestamp() {
        let tz = ParseContext::utc();
        let padding = "x".repeat(crate::timestamp::HEAD_WINDOW);
        let line = format!("{padding} 2026-08-08T17:42:01Z late");
        let extracted = parse_line(&line, &tz);
        assert!(extracted.timestamp.is_none());
        assert_eq!(extracted.message.as_deref(), Some(line.as_str()));
    }

    // --- payload dispatch (tasks 5.2, 5.3, 5.5) ------------------------------

    #[test]
    fn json_payload_behind_a_plain_prefix_contributes_fields() {
        let tz = ParseContext::utc();
        let extracted = parse_line(
            r#"2026-08-08 17:42:01 INFO {"status":500,"path":"/x"}"#,
            &tz,
        );
        assert_eq!(extracted.level, Some(Level::Info));
        assert!(extracted.fields.contains_key("status"));
        assert!(extracted.fields.contains_key("path"));
        assert_eq!(extracted.message.as_deref(), Some(""));
    }

    #[test]
    fn logfmt_payload_behind_a_syslog_prefix() {
        let ctx = ctx_at("2026-08-20T00:00:00Z");
        let extracted = parse_line(
            r#"Aug  8 17:42:01 host app: level=error msg="boom" service=api"#,
            &ctx,
        );
        assert_eq!(extracted.level, Some(Level::Error));
        assert_eq!(extracted.message.as_deref(), Some("boom"));
        match extracted.fields.get("service").unwrap() {
            FieldValue::String(s) => assert_eq!(s, "api"),
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn prose_with_one_pair_is_not_mistaken_for_logfmt() {
        let tz = ParseContext::utc();
        let extracted = parse_line(
            "2026-08-08T17:42:01Z retried because mode=safe was set",
            &tz,
        );
        assert!(extracted.fields.is_empty());
        assert_eq!(
            extracted.message.as_deref(),
            Some("retried because mode=safe was set")
        );
    }

    #[test]
    fn payload_that_fails_to_parse_stays_message_text() {
        let tz = ParseContext::utc();
        let extracted = parse_line(r#"2026-08-08 17:42:01 INFO {"broken":"#, &tz);
        assert!(extracted.timestamp.is_some());
        assert_eq!(extracted.level, Some(Level::Info));
        assert_eq!(extracted.message.as_deref(), Some(r#"{"broken":"#));
        assert!(extracted.timestamp_diagnostic.is_none());
        assert!(extracted.fields.is_empty());
    }

    #[test]
    fn payload_nesting_stops_at_one_level() {
        let tz = ParseContext::utc();
        // The payload's own message value looks like a structured line; it stays text.
        let extracted = parse_line(
            r#"2026-08-08T17:42:01Z {"msg":"level=error msg=\"inner\" service=api"}"#,
            &tz,
        );
        assert_eq!(
            extracted.message.as_deref(),
            Some(r#"level=error msg="inner" service=api"#)
        );
        assert!(extracted.fields.is_empty());
        assert_eq!(extracted.level, None);
    }

    /// The defect line verbatim from the measured bugreport
    /// (`logcat-and-payload-precision` design D4). `time` and `ret` are real fields;
    /// everything inside the two braced blocks is a Java map dump that used to arrive
    /// as top-level filter chips.
    #[test]
    fn a_dumped_java_map_behind_a_prefix_does_not_become_many_fields() {
        let tz = ParseContext::utc();
        let extracted = parse_line(
            r#"2026-04-16T21:34:04.009 - PROBE_HTTP http://x time=18ms ret=204 request={Connection=[close], User-Agent=[Mozilla/5.0 (X11; Linux x86_64)]} headers={null=[HTTP/1.1 204 No Content], Alt-Svc=[h3=":443"; ma=2592000], Content-Length=[0]}"#,
            &tz,
        );
        let names: Vec<&str> = extracted.fields.keys().map(String::as_str).collect();
        assert_eq!(names, vec!["headers", "request", "ret", "time"]);
        for gone in ["Alt-Svc", "Content-Length", "User-Agent", "Connection"] {
            assert!(
                !extracted.fields.contains_key(gone),
                "{gone} is still a field"
            );
        }
    }

    // --- conflicts (task 5.4) ------------------------------------------------

    #[test]
    fn payload_timestamp_and_level_win_and_the_prefix_timestamp_is_kept() {
        let tz = ParseContext::utc();
        let extracted = parse_line(
            r#"2026-08-08 17:42:01 INFO {"ts":"2026-08-08T17:41:59Z","level":"error","msg":"boom"}"#,
            &tz,
        );
        assert_eq!(
            extracted.timestamp.unwrap().to_rfc3339(),
            "2026-08-08T17:41:59+00:00"
        );
        assert_eq!(extracted.level, Some(Level::Error));
        assert_eq!(extracted.message.as_deref(), Some("boom"));
        match extracted.fields.get(PREFIX_TIMESTAMP_FIELD).unwrap() {
            FieldValue::String(s) => assert_eq!(s, "2026-08-08T17:42:01+00:00"),
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn the_prefix_fills_what_the_payload_omits() {
        let tz = ParseContext::utc();
        let extracted = parse_line(r#"2026-08-08 17:42:01 WARN {"msg":"boom","a":1}"#, &tz);
        assert_eq!(
            extracted.timestamp.unwrap().to_rfc3339(),
            "2026-08-08T17:42:01+00:00"
        );
        assert_eq!(extracted.level, Some(Level::Warn));
        assert_eq!(extracted.message.as_deref(), Some("boom"));
        // No disagreement to record.
        assert!(!extracted.fields.contains_key(PREFIX_TIMESTAMP_FIELD));
    }
}
