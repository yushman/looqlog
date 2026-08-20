// Time-ordered index over parsed entries (`entry-index` spec, design.md D1/D2/D4/D5).
//
// Entries stay in input order in a `Map` keyed by the parser-assigned `ordinal`
// (`Entry.ordinal`, `crates/looqlog-core/src/entry.rs`) — a stable identity, and,
// because the parser only ever increments it while emitting entries in input
// order, also a correct proxy for input order: sorting ordinals numerically
// recovers input order without a second array to track it (D2). `Map` iteration
// order is insertion order for whatever keys are still present, so it already
// gives input-order iteration and O(1) eviction-by-oldest for free.
//
// A second structure, `sorted`, holds only the ordinals of *timestamped* entries,
// ordered by timestamp, so range queries and bucket counts don't scan every entry.
// Timestampless entries (D5) live in their own set and never enter `sorted` — they
// are counted, never dropped, and never given a substitute time.
//
// This lives in TypeScript rather than `looqlog-core` (D1): it is a view structure,
// consulted on every drag frame from the main thread, and it must survive
// eviction. Revisit condition (measured in docs/devlog.md, task 1.6): move it into
// the core crate if profiling shows index maintenance, not rendering, dominating
// at target dataset sizes.

import type { ChainAwarePredicate } from "./predicate";
import type { EntryDto } from "./wasm-types";

interface TimestampedRef {
  ordinal: number;
  tsMs: number;
}

export interface RangeResult {
  entries: EntryDto[];
  count: number;
}

export interface RobustSpan {
  minMs: number;
  maxMs: number;
  /** Timestamped entries outside [minMs, maxMs] — the outliers D4 asks to keep
   * discoverable rather than hidden. */
  outlierCount: number;
}

// Tombstone compaction (`evictFront`): removing an arbitrary element from `sorted`
// is O(n), so eviction instead marks the ordinal as a tombstone (O(1)) and defers
// the O(n) filter to a periodic compaction — amortized flat cost per evicted
// entry, which is what task 1.3/1.4 asks for. Compaction only runs once
// tombstones are both a meaningful count and a meaningful fraction of `sorted`, so
// a slow trickle of evictions doesn't compact on every call.
const COMPACT_MIN_TOMBSTONES = 1024;
const COMPACT_RATIO = 0.25;

/** First index in `sorted` whose `tsMs` is >= `tsMs`. */
function lowerBound(sorted: readonly TimestampedRef[], tsMs: number): number {
  let lo = 0;
  let hi = sorted.length;
  while (lo < hi) {
    const mid = (lo + hi) >>> 1;
    // Safe: binary search invariant keeps mid within [0, sorted.length).
    if (sorted[mid]!.tsMs < tsMs) {
      lo = mid + 1;
    } else {
      hi = mid;
    }
  }
  return lo;
}

/** First index in `sorted` whose `tsMs` is > `tsMs`. */
function upperBound(sorted: readonly TimestampedRef[], tsMs: number): number {
  let lo = 0;
  let hi = sorted.length;
  while (lo < hi) {
    const mid = (lo + hi) >>> 1;
    if (sorted[mid]!.tsMs <= tsMs) {
      lo = mid + 1;
    } else {
      hi = mid;
    }
  }
  return lo;
}

export class EntryIndex {
  private entries = new Map<number, EntryDto>();
  private sorted: TimestampedRef[] = [];
  private tombstones = new Set<number>();
  private timestamplessOrdinals = new Set<number>();

  // ---- filtering-and-search additions ------------------------------------
  //
  // `predicate` is field-filters ∧ query only (design.md D2/D9) — deliberately
  // NOT the time range, which every range/bucket method below already answers
  // in O(log n + k) via `sorted`. Folding range into `predicate` would mean
  // re-testing every entry per range drag instead of reusing that binary search.
  //
  // `matchingOrdinals` mirrors `tombstones`'s shape: a sparse set of exceptions
  // consulted wherever `sorted` is walked, rather than a second parallel array to
  // keep in sync. It is populated only when a predicate is active (D9's cheap
  // path — the common "no filter" case pays nothing beyond a null check) and
  // rebuilt in full only when the predicate itself changes (D9's expensive path,
  // the one the 50ms target in `filtering-and-search`'s `filtering` spec
  // measures); a newly arriving entry is tested once against the current
  // predicate in `append`, never against the whole retained set.
  private predicate: ChainAwarePredicate | null = null;
  private matchingOrdinals = new Set<number>();

