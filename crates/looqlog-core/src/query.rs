//! Search compilation, field filters and the chain-aware predicate (`search`/`filtering`
//! specs, ADR-0010). Ported from the former `web/src/search-query.ts` and
//! `web/src/predicate.ts`.
//!
//! The combination rule lives here once: OR within a field, AND across fields, AND the
//! search query. The time range is deliberately not part of the predicate — the index
//! answers ranges by binary search, and folding the range in would re-test every entry
//! per range change.

use std::collections::{BTreeMap, BTreeSet};

use regex::Regex;

use crate::entry::{Entry, FieldValue};
use crate::fields::FieldInventory;
use crate::index::EntryIndex;

const REGEX_PREFIX: &str = "re:";
/// Documented escape: a leading backslash before `re:` searches for the literal text.
const LITERAL_ESCAPE_PREFIX: &str = "\\re:";

/// Field name → selected values. An empty value set imposes no constraint.
pub type FieldFilters = BTreeMap<String, BTreeSet<String>>;

#[derive(Debug, Clone)]
pub enum CompiledQuery {
    None,
    /// Lowercased needle; matched case-insensitively.
    Substring(String),
    Regex(Regex),
}

impl CompiledQuery {
    pub fn is_none(&self) -> bool {
        matches!(self, CompiledQuery::None)
    }
}

/// Compiles the search box's raw text. An invalid regex is an error, never a query
/// that silently matches nothing; keeping the previous query is the caller's job.
pub fn compile_query(raw: &str) -> Result<CompiledQuery, String> {
    if raw.is_empty() {
        return Ok(CompiledQuery::None);
    }
    if let Some(literal) = raw
        .strip_prefix('\\')
        .filter(|_| raw.starts_with(LITERAL_ESCAPE_PREFIX))
    {
        return Ok(CompiledQuery::Substring(literal.to_lowercase()));
    }
    if let Some(pattern) = raw.strip_prefix(REGEX_PREFIX) {
        // No implicit flags: applied as written.
        return Regex::new(pattern)
            .map(CompiledQuery::Regex)
            .map_err(|err| format!("invalid regex: {}", regex_error_summary(&err)));
    }
    Ok(CompiledQuery::Substring(raw.to_lowercase()))
}

/// The `regex` crate renders syntax errors as several lines with a caret under the
/// pattern; the last `error:` line is the readable part (e.g. "look-around, including
/// look-ahead and look-behind, is not supported").
fn regex_error_summary(err: &regex::Error) -> String {
    let text = err.to_string();
    text.lines()
        .rev()
        .find_map(|line| line.trim().strip_prefix("error: "))
        .unwrap_or(text.trim())
        .to_string()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtractedToken {
    pub field: String,
    pub value: String,
}

/// Splits `text` on whitespace and turns each whitespace-terminated `field=value`
/// token into a filter when `field` is known. The token still being typed (the tail,
/// when `text` does not end in whitespace) is left alone. An unknown field stays text.
pub fn extract_field_tokens(text: &str, known: &BTreeSet<String>) -> (Vec<ExtractedToken>, String) {
    let trailing_space = text.ends_with(char::is_whitespace);
    let words: Vec<&str> = text.split_whitespace().collect();
    let (complete, in_progress) = if trailing_space || words.is_empty() {
        (&words[..], &[][..])
    } else {
        words.split_at(words.len() - 1)
    };
    let mut tokens = Vec::new();
    let mut remaining: Vec<&str> = Vec::new();
    for word in complete {
        match split_field_token(word) {
            Some((field, value)) if known.contains(field) => tokens.push(ExtractedToken {
                field: field.to_string(),
                value: value.to_string(),
            }),
            _ => remaining.push(word),
        }
    }
    remaining.extend_from_slice(in_progress);
    (tokens, remaining.join(" "))
}

/// `^([A-Za-z_][\w.-]*)=(.+)$`
fn split_field_token(word: &str) -> Option<(&str, &str)> {
    let (field, value) = word.split_once('=')?;
    let mut chars = field.chars();
    let first = chars.next()?;
    if !(first.is_ascii_alphabetic() || first == '_') {
        return None;
    }
    if !chars.all(|c| c.is_alphanumeric() || c == '_' || c == '.' || c == '-') {
        return None;
    }
    (!value.is_empty()).then_some((field, value))
}

/// Field names chips and `field=value` may name: `level` always, plus the inventory.
pub fn known_field_names(inventory: &FieldInventory) -> BTreeSet<String> {
    let mut names: BTreeSet<String> = inventory.fields().keys().cloned().collect();
    names.insert("level".to_string());
    names
}

/// The canonical display string a value was counted under in the field inventory
/// (`FieldValue::display`), without allocating.
pub fn display_value(value: &FieldValue) -> &str {
    match value {
        FieldValue::String(s) | FieldValue::Number(s) | FieldValue::Json(s) => s,
        FieldValue::Bool(true) => "true",
        FieldValue::Bool(false) => "false",
        FieldValue::Null => "null",
    }
}

/// `level` is an `Entry` property rather than a field, but chips treat it as one.
pub fn field_display_value<'a>(entry: &'a Entry, field: &str) -> Option<&'a str> {
    if field == "level" {
        return entry.level.map(|l| l.as_str());
    }
    entry.fields.get(field).map(display_value)
}

