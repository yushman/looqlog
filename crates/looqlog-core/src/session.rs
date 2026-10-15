//! The engine behind the query API (`query-api` spec, ADR-0010, design.md D1/D3/D5).
//!
//! A [`Session`] holds the data of one source — the parser, the index and the retained
//! raw lines. Query state lives in a [`View`], one per connected client, so two tabs on
//! the same source never see each other's filters. A view catches up with appended and
//! evicted entries lazily, the next time it is asked for anything. No I/O, no async, no
//! clock: the caller feeds bytes and decides when a detection hold has timed out.

use std::collections::{BTreeMap, BTreeSet, HashSet, VecDeque};

use chrono::{DateTime, FixedOffset, Utc};

use crate::bucket::{bucket_count_for_span, pick_bucket_width_ms, TARGET_BUCKET_COUNT};
use crate::dto::{
    BucketsReplyDto, DetectionResultDto, DiagnosticsSummaryDto, EntryReplyDto, FieldInventoryDto,
    FieldTokenDto, FieldValueDto, QueryReplyDto, RowDto, StatsDto, TimeRangeDto, WindowReplyDto,
};
use crate::entry::Entry;
use crate::format::Format;
use crate::index::EntryIndex;
use crate::parser::Parser;
use crate::query::{
    compile_query, extract_field_tokens, find_match_ranges, known_field_names, CompiledQuery,
    FieldFilters, MatchSet, Predicate,
};
use crate::timestamp::{ParseContext, TimeZonePolicy};

/// Most rows one `window` call returns.
pub const MAX_WINDOW_ROWS: usize = 1000;

/// Per retained raw line: its `Box` in the deque plus allocator slack.
const RAW_LINE_BYTES: usize = 16;

/// Re-parse feeds retained lines to the parser in chunks of about this size.
const REPARSE_CHUNK_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, Default)]
pub struct SessionConfig {
    /// Retained raw lines; the oldest are evicted beyond it (stdin `--max-lines`).
    pub max_lines: Option<usize>,
    /// "Now", for dating timestamp shapes that carry no year. Supplied by the caller
    /// because this crate never reads the clock (ADR-0005).
    pub reference: Option<DateTime<Utc>>,
}

/// What a client asks for: the whole view state (`query-api` "The query is the whole
/// view state").
#[derive(Debug, Clone, Default)]
pub struct QueryInput {
    pub filters: FieldFilters,
    pub search: String,
    pub range: Option<TimeRangeDto>,
    /// Replaces the toggled chain roots when present; kept otherwise.
    pub toggled: Option<Vec<usize>>,
}

/// One client's query state over a session.
#[derive(Debug)]
pub struct View {
    generation: u64,
    filters: FieldFilters,
    query: CompiledQuery,
    range: Option<TimeRangeDto>,
    toggled: BTreeSet<usize>,
    /// Session epoch and revision the derived state below was computed at.
    epoch: Option<u64>,
    revision: u64,
    match_set: Option<MatchSet>,
    /// Ordinals passing filters, search and range, in input order.
    base: Vec<usize>,
    /// Chain roots with a member whose message the search matched.
    auto_expanded: HashSet<usize>,
    /// `base` with the members of collapsed chains removed: what offsets address.
    rows: Vec<usize>,
}

impl Default for View {
    fn default() -> Self {
        Self {
            generation: 0,
            filters: FieldFilters::new(),
            query: CompiledQuery::None,
            range: None,
            toggled: BTreeSet::new(),
            epoch: None,
            revision: 0,
            match_set: None,
            base: Vec::new(),
            auto_expanded: HashSet::new(),
            rows: Vec::new(),
        }
    }
}

impl View {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    fn contains(&self, ordinal: usize) -> bool {
        self.match_set
            .as_ref()
            .map_or(true, |m| m.contains(ordinal))
    }

    fn in_base(&self, ordinal: usize) -> bool {
        self.base.binary_search(&ordinal).is_ok()
    }

    fn is_expanded(&self, root: usize) -> bool {
        self.auto_expanded.contains(&root) != self.toggled.contains(&root)
    }

    /// The root of `entry`'s visible chain, when that root is in the view too.
    fn visible_root(&self, entry: &Entry) -> Option<usize> {
        entry.continuation_of.filter(|&r| self.in_base(r))
    }
}