  /** Chain root ordinal → its member ordinals, in input order
   * (`multiline-entry-continuations`). The reverse direction needs no structure of
   * its own: `EntryDto.continuationOf` already names the root, which is exactly why
   * the parser stores the root rather than the predecessor (design D2).
   *
   * A member whose root has been evicted keeps a `continuationOf` pointing at an
   * ordinal that is no longer here. That is not an error state: it renders and filters
   * as an ordinary standalone entry, which is what `rootOf` returning `null` means. */
  private chainMembers = new Map<number, number[]>();

  /** Distinct `level` values and their live counts, maintained incrementally
   * across append/evict — chips need this (`filtering` spec, "Filter chips come
   * from the field inventory") but `level` never enters the parser's field
   * inventory (`predicate.ts`'s `fieldDisplayValue` doc comment), so nothing else
   * in this codebase already tracks it. */
  private levelCounts = new Map<string, number>();

  /** Appends entries in input order. Safe to call repeatedly (live append) or
   * once with a whole parse result (file mode). */
  append(newEntries: readonly EntryDto[]): void {
    for (const e of newEntries) {
      this.entries.set(e.ordinal, e);
      if (e.level !== null) {
        this.levelCounts.set(e.level, (this.levelCounts.get(e.level) ?? 0) + 1);
      }
      // Registered before the predicate runs: a member arriving into an already-open
      // chain can flip that chain from hidden to shown (a search matching the frame
      // that just landed), and `chainMatches` has to be able to see it.
      if (e.continuationOf !== null) {
        const members = this.chainMembers.get(e.continuationOf);
        if (members === undefined) {
          this.chainMembers.set(e.continuationOf, [e.ordinal]);
        } else {
          members.push(e.ordinal);
        }
      }
      if (this.predicate !== null) {
        this.evaluateOnArrival(e);
      }
      if (e.timestamp === null) {
        this.timestamplessOrdinals.add(e.ordinal);
        continue;
      }
      const tsMs = Date.parse(e.timestamp);
      if (Number.isNaN(tsMs)) {
        // Defensive only: the DTO contract (wasm-types.ts) says `timestamp` is
        // RFC 3339 or `null`. An unparsable non-null string would be a
        // wasm-bridge bug, not a reason to crash the tab — hold it apart the same
        // way a genuinely timestampless entry is held apart (D5's "never invent a
        // time" cuts both ways: never invent one, and never trust a broken one).
        this.timestamplessOrdinals.add(e.ordinal);
        continue;
      }
      this.insertSorted({ ordinal: e.ordinal, tsMs });
    }
  }

  /** Recomputes which retained entries match `pred` (field filters ∧ query,
   * never the range — see the class-level comment). `null` clears filtering
   * entirely. This is D9's "expensive path": O(retained entries), paid once per
   * filter change rather than per arriving entry. */
  setPredicate(pred: ((e: EntryDto) => boolean) | ChainAwarePredicate | null): void {
    // A bare function stays supported and means "one rule for everything": it becomes
    // the root half, with a member half that matches anything, so a chain is shown iff
    // its root is.
    this.predicate =
      typeof pred === "function" ? { root: pred, member: () => true } : pred;
    this.matchingOrdinals.clear();
    if (this.predicate === null) {
      return;
    }
    for (const e of this.entries.values()) {
      if (this.rootOf(e) !== null) {
        continue; // decided as part of its chain, below
      }
      const members = this.chainMembers.get(e.ordinal);
      if (members === undefined || members.length === 0) {
        if (this.predicate.root(e) && this.predicate.member(e)) {
          this.matchingOrdinals.add(e.ordinal);
        }
        continue;
      }
      this.applyChainMatch(e.ordinal, this.chainMatches(e.ordinal));
    }
  }

  /** The root of `entry`'s chain, or `null` when it stands on its own — which
   * includes a member whose root has been evicted (design: an orphan is an ordinary
   * entry, not a failed lookup). */
  private rootOf(entry: EntryDto): number | null {
    const root = entry.continuationOf;
    return root !== null && this.entries.has(root) ? root : null;
  }

