import { describe, expect, it } from "vitest";

import { EntryIndex } from "./entry-index";
import type { EntryDto } from "./wasm-types";

function mkEntry(ordinal: number, timestamp: string | null, message = `m${ordinal}`): EntryDto {
  return { ordinal, timestamp, timestampUsedDefaultTz: false, level: null, message, fields: {} };
}

function iso(ms: number): string {
  return new Date(ms).toISOString();
}

describe("EntryIndex — out-of-order input", () => {
  it("returns every entry in a range regardless of input position (task 1.1)", () => {
    const idx = new EntryIndex();
    // Deliberately non-monotonic input order: ordinal 1 has the latest timestamp,
    // ordinal 5 the earliest — a "badly out of order" fixture (task 1.3).
    idx.append([
      mkEntry(1, iso(5000)),
      mkEntry(2, iso(1000)),
      mkEntry(3, iso(4000)),
      mkEntry(4, iso(2000)),
      mkEntry(5, iso(0)),
    ]);

    const result = idx.queryRange(1000, 5000);
    // Half-open [1000, 5000): ordinals 2 (1000), 4 (2000), 3 (4000) — not 1 (5000,
    // excluded end) and not 5 (0, before start). Returned in input order, i.e.
    // ordinal order (D2), not timestamp order: 2, 3, 4.
    expect(result.entries.map((e) => e.ordinal)).toEqual([2, 3, 4]);
    expect(result.count).toBe(3);
  });

  it("stays correct across many out-of-order appends and an eviction (task 1.3/1.4)", () => {
    const idx = new EntryIndex();
    const n = 2000;
    // Reverse-chronological input: the worst case for the near-sorted fast path.
    const entries: EntryDto[] = [];
    for (let i = 0; i < n; i++) {
      entries.push(mkEntry(i + 1, iso((n - i) * 1000)));
    }
    idx.append(entries);
    expect(idx.totalCount).toBe(n);
    expect(idx.timestampedCount).toBe(n);

    // Evict the first 500 by input order (ordinals 1..500, i.e. the *latest*
    // timestamps, since input order is reversed here).
    const evicted = idx.evictFront(500);
    expect(evicted.length).toBe(500);
    expect(idx.totalCount).toBe(n - 500);

    // No stale references: querying the full original span only returns what's left.
    const full = idx.queryRange(0, (n + 1) * 1000);
    expect(full.count).toBe(n - 500);
    for (const e of full.entries) {
      expect(evicted).not.toContain(e.ordinal);
    }
    // Input order preserved among survivors.
    const ordinals = full.entries.map((e) => e.ordinal);
    expect(ordinals).toEqual([...ordinals].sort((a, b) => a - b));
  });
});

describe("EntryIndex — timestampless entries", () => {
  it("holds them apart from range results and counts them separately (task 1.2)", () => {
    const idx = new EntryIndex();
    idx.append([mkEntry(1, iso(0)), mkEntry(2, null), mkEntry(3, iso(1000)), mkEntry(4, null)]);

    expect(idx.timestamplessCount).toBe(2);
    expect(idx.timestampedCount).toBe(2);
    expect(idx.totalCount).toBe(4);

    const result = idx.queryRange(-Infinity, Infinity);
    expect(result.entries.map((e) => e.ordinal)).toEqual([1, 3]);
  });
});

