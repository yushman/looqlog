//! Time-ordered index over parsed entries (`entry-index` spec, ADR-0010). Ported from
//! the former `web/src/entry-index.ts`.
//!
//! Entries stay in input order. The parser only ever increments `Entry::ordinal` while
//! emitting entries in input order, so ordinals are strictly increasing along the store:
//! lookup by ordinal is a binary search, and because eviction only ever removes from the
//! front, "still retained" is simply `ordinal >= first retained ordinal` — no tombstone
//! set is needed.
//!
//! `sorted` holds only timestamped entries, ordered by timestamp, so range queries and
//! bucket counts do not scan every entry. Timestampless entries never enter it — they
//! are counted, never dropped, and never given a substitute time.
//!
//! The index carries no predicate. Every range/bucket method takes a `keep` closure, so
//! whatever owns a match set (`query::MatchSet`) composes with eviction for free.

use std::collections::{BTreeMap, HashMap, VecDeque};

use crate::entry::{Entry, FieldValue};
use crate::level::Level;

/// Evicted refs are left in `sorted` and filtered lazily; the vector is compacted once
/// they are both a meaningful count and a meaningful fraction of it, so a slow trickle
/// of evictions does not compact on every call.
const COMPACT_MIN_DEAD: usize = 1024;
const COMPACT_RATIO: f64 = 0.25;

/// Calibrated against `mem_probe -- session` (docs/devlog.md): the fixed cost of one
/// retained entry (the `Entry` itself, its index refs, allocator slack) and of a non-empty
/// field map (its first B-tree node).
const ENTRY_BYTES: usize = 112;
const FIELD_MAP_BYTES: usize = 640;
const FIELD_BYTES: usize = 16;