pub struct Session {
    config: SessionConfig,
    format_override: Option<Format>,
    tz_offset_minutes: Option<i32>,
    parser: Parser,
    index: EntryIndex,
    raw: VecDeque<Box<[u8]>>,
    raw_bytes: usize,
    /// Line number of `raw[0]`, 1-based.
    first_line: usize,
    pending: Vec<u8>,
    evicted_lines: u64,
    bytes_ingested: u64,
    finished: bool,
    detection_forced: bool,
    /// Bumped when the source is re-parsed: every view rebuilds from scratch.
    epoch: u64,
    /// Bumped whenever entries are appended or evicted: views catch up.
    revision: u64,
}

impl Session {
    pub fn new(config: SessionConfig) -> Self {
        let parser = new_parser(&config, None, None).expect("no timezone override");
        Self {
            config,
            format_override: None,
            tz_offset_minutes: None,
            parser,
            index: EntryIndex::new(),
            raw: VecDeque::new(),
            raw_bytes: 0,
            first_line: 1,
            pending: Vec::new(),
            evicted_lines: 0,
            bytes_ingested: 0,
            finished: false,
            detection_forced: false,
            epoch: 0,
            revision: 0,
        }
    }

    /// Feeds the next chunk of input. A trailing partial line is held until the next
    /// chunk or [`Session::finish`].
    pub fn ingest(&mut self, chunk: &[u8]) {
        self.bytes_ingested += chunk.len() as u64;
        let mut start = 0;
        for (i, &b) in chunk.iter().enumerate() {
            if b == b'\n' {
                let line = if self.pending.is_empty() {
                    chunk[start..i].into()
                } else {
                    self.pending.extend_from_slice(&chunk[start..i]);
                    std::mem::take(&mut self.pending).into_boxed_slice()
                };
                self.raw_bytes += line.len();
                self.raw.push_back(line);
                start = i + 1;
            }
        }
        self.pending.extend_from_slice(&chunk[start..]);
        let entries = self.parser.feed(chunk);
        self.absorb(entries);
    }

    /// End of input: flushes a trailing line and finalizes detection.
    pub fn finish(&mut self) {
        if !self.pending.is_empty() {
            let line = std::mem::take(&mut self.pending).into_boxed_slice();
            self.raw_bytes += line.len();
            self.raw.push_back(line);
        }
        self.finished = true;
        let entries = self.parser.finish();
        self.absorb(entries);
    }

    /// Whether the opening lines are still held for format detection.
    pub fn is_detecting(&self) -> bool {
        self.parser.format().is_none()
    }

    /// Ends the detection hold (the caller's timeout expired) without ending input.
    pub fn force_detection(&mut self) {
        let entries = self.parser.force_detection();
        if self.parser.format().is_some() {
            self.detection_forced = true;
        }
        self.absorb(entries);
    }

    fn absorb(&mut self, entries: Vec<Entry>) {
        let changed = !entries.is_empty();
        for entry in entries {
            self.index.append(entry);
        }
        let evicted = self.evict();
        if changed || evicted {
            self.revision += 1;
        }
    }

    fn evict(&mut self) -> bool {
        let Some(max) = self.config.max_lines else {
            return false;
        };
        let max = max.max(1);
        if self.raw.len() > max {
            let n = self.raw.len() - max;
            self.raw_bytes -= self.raw.drain(..n).map(|l| l.len()).sum::<usize>();
            self.first_line += n;
            self.evicted_lines += n as u64;
        }
        // Also catches entries the detection hold released after their lines left.
        self.index.evict_before(self.first_line) > 0
    }

    /// Re-parses the retained lines with `format` (detection when `None`) and an
    /// optional fixed UTC offset for timestamps that carry none. Applies to lines
    /// ingested afterwards too. Every view rebuilds on its next call.
    pub fn set_format(
        &mut self,
        format: Option<Format>,
        tz_offset_minutes: Option<i32>,
    ) -> Result<StatsDto, String> {
        let mut parser =
            new_parser(&self.config, format, tz_offset_minutes)?.starting_at_line(self.first_line);
        self.format_override = format;
        self.tz_offset_minutes = tz_offset_minutes;
        let mut index = EntryIndex::new();
        let mut buf = Vec::with_capacity(REPARSE_CHUNK_BYTES);
        for line in &self.raw {
            buf.extend_from_slice(line);
            buf.push(b'\n');
            if buf.len() >= REPARSE_CHUNK_BYTES {
                parser.feed(&buf).into_iter().for_each(|e| index.append(e));
                buf.clear();
            }
        }
        buf.extend_from_slice(&self.pending);
        parser.feed(&buf).into_iter().for_each(|e| index.append(e));
        if self.finished {
            parser.finish().into_iter().for_each(|e| index.append(e));
        } else if self.detection_forced {
            parser
                .force_detection()
                .into_iter()
                .for_each(|e| index.append(e));
        }
        self.parser = parser;
        self.index = index;
        self.epoch += 1;
        self.revision += 1;
        Ok(self.stats())
    }