/// OR within a field, AND across fields.
pub fn matches_field_filters(entry: &Entry, filters: &FieldFilters) -> bool {
    filters.iter().all(|(field, values)| {
        values.is_empty() || field_display_value(entry, field).is_some_and(|v| values.contains(v))
    })
}

/// Message text plus every field value (including `level`), never field names.
pub fn matches_query(entry: &Entry, query: &CompiledQuery) -> bool {
    if query.is_none() {
        return true;
    }
    matches_text(&entry.message, query)
        || entry.level.is_some_and(|l| matches_text(l.as_str(), query))
        || entry
            .fields
            .values()
            .any(|v| matches_text(display_value(v), query))
}

fn matches_text(text: &str, query: &CompiledQuery) -> bool {
    match query {
        CompiledQuery::None => true,
        CompiledQuery::Substring(needle) => find_substring(text, needle, 0).is_some(),
        CompiledQuery::Regex(re) => re.is_match(text),
    }
}

/// Case-insensitive search for an already-lowercased `needle` from byte `from`;
/// returns the byte offset in `text`. ASCII needles compare bytes without allocating.
fn find_substring(text: &str, needle: &str, from: usize) -> Option<usize> {
    if needle.is_empty() {
        return None;
    }
    if needle.is_ascii() {
        let hay = text.as_bytes();
        let n = needle.as_bytes();
        if hay.len() < n.len() {
            return None;
        }
        return (from..=hay.len() - n.len()).find(|&i| hay[i..i + n.len()].eq_ignore_ascii_case(n));
    }
    let lower = text.to_lowercase();
    lower.get(from..)?.find(needle).map(|i| i + from)
}

/// Character ranges `query` matches in `text`, in UTF-16 code units (what the
/// browser indexes strings by), for highlighting.
pub fn find_match_ranges(text: &str, query: &CompiledQuery) -> Vec<(usize, usize)> {
    let byte_ranges: Vec<(usize, usize)> = match query {
        CompiledQuery::None => return Vec::new(),
        CompiledQuery::Substring(needle) => {
            if needle.is_ascii() {
                let mut out = Vec::new();
                let mut from = 0;
                while let Some(at) = find_substring(text, needle, from) {
                    out.push((at, at + needle.len()));
                    from = at + needle.len();
                }
                out
            } else {
                // Offsets of the lowercased text; lowercasing can change lengths, so
                // these are approximate for such text — same as the browser was.
                let lower = text.to_lowercase();
                let ranges: Vec<(usize, usize)> = lower
                    .match_indices(needle.as_str())
                    .map(|(at, m)| (at, at + m.len()))
                    .collect();
                return ranges
                    .into_iter()
                    .map(|(s, e)| (utf16_offset(&lower, s), utf16_offset(&lower, e)))
                    .collect();
            }
        }
        CompiledQuery::Regex(re) => re
            .find_iter(text)
            .filter(|m| !m.is_empty())
            .map(|m| (m.start(), m.end()))
            .collect(),
    };
    byte_ranges
        .into_iter()
        .map(|(s, e)| (utf16_offset(text, s), utf16_offset(text, e)))
        .collect()
}