  /** Whether the chain rooted at `rootOrdinal` is shown: its **root** must pass the
   * field filters, and the query must match the root or any one member (design D9). */
  private chainMatches(rootOrdinal: number): boolean {
    const predicate = this.predicate;
    const root = this.entries.get(rootOrdinal);
    if (predicate === null || root === undefined || !predicate.root(root)) {
      return false;
    }
    if (predicate.member(root)) {
      return true;
    }
    for (const ordinal of this.chainMembers.get(rootOrdinal) ?? []) {
      const member = this.entries.get(ordinal);
      if (member !== undefined && predicate.member(member)) {
        return true;
      }
    }
    return false;
  }

  /** Shows or hides a whole chain at once — never a member without its root, never a
   * root with its members omitted (`filtering` spec). */
  private applyChainMatch(rootOrdinal: number, matched: boolean): void {
    const ordinals = [rootOrdinal, ...(this.chainMembers.get(rootOrdinal) ?? [])];
    for (const ordinal of ordinals) {
      if (matched) {
        this.matchingOrdinals.add(ordinal);
      } else {
        this.matchingOrdinals.delete(ordinal);
      }
    }
  }

  /** One arriving entry against the active predicate (the cheap path — the whole
   * retained set is only ever re-tested by `setPredicate`). */
  private evaluateOnArrival(entry: EntryDto): void {
    const predicate = this.predicate;
    if (predicate === null) {
      return;
    }
    const root = this.rootOf(entry);
    if (root === null) {
      if (predicate.root(entry) && predicate.member(entry)) {
        this.matchingOrdinals.add(entry.ordinal);
      }
      return;
    }
    if (this.matchingOrdinals.has(root)) {
      this.matchingOrdinals.add(entry.ordinal); // chain already shown
    } else if (this.chainMatches(root)) {
      this.applyChainMatch(root, true);
    }
  }

  /** Whether `ordinal` continues a chain whose root is still retained — the test the
   * timeline uses to count events rather than lines (`timeline` spec, design D9). An
   * orphaned member counts as its own event, because that is how it renders. */
  private isChainMember(ordinal: number): boolean {
    const entry = this.entries.get(ordinal);
    return entry !== undefined && this.rootOf(entry) !== null;
  }

  /** Member ordinals of the chain rooted at `ordinal`, in input order; empty for an
   * entry that roots no chain. */
  membersOf(ordinal: number): readonly number[] {
    return this.chainMembers.get(ordinal) ?? [];
  }

  get hasActivePredicate(): boolean {
    return this.predicate !== null;
  }

  /** An ordinal excluded from range/bucket results: evicted (tombstoned), or —
   * when a predicate is active — not among its matches. Range/bucket methods
   * below all funnel through this so filtering and eviction compose for free. */
  private isExcluded(ordinal: number): boolean {
    if (this.tombstones.has(ordinal)) {
      return true;
    }
    return this.predicate !== null && !this.matchingOrdinals.has(ordinal);
  }

  get levelStats(): ReadonlyMap<string, number> {
    return this.levelCounts;
  }

  /** Count of retained entries matching the active predicate (field filters ∧
   * query), ignoring the time range — the "N" half of "N of `totalCount`" when no
   * range is selected (`filtering` spec, "Counts reflect the predicate"). Equal to
   * `totalCount` when no predicate is active. */
  get matchingCount(): number {
    return this.predicate === null ? this.totalCount : this.matchingOrdinals.size;
  }

  private insertSorted(ref: TimestampedRef): void {
    const last = this.sorted[this.sorted.length - 1];
    if (!last || ref.tsMs >= last.tsMs) {
      // Near-sorted fast path (D2): real logs arrive close to timestamp order, so
      // this is the common case and keeps append O(1) amortized.
      this.sorted.push(ref);
      return;
    }
    const idx = lowerBound(this.sorted, ref.tsMs);
    this.sorted.splice(idx, 0, ref);
  }