    pub fn format_override(&self) -> Option<Format> {
        self.format_override
    }

    pub fn tz_offset_minutes(&self) -> Option<i32> {
        self.tz_offset_minutes
    }

    pub fn epoch(&self) -> u64 {
        self.epoch
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn total(&self) -> usize {
        self.index.len()
    }

    pub fn evicted_lines(&self) -> u64 {
        self.evicted_lines
    }

    pub fn bytes_ingested(&self) -> u64 {
        self.bytes_ingested
    }

    pub fn is_finished(&self) -> bool {
        self.finished
    }

    /// Bytes held for retained raw lines — the part of memory that scales with input
    /// besides the index itself.
    pub fn retained_raw_bytes(&self) -> usize {
        self.raw_bytes + self.pending.len()
    }

    /// Estimated heap this session retains: raw lines plus parsed entries, calibrated
    /// against `mem_probe -- session` (docs/devlog.md). The memory budget counts this.
    pub fn retained_bytes(&self) -> usize {
        self.retained_raw_bytes() + self.raw.len() * RAW_LINE_BYTES + self.index.retained_bytes()
    }

    pub fn stats(&self) -> StatsDto {
        StatsDto {
            format: self.parser.format().map(|f| f.as_str().to_string()),
            detection: self.parser.detection().map(DetectionResultDto::from),
            diagnostics: DiagnosticsSummaryDto::from(self.parser.diagnostics()),
            field_inventory: FieldInventoryDto::from(self.parser.field_inventory()),
            level_counts: self
                .index
                .level_counts()
                .iter()
                .map(|(l, c)| (l.as_str().to_string(), *c))
                .collect(),
            entries: self.index.len(),
            total_lines: self.first_line - 1 + self.raw.len(),
            blank_lines: self.parser.blank_lines(),
            evicted_lines: self.evicted_lines,
            bytes_ingested: self.bytes_ingested,
            finished: self.finished,
        }
    }

    // ---- views ------------------------------------------------------------------

    /// Brings `view` up to date with the session: a full rebuild after a re-parse or
    /// a query change, an incremental catch-up after appends and evictions.
    fn refresh(&mut self, view: &mut View) {
        let stale_epoch = view.epoch != Some(self.epoch);
        if stale_epoch || view.match_set.is_none() {
            let predicate = Predicate {
                filters: view.filters.clone(),
                query: view.query.clone(),
            };
            view.match_set = Some(MatchSet::new(Some(predicate), &self.index));
        } else if view.revision != self.revision {
            if let Some(m) = view.match_set.as_mut() {
                m.catch_up(&self.index);
            }
        } else {
            return;
        }
        view.epoch = Some(self.epoch);
        view.revision = self.revision;
        self.recompute_base(view);
    }

    fn recompute_base(&mut self, view: &mut View) {
        let index = &self.index;
        let ms = view.match_set.as_ref().expect("refreshed");
        view.base = match view.range {
            Some(r) => index.range_ordinals(r.start_ms, r.end_ms, |o| ms.contains(o)),
            None => match ms.ordinals() {
                Some(o) => o.to_vec(),
                None => index.iter().map(|e| e.ordinal).collect(),
            },
        };
        view.auto_expanded.clear();
        if !view.query.is_none() {
            for entry in entries_for(index, &view.base) {
                if let Some(root) = view.visible_root(entry) {
                    if !view.auto_expanded.contains(&root)
                        && !find_match_ranges(&entry.message, &view.query).is_empty()
                    {
                        view.auto_expanded.insert(root);
                    }
                }
            }
        }
        Self::collapse(index, view);
    }

    fn collapse(index: &EntryIndex, view: &mut View) {
        let rows: Vec<usize> = entries_for(index, &view.base)
            .filter(|e| view.visible_root(e).map_or(true, |r| view.is_expanded(r)))
            .map(|e| e.ordinal)
            .collect();
        view.rows = rows;
    }

    /// Applies a new view state. A search that does not compile leaves the previous
    /// generation current and reports the error.
    pub fn set_query(&mut self, view: &mut View, input: QueryInput) -> QueryReplyDto {
        let known = known_field_names(self.parser.field_inventory());
        let (tokens, remaining) = extract_field_tokens(&input.search, &known);
        let compiled = match compile_query(&remaining) {
            Ok(c) => c,
            Err(err) => {
                self.refresh(view);
                return self.query_reply(view, Vec::new(), input.search, Some(err));
            }
        };
        let mut filters = input.filters;
        for t in &tokens {
            filters
                .entry(t.field.clone())
                .or_default()
                .insert(t.value.clone());
        }
        view.filters = filters;
        view.query = compiled;
        view.range = input.range;
        if let Some(toggled) = input.toggled {
            view.toggled = toggled.into_iter().collect();
        }
        view.generation += 1;
        view.match_set = None;
        self.refresh(view);
        let tokens = tokens
            .into_iter()
            .map(|t| FieldTokenDto {
                field: t.field,
                value: t.value,
            })
            .collect();
        self.query_reply(view, tokens, remaining, None)
    }

    fn query_reply(
        &self,
        view: &View,
        tokens: Vec<FieldTokenDto>,
        remaining_text: String,
        compile_error: Option<String>,
    ) -> QueryReplyDto {
        let timestampless_excluded = if view.range.is_some() {
            self.index
                .iter()
                .filter(|e| e.timestamp.is_none() && view.contains(e.ordinal))
                .count()
        } else {
            0
        };
        QueryReplyDto {
            generation: view.generation,
            total: self.index.len(),
            matched: view.base.len(),
            row_count: view.rows.len(),
            timestampless_excluded,
            tokens,
            remaining_text,
            compile_error,
        }
    }

    /// Flips one chain between expanded and collapsed. Returns the new row count.
    /// Re-folds the rows; never re-evaluates the predicate.
    pub fn toggle_chain(&mut self, view: &mut View, root: usize) -> usize {
        self.refresh(view);
        if !view.toggled.remove(&root) {
            view.toggled.insert(root);
        }
        Self::collapse(&self.index, view);
        view.rows.len()
    }

    /// Entries passing the view's filters, search and range, and the visible rows.
    pub fn counts(&mut self, view: &mut View) -> (usize, usize) {
        self.refresh(view);
        (view.base.len(), view.rows.len())
    }

    /// Up to `limit` (capped at [`MAX_WINDOW_ROWS`]) visible rows from `offset`.
    pub fn window(&mut self, view: &mut View, offset: usize, limit: usize) -> WindowReplyDto {
        self.refresh(view);
        let end = offset
            .saturating_add(limit.min(MAX_WINDOW_ROWS))
            .min(view.rows.len());
        let ordinals = view.rows.get(offset..end).unwrap_or(&[]);
        let rows = entries_for(&self.index, ordinals)
            .map(|e| self.row(view, e))
            .collect();
        WindowReplyDto {
            generation: view.generation,
            offset,
            row_count: view.rows.len(),
            matched: view.base.len(),
            total: self.index.len(),
            rows,
        }
    }

    /// Visible-row position of `ordinal`, or of the first row after it when it is not
    /// visible (filtered out, folded into a chain, or evicted) — how a client keeps its
    /// scroll anchored to an entry while rows come and go.
    pub fn locate(&mut self, view: &mut View, ordinal: usize) -> usize {
        self.refresh(view);
        view.rows.partition_point(|&o| o < ordinal)
    }

    /// The full entry and its raw line, or `None`s once it has been evicted.
    pub fn entry(&mut self, view: &mut View, ordinal: usize) -> EntryReplyDto {
        self.refresh(view);
        match self.index.get(ordinal) {
            None => EntryReplyDto {
                row: None,
                raw: None,
            },
            Some(e) => EntryReplyDto {
                row: Some(self.row(view, e)),
                raw: ordinal
                    .checked_sub(self.first_line)
                    .and_then(|i| self.raw.get(i))
                    .map(|l| String::from_utf8_lossy(l).into_owned()),
            },
        }
    }

    fn row(&self, view: &View, e: &Entry) -> RowDto {
        let chain_member = view.visible_root(e).is_some();
        let chain_size = if chain_member || !view.in_base(e.ordinal) {
            0
        } else {
            let members = self
                .index
                .members_of(e.ordinal)
                .iter()
                .filter(|&&o| view.in_base(o))
                .count();
            if members == 0 {
                0
            } else {
                members + 1
            }
        };
        RowDto {
            ordinal: e.ordinal,
            timestamp: e.timestamp.map(|ts| ts.to_rfc3339()),
            timestamp_used_default_tz: e.timestamp_used_default_tz,
            timestamp_year_inferred: e.timestamp_year_inferred,
            level: e.level.map(|l| l.as_str().to_string()),
            message: e.message.clone(),
            fields: e
                .fields
                .iter()
                .map(|(k, v)| (k.clone(), FieldValueDto::from(v)))
                .collect(),
            continuation_of: e.continuation_of,
            chain_size,
            expanded: chain_size > 0 && view.is_expanded(e.ordinal),
            chain_member,
            match_ranges: find_match_ranges(&e.message, &view.query),
        }
    }

    /// Event counts per bucket over `span`, or the robust span (the full span when
    /// `full_span`), both unfiltered and for the view's match set. The time range is
    /// not applied: the timeline shows it as a selection over the whole.
    pub fn buckets(
        &mut self,
        view: &mut View,
        span: Option<TimeRangeDto>,
        full_span: bool,
    ) -> BucketsReplyDto {
        self.refresh(view);
        let bounds = match span {
            Some(r) => Some((r.start_ms, r.end_ms, 0)),
            None if full_span => self.index.full_span().map(|(a, b)| (a, b, 0)),
            None => self
                .index
                .robust_span()
                .map(|s| (s.min_ms, s.max_ms, s.outlier_count)),
        };
        let mut reply = BucketsReplyDto {
            generation: view.generation,
            start_ms: None,
            bucket_ms: 0,
            total: Vec::new(),
            filtered: Vec::new(),
            timestamped: self.index.timestamped_count(),
            timestampless: self.index.timestampless_count(),
            outlier_count: 0,
        };
        let Some((mut min, mut max, outliers)) = bounds else {
            return reply;
        };
        if max <= min {
            // A single instant: pad so a bucket has a non-zero width.
            min -= 500;
            max = min + 1000;
        }
        let width = pick_bucket_width_ms(max - min, TARGET_BUCKET_COUNT);
        // Align the grid to a multiple of its own width, so boundaries land on clean marks.
        let start = min.div_euclid(width) * width;
        // Through `max` inclusive: an entry exactly at the end is still counted.
        let count = bucket_count_for_span(max + 1 - start, width);
        reply.start_ms = Some(start);
        reply.bucket_ms = width;
        reply.total = self.index.bucket_counts(start, width, count, |_| true);
        reply.filtered = self
            .index
            .bucket_counts(start, width, count, |o| view.contains(o));
        reply.outlier_count = outliers;
        reply
    }
}

fn new_parser(
    config: &SessionConfig,
    format: Option<Format>,
    tz_offset_minutes: Option<i32>,
) -> Result<Parser, String> {
    let tz = match tz_offset_minutes {
        None => TimeZonePolicy::utc(),
        Some(minutes) => minutes
            .checked_mul(60)
            .and_then(FixedOffset::east_opt)
            .map(TimeZonePolicy::fixed_offset)
            .ok_or_else(|| format!("timezone offset out of range: {minutes} minutes"))?,
    };
    let mut ctx = ParseContext::new(tz);
    if let Some(reference) = config.reference {
        ctx = ctx.with_reference(reference);
    }
    Ok(Parser::with_context(format, ctx))
}

/// Entries for sorted `ordinals`, in one forward walk over the index.
fn entries_for<'a>(
    index: &'a EntryIndex,
    ordinals: &'a [usize],
) -> impl Iterator<Item = &'a Entry> {
    let start = ordinals
        .first()
        .and_then(|&o| index.position(o))
        .unwrap_or(0);
    let mut it = index.iter_from(start).peekable();
    ordinals.iter().filter_map(move |&o| {
        while it.peek().is_some_and(|e| e.ordinal < o) {
            it.next();
        }
        it.peek().filter(|e| e.ordinal == o).copied()
    })
}