describe("EntryIndex — front eviction", () => {
  it("leaves no stale references in queries or bucket counts (task 1.4)", () => {
    const idx = new EntryIndex();
    idx.append([mkEntry(1, iso(0)), mkEntry(2, iso(1000)), mkEntry(3, iso(2000)), mkEntry(4, null)]);

    idx.evictFront(2); // evicts ordinal 1 (ts=0) and ordinal 2 (ts=1000)

    expect(idx.totalCount).toBe(2);
    const result = idx.queryRange(0, 3000);
    expect(result.entries.map((e) => e.ordinal)).toEqual([3]);
    expect(idx.bucketCounts(0, 1000, 3)).toEqual([0, 0, 1]);
  });

  it("also evicts timestampless entries when they're oldest by input order", () => {
    const idx = new EntryIndex();
    idx.append([mkEntry(1, null), mkEntry(2, iso(0))]);
    idx.evictFront(1);
    expect(idx.timestamplessCount).toBe(0);
    expect(idx.totalCount).toBe(1);
  });

  it("stays consistent under many small evictions forcing compaction", () => {
    const idx = new EntryIndex();
    const n = 5000;
    const entries: EntryDto[] = [];
    for (let i = 0; i < n; i++) {
      entries.push(mkEntry(i + 1, iso(i * 1000)));
    }
    idx.append(entries);
    for (let i = 0; i < n - 10; i++) {
      idx.evictFront(1);
    }
    expect(idx.totalCount).toBe(10);
    const result = idx.queryRange(0, n * 1000);
    expect(result.count).toBe(10);
    expect(result.entries.map((e) => e.ordinal)).toEqual([n - 9, n - 8, n - 7, n - 6, n - 5, n - 4, n - 3, n - 2, n - 1, n]);
  });
});

describe("EntryIndex — half-open range boundary", () => {
  it("applies the boundary consistently at exact-match timestamps (task 1.5)", () => {
    const idx = new EntryIndex();
    idx.append([mkEntry(1, iso(0)), mkEntry(2, iso(1000)), mkEntry(3, iso(2000))]);

    expect(idx.queryRange(0, 1000).entries.map((e) => e.ordinal)).toEqual([1]);
    expect(idx.queryRange(1000, 2000).entries.map((e) => e.ordinal)).toEqual([2]);
    // Adjacent ranges never double-count the shared boundary.
    const a = idx.queryRange(0, 1000).entries.map((e) => e.ordinal);
    const b = idx.queryRange(1000, 2000).entries.map((e) => e.ordinal);
    expect(a.filter((o) => b.includes(o))).toEqual([]);
  });

  it("returns an empty, distinguishable result for a range with no entries", () => {
    const idx = new EntryIndex();
    idx.append([mkEntry(1, iso(0))]);
    const result = idx.queryRange(5000, 6000);
    expect(result.entries).toEqual([]);
    expect(result.count).toBe(0);
  });
});

describe("EntryIndex — bucket counts", () => {
  it("matches a manual count from a small fixture (task 2.4)", () => {
    const idx = new EntryIndex();
    idx.append([
      mkEntry(1, iso(500)), // bucket 0 [0,1000)
      mkEntry(2, iso(900)), // bucket 0
      mkEntry(3, iso(1500)), // bucket 1 [1000,2000)
      mkEntry(4, iso(3200)), // bucket 3 [3000,4000)
      mkEntry(5, null),
    ]);
    expect(idx.bucketCounts(0, 1000, 4)).toEqual([2, 1, 0, 1]);
  });
});

describe("EntryIndex — robust span (D4)", () => {
  it("excludes a single epoch-zero outlier from the default span and reports it", () => {
    const idx = new EntryIndex();
    const base = Date.parse("2026-08-09T12:00:00Z");
    const entries: EntryDto[] = [];
    for (let i = 0; i < 200; i++) {
      entries.push(mkEntry(i + 1, iso(base + i * 1000)));
    }
    entries.push(mkEntry(201, iso(0))); // the Unix epoch
    idx.append(entries);

    const span = idx.robustSpan()!;
    expect(span.minMs).toBeGreaterThan(base - 1000);
    expect(span.maxMs).toBeLessThanOrEqual(base + 200 * 1000);
    expect(span.outlierCount).toBe(1);

    const full = idx.fullSpan()!;
    expect(full.minMs).toBe(0); // still reachable by zooming out
  });

  it("returns null when there are no timestamped entries", () => {
    const idx = new EntryIndex();
    idx.append([mkEntry(1, null)]);
    expect(idx.robustSpan()).toBeNull();
    expect(idx.fullSpan()).toBeNull();
  });
});

function mkLeveled(ordinal: number, tsMs: number, level: string | null): EntryDto {
  return { ordinal, timestamp: iso(tsMs), timestampUsedDefaultTz: false, level, message: `m${ordinal}`, fields: {} };
}