fn utf16_offset(text: &str, byte: usize) -> usize {
    let byte = byte.min(text.len());
    if text.is_ascii() {
        return byte;
    }
    text.get(..byte)
        .map_or(byte, |prefix| prefix.chars().map(char::len_utf16).sum())
}

/// The predicate a chain is judged by, in two halves: the field filters decide on
/// the chain **root** only, so `level=ERROR` shows a whole trace whose frames carry
/// no level; the search query may match **any** member, which surfaces the whole
/// chain with its root. An entry outside a chain — including an orphaned member — is
/// tested against both.
#[derive(Debug, Clone)]
pub struct Predicate {
    pub filters: FieldFilters,
    pub query: CompiledQuery,
}

impl Predicate {
    pub fn is_trivial(&self) -> bool {
        self.query.is_none() && self.filters.values().all(BTreeSet::is_empty)
    }

    pub fn root(&self, entry: &Entry) -> bool {
        matches_field_filters(entry, &self.filters)
    }

    pub fn member(&self, entry: &Entry) -> bool {
        matches_query(entry, &self.query)
    }
}

/// Which retained entries pass a [`Predicate`], as sorted ordinals. Rebuilt in full
/// only when the predicate changes; entries appended later are tested once each by
/// [`MatchSet::catch_up`], and evicted ones are dropped there too.
#[derive(Debug, Clone)]
pub struct MatchSet {
    /// `None` means "everything matches" — the common no-filter case pays nothing.
    predicate: Option<Predicate>,
    matching: Vec<usize>,
    /// Highest ordinal already evaluated.
    evaluated_upto: Option<usize>,
}

impl MatchSet {
    pub fn new(predicate: Option<Predicate>, index: &EntryIndex) -> Self {
        let predicate = predicate.filter(|p| !p.is_trivial());
        let mut set = Self {
            predicate,
            matching: Vec::new(),
            evaluated_upto: None,
        };
        set.rebuild(index);
        set
    }

    pub fn predicate(&self) -> Option<&Predicate> {
        self.predicate.as_ref()
    }

    pub fn is_filtering(&self) -> bool {
        self.predicate.is_some()
    }

    fn rebuild(&mut self, index: &EntryIndex) {
        self.matching.clear();
        self.evaluated_upto = index.iter().last().map(|e| e.ordinal);
        let Some(pred) = &self.predicate else {
            return;
        };
        for entry in index.iter() {
            if index.root_of(entry).is_some() {
                continue; // decided as part of its chain
            }
            let members = index.members_of(entry.ordinal);
            if members.is_empty() {
                if pred.root(entry) && pred.member(entry) {
                    self.matching.push(entry.ordinal);
                }
            } else if chain_matches(pred, index, entry.ordinal) {
                self.matching.push(entry.ordinal);
                self.matching.extend_from_slice(members);
            }
        }
        self.matching.sort_unstable();
        self.matching.dedup();
    }