/// Field filters from `(field, values)` pairs.
pub fn filters_from<I, F, V>(pairs: I) -> FieldFilters
where
    I: IntoIterator<Item = (F, Vec<V>)>,
    F: Into<String>,
    V: Into<String>,
{
    let mut out: FieldFilters = BTreeMap::new();
    for (f, vs) in pairs {
        out.entry(f.into())
            .or_default()
            .extend(vs.into_iter().map(Into::into));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(text: &str) -> Session {
        let mut s = Session::new(SessionConfig::default());
        s.ingest(text.as_bytes());
        s.finish();
        s
    }

    fn query(s: &mut Session, v: &mut View, filters: FieldFilters, search: &str) -> QueryReplyDto {
        s.set_query(
            v,
            QueryInput {
                filters,
                search: search.to_string(),
                ..Default::default()
            },
        )
    }

    fn ordinals(w: &WindowReplyDto) -> Vec<usize> {
        w.rows.iter().map(|r| r.ordinal).collect()
    }

    const JSONL: &str = concat!(
        r#"{"ts":"2026-08-09T10:00:00Z","level":"ERROR","msg":"db timeout","service":"api"}"#,
        "\n",
        r#"{"ts":"2026-08-09T10:00:01Z","level":"INFO","msg":"ok","service":"api"}"#,
        "\n",
        r#"{"ts":"2026-08-09T10:00:02Z","level":"ERROR","msg":"disk full","service":"db"}"#,
        "\n",
        r#"{"ts":"2026-08-09T10:00:03Z","level":"WARN","msg":"slow timeout","service":"db"}"#,
        "\n",
        r#"{"ts":"2026-08-09T10:00:04Z","level":"ERROR","msg":"another timeout","service":"db"}"#,
        "\n",
    );

    const TRACE: &str = "2026-08-09T10:00:00Z ERROR request failed\n\
        java.lang.IllegalStateException: boom\n\
        \tat com.example.IoBridge.connect(IoBridge.java:10)\n\
        \tat com.example.Other.run(Other.java:20)\n\
        2026-08-09T10:00:01Z INFO all good\n";

    #[test]
    fn retained_bytes_grows_on_ingest_and_shrinks_on_eviction() {
        let lines = [
            "a=1 msg=one\n",
            "b=2 msg=two\n",
            "c=3 msg=three\n",
            "d=4 msg=four\n",
        ];
        let feed = |max_lines| {
            let mut s = Session::new(SessionConfig {
                max_lines,
                ..Default::default()
            });
            assert_eq!(s.retained_bytes(), 0);
            s.ingest(lines[0].as_bytes());
            s.force_detection();
            let mut sizes = vec![s.retained_bytes()];
            for line in &lines[1..] {
                s.ingest(line.as_bytes());
                sizes.push(s.retained_bytes());
            }
            sizes
        };
        let unbounded = feed(None);
        assert!(unbounded.windows(2).all(|w| w[0] < w[1]), "{unbounded:?}");
        let bounded = feed(Some(2));
        assert_eq!(bounded[..2], unbounded[..2]);
        assert!(bounded[3] < unbounded[3]);
        assert!(bounded[3] < unbounded[2]);
    }

    #[test]
    fn filter_and_search_combine() {
        let mut s = session(JSONL);
        let mut v = View::new();
        let r = query(
            &mut s,
            &mut v,
            filters_from([("level", vec!["ERROR"])]),
            "timeout",
        );
        assert_eq!(r.total, 5);
        assert_eq!(r.matched, 2);
        assert_eq!(ordinals(&s.window(&mut v, 0, 10)), vec![1, 5]);
    }

    #[test]
    fn field_tokens_in_the_search_become_filters() {
        let mut s = session(JSONL);
        let mut v = View::new();
        let r = query(&mut s, &mut v, FieldFilters::new(), "service=db timeout");
        assert_eq!(
            r.tokens,
            vec![FieldTokenDto {
                field: "service".into(),
                value: "db".into()
            }]
        );
        assert_eq!(r.remaining_text, "timeout");
        assert_eq!(r.matched, 2);
    }

    #[test]
    fn compile_error_keeps_the_previous_generation() {
        let mut s = session(JSONL);
        let mut v = View::new();
        let ok = query(&mut s, &mut v, FieldFilters::new(), "timeout");
        let bad = query(&mut s, &mut v, FieldFilters::new(), "re:[unclosed");
        assert!(bad.compile_error.is_some());
        assert_eq!(bad.generation, ok.generation);
        assert_eq!(bad.matched, 3);
        assert_eq!(s.window(&mut v, 0, 10).rows.len(), 3);
        let look = query(&mut s, &mut v, FieldFilters::new(), "re:foo(?=bar)");
        assert!(look.compile_error.unwrap().contains("look-around"));
    }

    #[test]
    fn replies_carry_the_generation_they_were_computed_for() {
        let mut s = session(JSONL);
        let mut v = View::new();
        let g1 = query(&mut s, &mut v, FieldFilters::new(), "").generation;
        let w = s.window(&mut v, 0, 1);
        let g2 = query(&mut s, &mut v, FieldFilters::new(), "disk").generation;
        assert_eq!(w.generation, g1);
        assert!(g2 > g1);
        assert_eq!(s.window(&mut v, 0, 1).generation, g2);
    }

    #[test]
    fn window_past_the_end_is_empty_not_an_error() {
        let mut s = session(JSONL);
        let mut v = View::new();
        let w = s.window(&mut v, 100, 10);
        assert!(w.rows.is_empty());
        assert_eq!(w.matched, 5);
    }

    #[test]
    fn two_views_keep_their_own_filters() {
        let mut s = session(JSONL);
        let (mut a, mut b) = (View::new(), View::new());
        query(&mut s, &mut a, filters_from([("level", vec!["ERROR"])]), "");
        query(&mut s, &mut b, FieldFilters::new(), "");
        assert_eq!(ordinals(&s.window(&mut a, 0, 10)), vec![1, 3, 5]);
        assert_eq!(s.window(&mut b, 0, 10).rows.len(), 5);
    }

    #[test]
    fn range_counts_and_timestampless_exclusion() {
        let mut s = session(&format!(
            "{JSONL}{}\n",
            r#"{"level":"ERROR","msg":"no time"}"#
        ));
        let mut v = View::new();
        let start = DateTime::parse_from_rfc3339("2026-08-09T10:00:01Z")
            .unwrap()
            .timestamp_millis();
        let r = s.set_query(
            &mut v,
            QueryInput {
                filters: filters_from([("level", vec!["ERROR"])]),
                range: Some(TimeRangeDto {
                    start_ms: start,
                    end_ms: start + 10_000,
                }),
                ..Default::default()
            },
        );
        assert_eq!(r.matched, 2);
        assert_eq!(r.timestampless_excluded, 1);
    }

    #[test]
    fn a_collapsed_chain_is_one_row_with_its_structure() {
        let mut s = session(TRACE);
        let mut v = View::new();
        let w = s.window(&mut v, 0, 10);
        assert_eq!(ordinals(&w), vec![1, 5]);
        assert_eq!(w.row_count, 2);
        assert_eq!(w.matched, 5);
        let root = &w.rows[0];
        assert_eq!(
            (root.chain_size, root.expanded, root.continuation_of),
            (4, false, None)
        );

        assert_eq!(s.locate(&mut v, 5), 1);
        assert_eq!(s.locate(&mut v, 3), 1);
        assert_eq!(s.toggle_chain(&mut v, 1), 5);
        let w = s.window(&mut v, 0, 10);
        assert_eq!(ordinals(&w), vec![1, 2, 3, 4, 5]);
        assert!(w.rows[0].expanded);
        assert!(w.rows[2].chain_member);
        assert_eq!(w.rows[2].continuation_of, Some(1));
    }

    #[test]
    fn search_expands_a_chain_and_a_toggle_collapses_it() {
        let mut s = session(TRACE);
        let mut v = View::new();
        let r = query(&mut s, &mut v, FieldFilters::new(), "iobridge");
        assert_eq!((r.matched, r.row_count), (4, 4));
        let w = s.window(&mut v, 0, 10);
        assert_eq!(w.rows[2].match_ranges, vec![(16, 24), (33, 41)]);
        assert_eq!(s.toggle_chain(&mut v, 1), 1);
    }

    #[test]
    fn entry_returns_the_raw_line_or_reports_eviction() {
        let mut s = Session::new(SessionConfig {
            max_lines: Some(2),
            ..Default::default()
        });
        let mut v = View::new();
        s.ingest(b"a\nb\nc\n");
        s.force_detection();
        assert_eq!(s.entry(&mut v, 3).raw.as_deref(), Some("c"));
        let gone = s.entry(&mut v, 1);
        assert!(gone.row.is_none() && gone.raw.is_none());
    }

    #[test]
    fn eviction_updates_counts_and_ordinals() {
        let mut s = Session::new(SessionConfig {
            max_lines: Some(100),
            ..Default::default()
        });
        let mut v = View::new();
        for i in 0..150 {
            s.ingest(format!("line {i}\n").as_bytes());
        }
        assert_eq!(s.evicted_lines(), 50);
        let w = s.window(&mut v, 0, 1);
        assert_eq!(w.rows[0].ordinal, 51);
        assert_eq!(w.total, 100);
        assert_eq!(s.stats().total_lines, 150);
    }

    #[test]
    fn eviction_applies_to_a_filtered_view_and_its_range() {
        let mut s = Session::new(SessionConfig {
            max_lines: Some(3),
            ..Default::default()
        });
        let mut v = View::new();
        s.ingest(JSONL.as_bytes());
        s.force_detection();
        query(&mut s, &mut v, filters_from([("level", vec!["ERROR"])]), "");
        assert_eq!(ordinals(&s.window(&mut v, 0, 10)), vec![3, 5]);
        s.ingest(b"{\"ts\":\"2026-08-09T10:00:05Z\",\"level\":\"ERROR\",\"msg\":\"x\"}\n");
        assert_eq!(ordinals(&s.window(&mut v, 0, 10)), vec![5, 6]);
        let b = s.buckets(&mut v, None, true);
        assert_eq!(b.filtered.iter().sum::<u64>(), 2);
        assert_eq!(b.total.iter().sum::<u64>(), 3);
    }

    #[test]
    fn detection_holds_until_forced() {
        let mut s = Session::new(SessionConfig::default());
        let mut v = View::new();
        s.ingest(b"{\"level\":\"INFO\",\"msg\":\"a\"}\n{\"level\":\"INFO\",\"msg\":\"b\"}\n");
        assert!(s.is_detecting());
        assert_eq!(s.window(&mut v, 0, 10).rows.len(), 0);
        s.force_detection();
        assert_eq!(s.stats().format.as_deref(), Some("json"));
        assert_eq!(s.window(&mut v, 0, 10).rows.len(), 2);
    }

    #[test]
    fn format_override_reparses_retained_lines() {
        let mut s = session("starting worker=a retries=3\nstopping worker=b retries=4\n");
        let plain = s.set_format(Some(Format::Plain), None).unwrap();
        assert!(!plain.field_inventory.fields.contains_key("worker"));
        let stats = s.set_format(Some(Format::Logfmt), None).unwrap();
        assert_eq!(stats.format.as_deref(), Some("logfmt"));
        assert!(stats.field_inventory.fields.contains_key("worker"));
        let mut v = View::new();
        let r = query(&mut s, &mut v, filters_from([("worker", vec!["b"])]), "");
        assert_eq!(r.matched, 1);
    }

    #[test]
    fn a_reparse_after_eviction_keeps_ordinals() {
        let mut s = Session::new(SessionConfig {
            max_lines: Some(2),
            ..Default::default()
        });
        s.ingest(b"a=1\nb=2\nc=3\n");
        s.force_detection();
        s.set_format(Some(Format::Plain), None).unwrap();
        let mut v = View::new();
        assert_eq!(ordinals(&s.window(&mut v, 0, 10)), vec![2, 3]);
    }

    #[test]
    fn timezone_override_applies_to_offsetless_timestamps() {
        let mut s = session("{\"ts\":\"2026-08-09T10:00:00\",\"msg\":\"a\"}\n");
        s.set_format(None, Some(120)).unwrap();
        let mut v = View::new();
        let w = s.window(&mut v, 0, 1);
        assert_eq!(
            w.rows[0].timestamp.as_deref(),
            Some("2026-08-09T08:00:00+00:00")
        );
        assert!(s.set_format(None, Some(100_000)).is_err());
    }

    #[test]
    fn a_view_rebuilds_after_a_reparse() {
        let mut s = session("starting worker=a\nstopping worker=b\n");
        let mut v = View::new();
        query(&mut s, &mut v, FieldFilters::new(), "stopping");
        assert_eq!(s.window(&mut v, 0, 10).rows.len(), 1);
        s.set_format(Some(Format::Logfmt), None).unwrap();
        assert_eq!(s.counts(&mut v).0, 1);
    }

    #[test]
    fn buckets_reflect_filters() {
        let mut s = session(JSONL);
        let mut v = View::new();
        query(
            &mut s,
            &mut v,
            filters_from([("level", vec!["INFO", "WARN"])]),
            "",
        );
        let b = s.buckets(&mut v, None, false);
        assert_eq!(b.bucket_ms, 1000);
        assert_eq!(b.total.iter().sum::<u64>(), 5);
        assert_eq!(b.filtered.iter().sum::<u64>(), 2);
    }
}