  /** Evicts the oldest `count` entries in input order (front eviction, matching
   * `live-tail`'s retention model). Returns the evicted ordinals. */
  evictFront(count: number): number[] {
    const evicted: number[] = [];
    for (const ordinal of this.entries.keys()) {
      if (evicted.length >= count) {
        break;
      }
      evicted.push(ordinal);
    }
    for (const ordinal of evicted) {
      const entry = this.entries.get(ordinal);
      if (entry?.level !== undefined && entry.level !== null) {
        const c = this.levelCounts.get(entry.level);
        if (c !== undefined) {
          if (c <= 1) {
            this.levelCounts.delete(entry.level);
          } else {
            this.levelCounts.set(entry.level, c - 1);
          }
        }
      }
      this.matchingOrdinals.delete(ordinal);
      // A root's member list goes with it; the members themselves survive as
      // orphans and are treated as standalone from here on. A member evicted while
      // its root is still retained leaves that root's list.
      this.chainMembers.delete(ordinal);
      if (entry !== undefined && entry.continuationOf !== null) {
        const siblings = this.chainMembers.get(entry.continuationOf);
        if (siblings !== undefined) {
          const at = siblings.indexOf(ordinal);
          if (at >= 0) {
            siblings.splice(at, 1);
          }
        }
      }
      this.entries.delete(ordinal);
      if (this.timestamplessOrdinals.delete(ordinal)) {
        continue;
      }
      this.tombstones.add(ordinal);
    }
    if (
      this.tombstones.size >= COMPACT_MIN_TOMBSTONES &&
      this.tombstones.size >= this.sorted.length * COMPACT_RATIO
    ) {
      this.compact();
    }
    return evicted;
  }

  private compact(): void {
    if (this.tombstones.size === 0) {
      return;
    }
    this.sorted = this.sorted.filter((r) => !this.tombstones.has(r.ordinal));
    this.tombstones.clear();
  }

  /** Half-open range query [startMs, endMs): entries in input order plus a count.
   * An entry with timestamp exactly `startMs` is included; one exactly at `endMs`
   * is not — so adjacent ranges/buckets never double-count a boundary entry. */
  queryRange(startMs: number, endMs: number): RangeResult {
    const ordinals = this.ordinalsInRange(startMs, endMs);
    ordinals.sort((a, b) => a - b); // ordinal order === input order (D2)
    const entries = ordinals.map((o) => this.entries.get(o)).filter((e): e is EntryDto => e !== undefined);
    return { entries, count: entries.length };
  }

  /** Count of timestamped entries in [startMs, endMs), without materializing them
   * — cheaper than `queryRange(...).count` for callers that only need the number
   * (the timeline's outlier count). */
  countInRange(startMs: number, endMs: number): number {
    return this.ordinalsInRange(startMs, endMs).length;
  }

  /** @param respectPredicate When true (the default via `ordinalsInRange`), an
   * active predicate's non-matches are excluded too, not just evicted entries. */
  private ordinalsInRange(startMs: number, endMs: number, respectPredicate = true): number[] {
    const lo = lowerBound(this.sorted, startMs);
    const hi = lowerBound(this.sorted, endMs);
    const ordinals: number[] = [];
    for (let i = lo; i < hi; i++) {
      const ref = this.sorted[i]!; // i is within [lo, hi) and hi <= sorted.length
      if (this.tombstones.has(ref.ordinal)) {
        continue;
      }
      if (respectPredicate && this.predicate !== null && !this.matchingOrdinals.has(ref.ordinal)) {
        continue;
      }
      ordinals.push(ref.ordinal);
    }
    return ordinals;
  }

  /** Counts **events**, not lines: an entry that continues the entry above it is not
   * counted, so a sixty-frame exception is one point on the timeline rather than a
   * spike that reads as sixty failures (`timeline` spec, design D9). The consequence
   * is intended and visible — the timeline's total no longer equals the number of
   * table rows.
   *
   * Counts per bucket of width `bucketMs`, covering `bucketCount` buckets
   * starting at `startMs`, for entries matching the active predicate (D2 — this is
   * what the table and every count use; equal to `bucketCountsUnfiltered` when no
   * predicate is set). Fast enough to call on every drag frame: one pass over the
   * timestamped range via binary-searched bounds, no allocation beyond the result
   * array. */
  bucketCounts(startMs: number, bucketMs: number, bucketCount: number): number[] {
    return this.computeBucketCounts(startMs, bucketMs, bucketCount, true);
  }

  /** Counts ignoring the active predicate (evicted entries still excluded) — the
   * timeline's background series (D6, `timeline` spec "Filtered distribution
   * against the whole"): the dataset's shape stays visible under any filter,
   * including one that matches nothing. */
  bucketCountsUnfiltered(startMs: number, bucketMs: number, bucketCount: number): number[] {
    return this.computeBucketCounts(startMs, bucketMs, bucketCount, false);
  }