    /// Drops evicted ordinals and tests entries appended since the last call.
    pub fn catch_up(&mut self, index: &EntryIndex) {
        if let Some(first) = index.iter().next().map(|e| e.ordinal) {
            let cut = self.matching.partition_point(|&o| o < first);
            self.matching.drain(..cut);
        } else {
            self.matching.clear();
        }
        let from = match self.evaluated_upto {
            None => 0,
            Some(last) => match index.position(last) {
                Some(p) => p + 1,
                None => index
                    .iter()
                    .position(|e| e.ordinal > last)
                    .unwrap_or(index.len()),
            },
        };
        if from >= index.len() {
            return;
        }
        self.evaluated_upto = index.iter().last().map(|e| e.ordinal);
        let Some(pred) = self.predicate.clone() else {
            return;
        };
        for entry in index.iter_from(from) {
            match index.root_of(entry) {
                None => {
                    if pred.root(entry) && pred.member(entry) {
                        self.insert(entry.ordinal);
                    }
                }
                Some(root) => {
                    if self.contains_ordinal(root) {
                        self.insert(entry.ordinal);
                    } else if chain_matches(&pred, index, root) {
                        self.insert(root);
                        for &m in index.members_of(root) {
                            self.insert(m);
                        }
                    }
                }
            }
        }
    }

    fn insert(&mut self, ordinal: usize) {
        if self.matching.last().map_or(true, |&last| ordinal > last) {
            self.matching.push(ordinal);
        } else if let Err(at) = self.matching.binary_search(&ordinal) {
            self.matching.insert(at, ordinal);
        }
    }

    fn contains_ordinal(&self, ordinal: usize) -> bool {
        self.matching.binary_search(&ordinal).is_ok()
    }

    /// Whether a retained `ordinal` passes. Always true when not filtering.
    pub fn contains(&self, ordinal: usize) -> bool {
        self.predicate.is_none() || self.contains_ordinal(ordinal)
    }

    /// Matching ordinals in input order, when filtering; `None` means "all".
    pub fn ordinals(&self) -> Option<&[usize]> {
        self.predicate.as_ref().map(|_| self.matching.as_slice())
    }

    pub fn len(&self, index: &EntryIndex) -> usize {
        match self.predicate {
            None => index.len(),
            Some(_) => self.matching.len(),
        }
    }
}