/// Estimated heap retained by one entry.
fn entry_bytes(entry: &Entry) -> usize {
    let fields: usize = entry
        .fields
        .iter()
        .map(|(key, value)| {
            FIELD_BYTES
                + key.len()
                + match value {
                    FieldValue::String(s) | FieldValue::Number(s) | FieldValue::Json(s) => s.len(),
                    FieldValue::Bool(_) | FieldValue::Null => 0,
                }
        })
        .sum();
    let map = if entry.fields.is_empty() {
        0
    } else {
        FIELD_MAP_BYTES
    };
    ENTRY_BYTES + entry.message.len() + map + fields
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RobustSpan {
    pub min_ms: i64,
    pub max_ms: i64,
    /// Timestamped entries outside `[min_ms, max_ms]` — kept discoverable, not hidden.
    pub outlier_count: usize,
}

#[derive(Debug, Default)]
pub struct EntryIndex {
    entries: VecDeque<Entry>,
    /// `(timestamp ms, ordinal)`, sorted by timestamp. May hold evicted refs between
    /// compactions.
    sorted: Vec<(i64, usize)>,
    timestamped_live: usize,
    timestampless: usize,
    /// Chain root ordinal → its member ordinals, in input order. A member whose root
    /// has been evicted keeps its `continuation_of`, and is treated as standalone.
    chain_members: HashMap<usize, Vec<usize>>,
    level_counts: BTreeMap<Level, usize>,
    /// Sum of [`entry_bytes`] over retained entries.
    bytes: usize,
}

/// First index in `sorted` whose timestamp is `>= ms`.
fn lower_bound(sorted: &[(i64, usize)], ms: i64) -> usize {
    sorted.partition_point(|(ts, _)| *ts < ms)
}

/// First index in `sorted` whose timestamp is `> ms`.
fn upper_bound(sorted: &[(i64, usize)], ms: i64) -> usize {
    sorted.partition_point(|(ts, _)| *ts <= ms)
}

impl EntryIndex {
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends one entry. Callers must append in input order (strictly increasing
    /// ordinals), which is what `Parser` emits.
    pub fn append(&mut self, entry: Entry) {
        debug_assert!(self
            .entries
            .back()
            .map_or(true, |last| last.ordinal < entry.ordinal));
        if let Some(level) = entry.level {
            *self.level_counts.entry(level).or_insert(0) += 1;
        }
        if let Some(root) = entry.continuation_of {
            self.chain_members
                .entry(root)
                .or_default()
                .push(entry.ordinal);
        }
        match entry.timestamp {
            Some(ts) => {
                let ms = ts.timestamp_millis();
                let r = (ms, entry.ordinal);
                if self.sorted.last().map_or(true, |last| ms >= last.0) {
                    // Near-sorted fast path: real logs arrive close to timestamp order.
                    self.sorted.push(r);
                } else {
                    let at = lower_bound(&self.sorted, ms);
                    self.sorted.insert(at, r);
                }
                self.timestamped_live += 1;
            }
            None => self.timestampless += 1,
        }
        self.bytes += entry_bytes(&entry);
        self.entries.push_back(entry);
    }

    /// Evicts the oldest `count` entries in input order. Returns the evicted ordinals.
    pub fn evict_front(&mut self, count: usize) -> Vec<usize> {
        let mut evicted = Vec::with_capacity(count.min(self.entries.len()));
        for _ in 0..count {
            let Some(entry) = self.entries.pop_front() else {
                break;
            };
            if let Some(level) = entry.level {
                if let Some(c) = self.level_counts.get_mut(&level) {
                    *c -= 1;
                    if *c == 0 {
                        self.level_counts.remove(&level);
                    }
                }
            }
            // A root's member list goes with it; the members survive as orphans. A
            // member evicted while its root is still retained leaves that root's list.
            self.chain_members.remove(&entry.ordinal);
            if let Some(root) = entry.continuation_of {
                if let Some(siblings) = self.chain_members.get_mut(&root) {
                    siblings.retain(|&o| o != entry.ordinal);
                }
            }
            if entry.timestamp.is_some() {
                self.timestamped_live -= 1;
            } else {
                self.timestampless -= 1;
            }
            self.bytes -= entry_bytes(&entry);
            evicted.push(entry.ordinal);
        }
        let dead = self.sorted.len() - self.timestamped_live;
        if dead >= COMPACT_MIN_DEAD && dead as f64 >= self.sorted.len() as f64 * COMPACT_RATIO {
            self.compact();
        }
        evicted
    }

    /// Evicts every entry whose ordinal is below `ordinal`. Returns how many.
    pub fn evict_before(&mut self, ordinal: usize) -> usize {
        let count = self.entries.partition_point(|e| e.ordinal < ordinal);
        if count > 0 {
            self.evict_front(count);
        }
        count
    }

    fn compact(&mut self) {
        if self.sorted.len() == self.timestamped_live {
            return;
        }
        let first = self.first_ordinal();
        self.sorted.retain(|&(_, o)| first.is_some_and(|f| o >= f));
    }

    fn first_ordinal(&self) -> Option<usize> {
        self.entries.front().map(|e| e.ordinal)
    }

    fn is_live(&self, ordinal: usize) -> bool {
        self.first_ordinal().is_some_and(|f| ordinal >= f)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Estimated heap retained by the entries.
    pub fn retained_bytes(&self) -> usize {
        self.bytes
    }

    pub fn timestampless_count(&self) -> usize {
        self.timestampless
    }

    pub fn timestamped_count(&self) -> usize {
        self.timestamped_live
    }

    pub fn level_counts(&self) -> &BTreeMap<Level, usize> {
        &self.level_counts
    }

    /// Position of `ordinal` in input order, if retained.
    pub fn position(&self, ordinal: usize) -> Option<usize> {
        self.entries
            .binary_search_by_key(&ordinal, |e| e.ordinal)
            .ok()
    }

    pub fn get(&self, ordinal: usize) -> Option<&Entry> {
        self.position(ordinal).map(|p| &self.entries[p])
    }

    pub fn at(&self, position: usize) -> Option<&Entry> {
        self.entries.get(position)
    }

    /// Retained entries in input order.
    pub fn iter(&self) -> impl Iterator<Item = &Entry> {
        self.entries.iter()
    }

    /// Retained entries from input-order position `from` onwards.
    pub fn iter_from(&self, from: usize) -> impl Iterator<Item = &Entry> {
        self.entries.range(from.min(self.entries.len())..)
    }

    /// The root of `entry`'s chain, or `None` when it stands on its own — which
    /// includes a member whose root has been evicted.
    pub fn root_of(&self, entry: &Entry) -> Option<usize> {
        entry.continuation_of.filter(|&root| self.is_live(root))
    }

    /// Whether `ordinal` continues a chain whose root is still retained. An orphaned
    /// member counts as its own event, because that is how it renders.
    pub fn is_chain_member(&self, ordinal: usize) -> bool {
        self.get(ordinal).is_some_and(|e| self.root_of(e).is_some())
    }

    /// Member ordinals of the chain rooted at `ordinal`, in input order.
    pub fn members_of(&self, ordinal: usize) -> &[usize] {
        self.chain_members
            .get(&ordinal)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    /// Ordinals of retained timestamped entries in the half-open range
    /// `[start_ms, end_ms)` that pass `keep`, in input order. An entry exactly at
    /// `start_ms` is included and one exactly at `end_ms` is not, so adjacent ranges
    /// never share a boundary entry.
    pub fn range_ordinals(
        &self,
        start_ms: i64,
        end_ms: i64,
        keep: impl Fn(usize) -> bool,
    ) -> Vec<usize> {
        let mut out = self.range_refs(start_ms, end_ms, keep);
        out.sort_unstable();
        out
    }

    pub fn count_in_range(
        &self,
        start_ms: i64,
        end_ms: i64,
        keep: impl Fn(usize) -> bool,
    ) -> usize {
        self.range_refs(start_ms, end_ms, keep).len()
    }

    fn range_refs(&self, start_ms: i64, end_ms: i64, keep: impl Fn(usize) -> bool) -> Vec<usize> {
        let lo = lower_bound(&self.sorted, start_ms);
        let hi = lower_bound(&self.sorted, end_ms).max(lo);
        let first = self.first_ordinal().unwrap_or(usize::MAX);
        self.sorted[lo..hi]
            .iter()
            .map(|&(_, o)| o)
            .filter(|&o| o >= first && keep(o))
            .collect()
    }

    /// Counts **events**, not lines: an entry continuing a retained root is not
    /// counted, so a sixty-frame exception is one point on the timeline. Counts
    /// `bucket_count` buckets of width `bucket_ms` from `start_ms`, for entries that
    /// pass `keep`.
    pub fn bucket_counts(
        &self,
        start_ms: i64,
        bucket_ms: i64,
        bucket_count: usize,
        keep: impl Fn(usize) -> bool,
    ) -> Vec<u64> {
        let mut counts = vec![0u64; bucket_count];
        if bucket_count == 0 || bucket_ms <= 0 {
            return counts;
        }
        let end_ms = start_ms.saturating_add(bucket_ms.saturating_mul(bucket_count as i64));
        let lo = lower_bound(&self.sorted, start_ms);
        let hi = lower_bound(&self.sorted, end_ms).max(lo);
        let first = self.first_ordinal().unwrap_or(usize::MAX);
        for &(ts, o) in &self.sorted[lo..hi] {
            if o < first || !keep(o) || self.is_chain_member(o) {
                continue;
            }
            let idx = ((ts - start_ms) / bucket_ms).clamp(0, bucket_count as i64 - 1) as usize;
            counts[idx] += 1;
        }
        counts
    }

    /// A robust default span: Tukey's fence (1.5x IQR beyond the 25th/75th
    /// percentile) rather than min/max, so a single absurd timestamp cannot compress
    /// every real entry into one pixel. `None` when nothing is timestamped.
    pub fn robust_span(&mut self) -> Option<RobustSpan> {
        self.compact();
        let s = &self.sorted;
        let n = s.len();
        if n == 0 {
            return None;
        }
        if n == 1 {
            return Some(RobustSpan {
                min_ms: s[0].0,
                max_ms: s[0].0,
                outlier_count: 0,
            });
        }
        let q1 = s[n / 4].0;
        let q3 = s[(n * 3 / 4).min(n - 1)].0;
        let iqr = q3 - q1;
        // A zero IQR means half the dataset shares one instant: pad by a fixed 1s.
        let pad = if iqr == 0 { 1000 } else { iqr + iqr / 2 };
        let idx_low = lower_bound(s, q1 - pad);
        let idx_high = upper_bound(s, q3 + pad);
        Some(RobustSpan {
            min_ms: s[idx_low.min(n - 1)].0,
            max_ms: s[idx_high.saturating_sub(1)].0,
            outlier_count: n - (idx_high - idx_low),
        })
    }

    /// Min/max over retained timestamped entries, with no outlier trimming.
    pub fn full_span(&self) -> Option<(i64, i64)> {
        let first = self.first_ordinal()?;
        let min = self.sorted.iter().find(|&&(_, o)| o >= first)?.0;
        let max = self.sorted.iter().rev().find(|&&(_, o)| o >= first)?.0;
        Some((min, max))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::DateTime;

    fn mk(ordinal: usize, ts_ms: Option<i64>) -> Entry {
        Entry {
            ordinal,
            timestamp: ts_ms.map(|ms| DateTime::from_timestamp_millis(ms).unwrap()),
            timestamp_used_default_tz: false,
            timestamp_year_inferred: false,
            level: None,
            message: format!("m{ordinal}"),
            fields: Default::default(),
            continuation_of: None,
        }
    }

    fn leveled(ordinal: usize, ts_ms: i64, level: Option<Level>) -> Entry {
        Entry {
            level,
            ..mk(ordinal, Some(ts_ms))
        }
    }

    fn member(ordinal: usize, ts_ms: i64, root: usize) -> Entry {
        Entry {
            continuation_of: Some(root),
            ..mk(ordinal, Some(ts_ms))
        }
    }

    fn all(_: usize) -> bool {
        true
    }

    fn index(entries: Vec<Entry>) -> EntryIndex {
        let mut idx = EntryIndex::new();
        for e in entries {
            idx.append(e);
        }
        idx
    }

    #[test]
    fn range_returns_every_entry_regardless_of_input_position() {
        let idx = index(vec![
            mk(1, Some(5000)),
            mk(2, Some(1000)),
            mk(3, Some(4000)),
            mk(4, Some(2000)),
            mk(5, Some(0)),
        ]);
        assert_eq!(idx.range_ordinals(1000, 5000, all), vec![2, 3, 4]);
        assert_eq!(idx.count_in_range(1000, 5000, all), 3);
    }

    #[test]
    fn stays_correct_across_many_out_of_order_appends_and_an_eviction() {
        let n = 2000;
        let idx_entries = (0..n)
            .map(|i| mk(i + 1, Some(((n - i) * 1000) as i64)))
            .collect();
        let mut idx = index(idx_entries);
        assert_eq!(idx.len(), n);
        assert_eq!(idx.timestamped_count(), n);
        let evicted = idx.evict_front(500);
        assert_eq!(evicted.len(), 500);
        assert_eq!(idx.len(), n - 500);
        let full = idx.range_ordinals(0, ((n + 1) * 1000) as i64, all);
        assert_eq!(full.len(), n - 500);
        assert!(full.iter().all(|o| !evicted.contains(o)));
        assert!(full.windows(2).all(|w| w[0] < w[1]));
    }

    #[test]
    fn timestampless_entries_are_held_apart_and_counted() {
        let idx = index(vec![
            mk(1, Some(0)),
            mk(2, None),
            mk(3, Some(1000)),
            mk(4, None),
        ]);
        assert_eq!(idx.timestampless_count(), 2);
        assert_eq!(idx.timestamped_count(), 2);
        assert_eq!(idx.len(), 4);
        assert_eq!(idx.range_ordinals(i64::MIN, i64::MAX, all), vec![1, 3]);
    }

    #[test]
    fn front_eviction_leaves_no_stale_references() {
        let mut idx = index(vec![
            mk(1, Some(0)),
            mk(2, Some(1000)),
            mk(3, Some(2000)),
            mk(4, None),
        ]);
        idx.evict_front(2);
        assert_eq!(idx.len(), 2);
        assert_eq!(idx.range_ordinals(0, 3000, all), vec![3]);
        assert_eq!(idx.bucket_counts(0, 1000, 3, all), vec![0, 0, 1]);
    }

    #[test]
    fn eviction_also_takes_timestampless_entries() {
        let mut idx = index(vec![mk(1, None), mk(2, Some(0))]);
        idx.evict_front(1);
        assert_eq!(idx.timestampless_count(), 0);
        assert_eq!(idx.len(), 1);
    }

    #[test]
    fn many_small_evictions_forcing_compaction_stay_consistent() {
        let n = 5000;
        let mut idx = index((0..n).map(|i| mk(i + 1, Some(i as i64 * 1000))).collect());
        for _ in 0..n - 10 {
            idx.evict_front(1);
        }
        assert_eq!(idx.len(), 10);
        assert_eq!(
            idx.range_ordinals(0, n as i64 * 1000, all),
            ((n - 9)..=n).collect::<Vec<_>>()
        );
    }

    #[test]
    fn half_open_boundary_is_applied_consistently() {
        let idx = index(vec![mk(1, Some(0)), mk(2, Some(1000)), mk(3, Some(2000))]);
        assert_eq!(idx.range_ordinals(0, 1000, all), vec![1]);
        assert_eq!(idx.range_ordinals(1000, 2000, all), vec![2]);
    }

    #[test]
    fn empty_range_is_an_empty_result() {
        let idx = index(vec![mk(1, Some(0))]);
        assert!(idx.range_ordinals(5000, 6000, all).is_empty());
        assert_eq!(idx.count_in_range(5000, 6000, all), 0);
    }

    #[test]
    fn bucket_counts_match_a_manual_count() {
        let idx = index(vec![
            mk(1, Some(500)),
            mk(2, Some(900)),
            mk(3, Some(1500)),
            mk(4, Some(3200)),
            mk(5, None),
        ]);
        assert_eq!(idx.bucket_counts(0, 1000, 4, all), vec![2, 1, 0, 1]);
    }

    #[test]
    fn keep_filters_ranges_and_buckets_consistently() {
        let idx = index(vec![
            leveled(1, 0, Some(Level::Error)),
            leveled(2, 1000, Some(Level::Info)),
            leveled(3, 2000, Some(Level::Error)),
        ]);
        let keep = |o: usize| o != 2;
        assert_eq!(idx.range_ordinals(0, 3000, keep), vec![1, 3]);
        assert_eq!(idx.bucket_counts(0, 1000, 3, keep), vec![1, 0, 1]);
        assert_eq!(idx.bucket_counts(0, 1000, 3, all), vec![1, 1, 1]);
    }

    #[test]
    fn robust_span_excludes_an_epoch_zero_outlier_and_reports_it() {
        let base = 1_786_276_800_000i64; // 2026-08-09T12:00:00Z
        let mut entries: Vec<Entry> = (0..200)
            .map(|i| mk(i + 1, Some(base + i as i64 * 1000)))
            .collect();
        entries.push(mk(201, Some(0)));
        let mut idx = index(entries);
        let span = idx.robust_span().unwrap();
        assert!(span.min_ms > base - 1000);
        assert!(span.max_ms <= base + 200 * 1000);
        assert_eq!(span.outlier_count, 1);
        assert_eq!(idx.full_span().unwrap().0, 0);
    }

    #[test]
    fn spans_are_none_without_timestamps() {
        let mut idx = index(vec![mk(1, None)]);
        assert!(idx.robust_span().is_none());
        assert!(idx.full_span().is_none());
    }

    #[test]
    fn level_counts_track_eviction() {
        let mut idx = index(vec![
            leveled(1, 0, Some(Level::Error)),
            leveled(2, 1000, Some(Level::Error)),
            leveled(3, 2000, Some(Level::Info)),
            leveled(4, 3000, None),
        ]);
        assert_eq!(idx.level_counts().get(&Level::Error), Some(&2));
        assert_eq!(idx.level_counts().get(&Level::Info), Some(&1));
        idx.evict_front(1);
        assert_eq!(idx.level_counts().get(&Level::Error), Some(&1));
        idx.evict_front(1);
        assert!(!idx.level_counts().contains_key(&Level::Error));
    }

    #[test]
    fn a_chain_counts_once_per_bucket() {
        let idx = index(vec![
            leveled(1, 0, Some(Level::Error)),
            member(2, 1, 1),
            member(3, 2, 1),
            member(4, 3, 1),
            leveled(5, 4, Some(Level::Info)),
        ]);
        assert_eq!(idx.len(), 5);
        assert_eq!(idx.bucket_counts(0, 1000, 1, all), vec![2]);
    }

    #[test]
    fn full_span_covers_chain_members() {
        let idx = index(vec![leveled(1, 0, Some(Level::Error)), member(2, 5000, 1)]);
        assert_eq!(idx.full_span(), Some((0, 5000)));
    }

    #[test]
    fn a_member_orphaned_by_eviction_is_its_own_event() {
        let mut idx = index(vec![
            leveled(1, 0, Some(Level::Error)),
            member(2, 1000, 1),
            member(3, 2000, 1),
        ]);
        idx.evict_front(1);
        assert_eq!(
            idx.iter().map(|e| e.ordinal).collect::<Vec<_>>(),
            vec![2, 3]
        );
        assert_eq!(idx.bucket_counts(0, 10_000, 1, all), vec![2]);
        assert!(idx.root_of(idx.get(2).unwrap()).is_none());
    }

    #[test]
    fn evicting_a_member_leaves_the_roots_list() {
        let mut idx = index(vec![
            leveled(1, 0, Some(Level::Error)),
            member(2, 1000, 1),
            member(3, 2000, 1),
        ]);
        assert_eq!(idx.members_of(1), &[2, 3]);
        idx.evict_front(2);
        assert!(idx.members_of(1).is_empty());
    }
}