  private computeBucketCounts(
    startMs: number,
    bucketMs: number,
    bucketCount: number,
    respectPredicate: boolean,
  ): number[] {
    const counts = new Array<number>(bucketCount).fill(0);
    if (bucketCount <= 0 || bucketMs <= 0) {
      return counts;
    }
    const endMs = startMs + bucketMs * bucketCount;
    const lo = lowerBound(this.sorted, startMs);
    const hi = lowerBound(this.sorted, endMs);
    for (let i = lo; i < hi; i++) {
      const ref = this.sorted[i]!; // i is within [lo, hi) and hi <= sorted.length
      if (this.tombstones.has(ref.ordinal)) {
        continue;
      }
      if (respectPredicate && this.predicate !== null && !this.matchingOrdinals.has(ref.ordinal)) {
        continue;
      }
      if (this.isChainMember(ref.ordinal)) {
        continue; // one count per event, not per physical line
      }
      let idx = Math.floor((ref.tsMs - startMs) / bucketMs);
      if (idx >= bucketCount) {
        idx = bucketCount - 1;
      } else if (idx < 0) {
        idx = 0;
      }
      counts[idx] = counts[idx]! + 1; // idx is clamped into [0, bucketCount), the array's full length
    }
    return counts;
  }

  /** A robust default span over the timestamped entries (D4): Tukey's outlier
   * fence (1.5x IQR beyond the 25th/75th percentile) rather than min/max, so a
   * single absurd timestamp cannot compress every real entry into one pixel. The
   * excluded entries are reported via `outlierCount`, not hidden. Returns `null`
   * when there are no timestamped entries at all. */
  robustSpan(): RobustSpan | null {
    this.compact(); // percentile indices must line up with live entries
    const n = this.sorted.length;
    if (n === 0) {
      return null;
    }
    if (n === 1) {
      return { minMs: this.sorted[0]!.tsMs, maxMs: this.sorted[0]!.tsMs, outlierCount: 0 };
    }
    const q1 = this.sorted[Math.floor(n * 0.25)]!.tsMs;
    const q3 = this.sorted[Math.min(n - 1, Math.floor(n * 0.75))]!.tsMs;
    const iqr = q3 - q1;
    // A zero IQR means at least half the dataset shares one instant; there's no
    // meaningful "interquartile spread" to scale a fence from, so pad by a fixed
    // 1s instead of 1.5x zero.
    const pad = iqr === 0 ? 1000 : iqr * 1.5;
    const lowerFence = q1 - pad;
    const upperFence = q3 + pad;
    const idxLow = lowerBound(this.sorted, lowerFence);
    const idxHigh = upperBound(this.sorted, upperFence); // exclusive
    const minMs = this.sorted[Math.min(idxLow, n - 1)]!.tsMs;
    const maxMs = this.sorted[Math.max(idxHigh - 1, 0)]!.tsMs;
    const outlierCount = n - (idxHigh - idxLow);
    return { minMs, maxMs, outlierCount };
  }

  /** Full min/max over timestamped entries, with no outlier trimming — how the
   * timeline reaches "zoom out to see everything" (timeline spec, "Outliers
   * remain reachable"). */
  fullSpan(): { minMs: number; maxMs: number } | null {
    if (this.sorted.length === 0) {
      return null;
    }
    // `sorted` may still hold tombstones between compactions; the true min/max
    // needs the first/last *live* entries, not just index 0/length-1.
    let minMs: number | null = null;
    let maxMs: number | null = null;
    for (const ref of this.sorted) {
      if (this.tombstones.has(ref.ordinal)) {
        continue;
      }
      if (minMs === null) {
        minMs = ref.tsMs;
      }
      maxMs = ref.tsMs;
    }
    return minMs === null || maxMs === null ? null : { minMs, maxMs };
  }

  /** All retained entries in input order, matching the active predicate if one is
   * set (D2 — same rule `queryRange`/`bucketCounts` apply, for the no-range case). */
  entriesInInputOrder(): EntryDto[] {
    if (this.predicate === null) {
      return Array.from(this.entries.values());
    }
    const out: EntryDto[] = [];
    for (const [ordinal, entry] of this.entries) {
      if (this.matchingOrdinals.has(ordinal)) {
        out.push(entry);
      }
    }
    return out;
  }

  /** O(1) lookup by ordinal (the stable identity, D2) — `undefined` once evicted. */
  getByOrdinal(ordinal: number): EntryDto | undefined {
    return this.entries.get(ordinal);
  }

  get totalCount(): number {
    return this.entries.size;
  }

  get timestamplessCount(): number {
    return this.timestamplessOrdinals.size;
  }

  get timestampedCount(): number {
    return this.sorted.length - this.tombstones.size;
  }
}