/// A chain is shown iff its root passes the field filters and the query matches the
/// root or any member.
fn chain_matches(pred: &Predicate, index: &EntryIndex, root: usize) -> bool {
    let Some(root_entry) = index.get(root) else {
        return false;
    };
    if !pred.root(root_entry) {
        return false;
    }
    pred.member(root_entry)
        || index
            .members_of(root)
            .iter()
            .filter_map(|&o| index.get(o))
            .any(|m| pred.member(m))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::level::Level;
    use chrono::DateTime;

    fn known(names: &[&str]) -> BTreeSet<String> {
        names.iter().map(|s| s.to_string()).collect()
    }

    fn entry(ordinal: usize) -> Entry {
        Entry {
            ordinal,
            timestamp: Some(DateTime::from_timestamp_millis(ordinal as i64).unwrap()),
            timestamp_used_default_tz: false,
            timestamp_year_inferred: false,
            level: None,
            message: format!("m{ordinal}"),
            fields: BTreeMap::new(),
            continuation_of: None,
        }
    }

    fn with(ordinal: usize, level: Option<Level>, message: &str, fields: &[(&str, &str)]) -> Entry {
        Entry {
            level,
            message: message.to_string(),
            fields: fields
                .iter()
                .map(|(k, v)| (k.to_string(), FieldValue::String(v.to_string())))
                .collect(),
            ..entry(ordinal)
        }
    }

    fn member(ordinal: usize, root: usize, message: &str) -> Entry {
        Entry {
            message: message.to_string(),
            continuation_of: Some(root),
            ..entry(ordinal)
        }
    }

    fn filters(pairs: &[(&str, &[&str])]) -> FieldFilters {
        pairs
            .iter()
            .map(|(f, vs)| (f.to_string(), vs.iter().map(|v| v.to_string()).collect()))
            .collect()
    }

    fn substring(s: &str) -> CompiledQuery {
        CompiledQuery::Substring(s.to_string())
    }

    fn regex(s: &str) -> CompiledQuery {
        CompiledQuery::Regex(Regex::new(s).unwrap())
    }

    fn index(entries: Vec<Entry>) -> EntryIndex {
        let mut idx = EntryIndex::new();
        for e in entries {
            idx.append(e);
        }
        idx
    }

    fn pred(filters: FieldFilters, query: CompiledQuery) -> Option<Predicate> {
        Some(Predicate { filters, query })
    }

    fn shown(set: &MatchSet, idx: &EntryIndex) -> Vec<usize> {
        idx.iter()
            .map(|e| e.ordinal)
            .filter(|&o| set.contains(o))
            .collect()
    }

    // ---- compile_query ------------------------------------------------------

    #[test]
    fn empty_text_compiles_to_none() {
        assert!(compile_query("").unwrap().is_none());
    }

    #[test]
    fn plain_text_compiles_to_a_lowercased_substring() {
        assert!(
            matches!(compile_query("Connection Refused").unwrap(), CompiledQuery::Substring(n) if n == "connection refused")
        );
    }

    #[test]
    fn re_prefix_compiles_a_regex_applied_as_written() {
        let CompiledQuery::Regex(re) = compile_query("re:^ERROR.*timeout").unwrap() else {
            panic!("expected a regex");
        };
        assert!(re.is_match("ERROR: timeout"));
        assert!(!re.is_match("error: timeout"));
    }

    #[test]
    fn an_unbalanced_bracket_fails_loudly() {
        let err = compile_query("re:[unclosed").unwrap_err();
        assert!(err.starts_with("invalid regex"), "{err}");
    }

    #[test]
    fn look_around_is_reported_as_unsupported() {
        let err = compile_query("re:foo(?=bar)").unwrap_err();
        assert!(err.contains("look-around"), "{err}");
        assert!(!err.contains('\n'), "{err}");
    }

    #[test]
    fn backreference_is_a_compile_error() {
        assert!(compile_query(r"re:(a)\1").is_err());
    }

    #[test]
    fn the_escape_gives_a_literal_substring_starting_with_re() {
        assert!(
            matches!(compile_query("\\re:something").unwrap(), CompiledQuery::Substring(n) if n == "re:something")
        );
    }

    // ---- extract_field_tokens ----------------------------------------------

    #[test]
    fn converts_a_complete_token_followed_by_a_space() {
        let (tokens, rest) = extract_field_tokens("service=api ", &known(&["level", "service"]));
        assert_eq!(
            tokens,
            vec![ExtractedToken {
                field: "service".into(),
                value: "api".into()
            }]
        );
        assert_eq!(rest, "");
    }

    #[test]
    fn leaves_the_in_progress_token_alone() {
        let (tokens, rest) = extract_field_tokens("service=api", &known(&["level", "service"]));
        assert!(tokens.is_empty());
        assert_eq!(rest, "service=api");
    }

    #[test]
    fn mixes_a_chip_with_remaining_text() {
        let (tokens, rest) = extract_field_tokens(
            "level=ERROR connection refused",
            &known(&["level", "service"]),
        );
        assert_eq!(
            tokens,
            vec![ExtractedToken {
                field: "level".into(),
                value: "ERROR".into()
            }]
        );
        assert_eq!(rest, "connection refused");
    }

    #[test]
    fn unknown_field_stays_text() {
        let (tokens, rest) = extract_field_tokens("unknownfield=x ", &known(&["level", "service"]));
        assert!(tokens.is_empty());
        assert_eq!(rest, "unknownfield=x");
    }

    #[test]
    fn several_complete_tokens() {
        let (tokens, rest) =
            extract_field_tokens("level=ERROR service=api ", &known(&["level", "service"]));
        assert_eq!(tokens.len(), 2);
        assert_eq!(rest, "");
    }

    // ---- predicate -----------------------------------------------------------

    #[test]
    fn same_field_values_widen_and_different_fields_narrow() {
        let error_api = with(1, Some(Level::Error), "", &[("service", "api")]);
        let warn_api = with(2, Some(Level::Warn), "", &[("service", "api")]);
        let error_db = with(3, Some(Level::Error), "", &[("service", "db")]);
        let info_api = with(4, Some(Level::Info), "", &[("service", "api")]);

        let or = filters(&[("level", &["ERROR", "WARN"])]);
        assert!(matches_field_filters(&error_api, &or));
        assert!(matches_field_filters(&warn_api, &or));
        assert!(matches_field_filters(&error_db, &or));
        assert!(!matches_field_filters(&info_api, &or));

        let and = filters(&[("level", &["ERROR"]), ("service", &["api"])]);
        assert!(matches_field_filters(&error_api, &and));
        assert!(!matches_field_filters(&error_db, &and));
        assert!(!matches_field_filters(&warn_api, &and));
        assert!(!matches_field_filters(&info_api, &and));
    }

    #[test]
    fn empty_value_set_imposes_nothing_and_absent_field_never_matches() {
        let info = with(1, Some(Level::Info), "", &[]);
        assert!(matches_field_filters(&info, &filters(&[("level", &[])])));
        assert!(!matches_field_filters(
            &info,
            &filters(&[("service", &["api"])])
        ));
    }

    #[test]
    fn query_covers_message_and_values_not_names() {
        let e = Entry {
            fields: [
                ("service".to_string(), FieldValue::String("api".into())),
                ("code".to_string(), FieldValue::Number("500".into())),
            ]
            .into(),
            ..with(1, Some(Level::Error), "Connection Refused by upstream", &[])
        };
        assert!(matches_query(&e, &substring("connection refused")));
        assert!(!matches_query(&e, &substring("nope")));
        assert!(matches_query(&e, &substring("api")));
        assert!(!matches_query(&e, &substring("service")));
        assert!(matches_query(&e, &regex("^Connection")));
        assert!(!matches_query(&e, &regex("^connection")));
        assert!(matches_query(&e, &CompiledQuery::None));
    }

    #[test]
    fn known_fields_always_include_level() {
        let mut inv = FieldInventory::new(10);
        inv.record("service", "api");
        let names = known_field_names(&inv);
        assert!(names.contains("level") && names.contains("service") && !names.contains("unknown"));
        assert!(known_field_names(&FieldInventory::new(10)).contains("level"));
    }

    #[test]
    fn match_ranges_find_every_substring_occurrence() {
        assert_eq!(
            find_match_ranges("ababAB", &substring("ab")),
            vec![(0, 2), (2, 4), (4, 6)]
        );
    }

    #[test]
    fn match_ranges_skip_zero_length_regex_matches() {
        assert!(!find_match_ranges("aaa", &regex("a*")).is_empty());
        assert_eq!(find_match_ranges("anything", &CompiledQuery::None), vec![]);
    }

    #[test]
    fn match_ranges_are_utf16_offsets() {
        // "é" is 2 bytes in UTF-8 but 1 UTF-16 unit; "𝄞" is 4 bytes and 2 units.
        assert_eq!(find_match_ranges("é𝄞ab", &substring("ab")), vec![(3, 5)]);
        assert_eq!(find_match_ranges("é𝄞ab", &regex("b")), vec![(4, 5)]);
    }

    #[test]
    fn the_two_halves_split_filters_and_query() {
        let p = Predicate {
            filters: filters(&[("level", &["ERROR"])]),
            query: substring("iobridge"),
        };
        let root = with(1, Some(Level::Error), "connection failed", &[]);
        let frame = member(2, 1, "at com.example.IoBridge.connect");
        assert!(p.root(&root) && !p.root(&frame));
        assert!(p.member(&frame) && !p.member(&root));
    }

    // ---- MatchSet over an index ---------------------------------------------

    #[test]
    fn no_predicate_matches_everything() {
        let idx = index(vec![
            with(1, Some(Level::Error), "", &[]),
            with(2, Some(Level::Info), "", &[]),
        ]);
        let set = MatchSet::new(None, &idx);
        assert!(!set.is_filtering());
        assert_eq!(set.len(&idx), 2);
        assert_eq!(shown(&set, &idx), vec![1, 2]);
    }

    #[test]
    fn filters_and_new_arrivals_are_tested_once() {
        let mut idx = index(vec![with(1, Some(Level::Info), "", &[])]);
        let mut set = MatchSet::new(
            pred(filters(&[("level", &["ERROR"])]), CompiledQuery::None),
            &idx,
        );
        assert_eq!(set.len(&idx), 0);
        idx.append(with(2, Some(Level::Error), "", &[]));
        set.catch_up(&idx);
        assert_eq!(set.ordinals().unwrap(), &[2]);
    }

    #[test]
    fn eviction_removes_from_the_match_set() {
        let mut idx = index(vec![
            with(1, Some(Level::Error), "", &[]),
            with(2, Some(Level::Error), "", &[]),
        ]);
        let mut set = MatchSet::new(
            pred(filters(&[("level", &["ERROR"])]), CompiledQuery::None),
            &idx,
        );
        assert_eq!(set.len(&idx), 2);
        idx.evict_front(1);
        set.catch_up(&idx);
        assert_eq!(set.ordinals().unwrap(), &[2]);
    }

    #[test]
    fn a_chain_is_shown_as_a_unit_deciding_on_the_root() {
        let idx = index(vec![
            with(1, Some(Level::Error), "boom", &[]),
            member(2, 1, "a"),
            member(3, 1, "b"),
            with(4, Some(Level::Info), "ok", &[]),
        ]);
        let set = MatchSet::new(
            pred(filters(&[("level", &["ERROR"])]), CompiledQuery::None),
            &idx,
        );
        assert_eq!(shown(&set, &idx), vec![1, 2, 3]);
    }

    #[test]
    fn a_chain_whose_root_fails_is_hidden_even_if_a_member_would_pass() {
        let mut m = member(2, 1, "x");
        m.level = Some(Level::Error);
        let idx = index(vec![
            with(1, Some(Level::Info), "", &[]),
            m,
            with(3, Some(Level::Error), "", &[]),
        ]);
        let set = MatchSet::new(
            pred(filters(&[("level", &["ERROR"])]), CompiledQuery::None),
            &idx,
        );
        assert_eq!(shown(&set, &idx), vec![3]);
    }

    #[test]
    fn a_query_matching_only_a_member_surfaces_the_whole_chain() {
        let idx = index(vec![
            with(1, Some(Level::Error), "boom", &[]),
            member(2, 1, "at com.example.IoBridge.connect"),
            member(3, 1, "at com.example.Other.run"),
            with(4, Some(Level::Info), "ok", &[]),
        ]);
        let set = MatchSet::new(pred(FieldFilters::new(), substring("iobridge")), &idx);
        assert_eq!(shown(&set, &idx), vec![1, 2, 3]);
        assert_eq!(set.len(&idx), 3);
    }

    #[test]
    fn a_live_matching_member_pulls_in_its_retained_chain() {
        let mut idx = index(vec![with(1, Some(Level::Error), "boom", &[])]);
        let mut set = MatchSet::new(pred(FieldFilters::new(), substring("iobridge")), &idx);
        assert_eq!(set.len(&idx), 0);
        idx.append(member(2, 1, "at com.example.IoBridge.connect"));
        set.catch_up(&idx);
        assert_eq!(set.ordinals().unwrap(), &[1, 2]);
        idx.append(member(3, 1, "at x"));
        set.catch_up(&idx);
        assert_eq!(set.ordinals().unwrap(), &[1, 2, 3]);
    }

    #[test]
    fn an_orphaned_member_is_filtered_on_its_own_values() {
        let mut idx = index(vec![
            with(1, Some(Level::Error), "", &[]),
            member(2, 1, "a"),
            member(3, 1, "b"),
        ]);
        idx.evict_front(1);
        let set = MatchSet::new(pred(FieldFilters::new(), substring("b")), &idx);
        assert_eq!(shown(&set, &idx), vec![3]);
    }
}