describe("EntryIndex — predicate (filtering-and-search, D2/D9)", () => {
  it("setPredicate(null) is equivalent to no filtering at all", () => {
    const idx = new EntryIndex();
    idx.append([mkLeveled(1, 0, "ERROR"), mkLeveled(2, 1000, "INFO")]);
    idx.setPredicate(null);
    expect(idx.hasActivePredicate).toBe(false);
    expect(idx.matchingCount).toBe(2);
    expect(idx.entriesInInputOrder().map((e) => e.ordinal)).toEqual([1, 2]);
    expect(idx.bucketCounts(0, 1000, 2)).toEqual([1, 1]);
  });

  it("filters entriesInInputOrder, queryRange and bucketCounts consistently (D2: one predicate, one result)", () => {
    const idx = new EntryIndex();
    idx.append([mkLeveled(1, 0, "ERROR"), mkLeveled(2, 1000, "INFO"), mkLeveled(3, 2000, "ERROR")]);
    idx.setPredicate((e) => e.level === "ERROR");

    expect(idx.hasActivePredicate).toBe(true);
    expect(idx.matchingCount).toBe(2);
    expect(idx.entriesInInputOrder().map((e) => e.ordinal)).toEqual([1, 3]);
    expect(idx.queryRange(0, 3000).entries.map((e) => e.ordinal)).toEqual([1, 3]);
    expect(idx.bucketCounts(0, 1000, 3)).toEqual([1, 0, 1]);
    // The unfiltered series (timeline background) ignores the predicate entirely.
    expect(idx.bucketCountsUnfiltered(0, 1000, 3)).toEqual([1, 1, 1]);
  });

  it("a predicate matching nothing still leaves the unfiltered series showing the dataset's shape", () => {
    const idx = new EntryIndex();
    idx.append([mkLeveled(1, 0, "INFO"), mkLeveled(2, 1000, "INFO")]);
    idx.setPredicate(() => false);
    expect(idx.matchingCount).toBe(0);
    expect(idx.entriesInInputOrder()).toEqual([]);
    expect(idx.bucketCounts(0, 1000, 2)).toEqual([0, 0]);
    expect(idx.bucketCountsUnfiltered(0, 1000, 2)).toEqual([1, 1]);
  });

  it("a newly appended entry is tested against the active predicate on arrival", () => {
    const idx = new EntryIndex();
    idx.setPredicate((e) => e.level === "ERROR");
    idx.append([mkLeveled(1, 0, "INFO")]);
    expect(idx.matchingCount).toBe(0);
    idx.append([mkLeveled(2, 1000, "ERROR")]);
    expect(idx.matchingCount).toBe(1);
    expect(idx.entriesInInputOrder().map((e) => e.ordinal)).toEqual([2]);
  });

  it("eviction removes an entry from the matching set too", () => {
    const idx = new EntryIndex();
    idx.append([mkLeveled(1, 0, "ERROR"), mkLeveled(2, 1000, "ERROR")]);
    idx.setPredicate((e) => e.level === "ERROR");
    expect(idx.matchingCount).toBe(2);
    idx.evictFront(1);
    expect(idx.matchingCount).toBe(1);
    expect(idx.entriesInInputOrder().map((e) => e.ordinal)).toEqual([2]);
  });
});

describe("EntryIndex — level counts", () => {
  it("tracks distinct level values incrementally, including across eviction", () => {
    const idx = new EntryIndex();
    idx.append([mkLeveled(1, 0, "ERROR"), mkLeveled(2, 1000, "ERROR"), mkLeveled(3, 2000, "INFO"), mkLeveled(4, 3000, null)]);
    expect(idx.levelStats.get("ERROR")).toBe(2);
    expect(idx.levelStats.get("INFO")).toBe(1);
    expect(idx.levelStats.has("null")).toBe(false);

    idx.evictFront(1); // evicts the first ERROR
    expect(idx.levelStats.get("ERROR")).toBe(1);

    idx.evictFront(1); // evicts the second ERROR
    expect(idx.levelStats.has("ERROR")).toBe(false);
  });
});
