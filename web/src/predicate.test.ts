import { describe, expect, it } from "vitest";

import {
  buildHaystack,
  findMatchRanges,
  knownFieldNames,
  matchesFieldFilters,
  matchesPredicate,
  matchesQuery,
} from "./predicate";
import type { EntryDto, FieldInventoryDto } from "./wasm-types";

function mkEntry(opts: Partial<EntryDto> & { ordinal: number }): EntryDto {
  return {
    timestamp: null,
    timestampUsedDefaultTz: false,
    timestampYearInferred: false,
    level: null,
    message: "",
    fields: {},
    ...opts,
  };
}

describe("matchesFieldFilters — D1 combination rule (task 1.4)", () => {
  const errorApi = mkEntry({ ordinal: 1, level: "ERROR", fields: { service: { kind: "string", value: "api" } } });
  const warnApi = mkEntry({ ordinal: 2, level: "WARN", fields: { service: { kind: "string", value: "api" } } });
  const errorDb = mkEntry({ ordinal: 3, level: "ERROR", fields: { service: { kind: "string", value: "db" } } });
  const infoApi = mkEntry({ ordinal: 4, level: "INFO", fields: { service: { kind: "string", value: "api" } } });

  it("two values of the SAME field widen the result (OR)", () => {
    const filters = new Map([["level", new Set(["ERROR", "WARN"])]]);
    expect(matchesFieldFilters(errorApi, filters)).toBe(true);
    expect(matchesFieldFilters(warnApi, filters)).toBe(true);
    expect(matchesFieldFilters(errorDb, filters)).toBe(true); // ERROR, still matches
    expect(matchesFieldFilters(infoApi, filters)).toBe(false); // neither ERROR nor WARN
  });

  it("two DIFFERENT fields narrow the result (AND)", () => {
    const filters = new Map([
      ["level", new Set(["ERROR"])],
      ["service", new Set(["api"])],
    ]);
    expect(matchesFieldFilters(errorApi, filters)).toBe(true); // both match
    expect(matchesFieldFilters(errorDb, filters)).toBe(false); // level matches, service doesn't
    expect(matchesFieldFilters(warnApi, filters)).toBe(false); // service matches, level doesn't
    expect(matchesFieldFilters(infoApi, filters)).toBe(false); // neither
  });

  it("an empty value set for a field imposes no constraint", () => {
    const filters = new Map([["level", new Set<string>()]]);
    expect(matchesFieldFilters(infoApi, filters)).toBe(true);
  });

  it("a field absent from the entry never matches a non-empty filter", () => {
    const noService = mkEntry({ ordinal: 5, level: "ERROR" });
    const filters = new Map([["service", new Set(["api"])]]);
    expect(matchesFieldFilters(noService, filters)).toBe(false);
  });
});

describe("matchesQuery", () => {
  const entry = mkEntry({
    ordinal: 1,
    level: "ERROR",
    message: "Connection Refused by upstream",
    fields: { service: { kind: "string", value: "api" }, code: { kind: "number", value: "500" } },
  });

  it("substring search is case-insensitive over the message", () => {
    expect(matchesQuery(entry, { kind: "substring", needle: "connection refused" })).toBe(true);
    expect(matchesQuery(entry, { kind: "substring", needle: "nope" })).toBe(false);
  });

  it("substring search covers field values, not field names", () => {
    expect(matchesQuery(entry, { kind: "substring", needle: "api" })).toBe(true); // service's value
    expect(matchesQuery(entry, { kind: "substring", needle: "service" })).toBe(false); // the field name itself
  });

  it("regex search is applied as written (no implicit case-insensitivity)", () => {
    expect(matchesQuery(entry, { kind: "regex", re: /^Connection/, source: "^Connection" })).toBe(true);
    expect(matchesQuery(entry, { kind: "regex", re: /^connection/, source: "^connection" })).toBe(false);
  });

  it("kind none matches everything", () => {
    expect(matchesQuery(entry, { kind: "none" })).toBe(true);
  });
});

describe("matchesPredicate", () => {
  const entry = mkEntry({ ordinal: 1, timestamp: "2026-08-09T10:00:00Z", level: "ERROR", message: "boom" });

  it("range excludes an entry outside it", () => {
    const inRange = { startMs: Date.parse("2026-08-09T09:00:00Z"), endMs: Date.parse("2026-08-09T11:00:00Z") };
    const outOfRange = { startMs: Date.parse("2026-08-09T11:00:00Z"), endMs: Date.parse("2026-08-09T12:00:00Z") };
    expect(matchesPredicate(entry, inRange, new Map(), { kind: "none" })).toBe(true);
    expect(matchesPredicate(entry, outOfRange, new Map(), { kind: "none" })).toBe(false);
  });

  it("a timestampless entry never matches an active range", () => {
    const noTs = mkEntry({ ordinal: 2, timestamp: null });
    const range = { startMs: 0, endMs: Date.now() + 1_000_000 };
    expect(matchesPredicate(noTs, range, new Map(), { kind: "none" })).toBe(false);
  });

  it("range, fields and query all conjoin", () => {
    const range = { startMs: Date.parse("2026-08-09T09:00:00Z"), endMs: Date.parse("2026-08-09T11:00:00Z") };
    const filters = new Map([["level", new Set(["ERROR"])]]);
    expect(matchesPredicate(entry, range, filters, { kind: "substring", needle: "boom" })).toBe(true);
    expect(matchesPredicate(entry, range, filters, { kind: "substring", needle: "nope" })).toBe(false);
  });
});

describe("buildHaystack", () => {
  it("includes message, level and every field value", () => {
    const entry = mkEntry({
      ordinal: 1,
      level: "INFO",
      message: "hello",
      fields: { a: { kind: "string", value: "x" }, b: { kind: "bool", value: true } },
    });
    expect(buildHaystack(entry)).toEqual(["hello", "INFO", "x", "true"]);
  });
});

describe("knownFieldNames", () => {
  it("always includes level, plus every inventory field", () => {
    const inv: FieldInventoryDto = { cap: 10, fields: { service: { count: 1, values: {}, highCardinality: false } } };
    const names = knownFieldNames(inv);
    expect(names.has("level")).toBe(true);
    expect(names.has("service")).toBe(true);
    expect(names.has("unknown")).toBe(false);
  });

  it("includes level even with a null inventory", () => {
    expect(knownFieldNames(null).has("level")).toBe(true);
  });
});

describe("findMatchRanges", () => {
  it("finds every substring occurrence, case-insensitively", () => {
    expect(findMatchRanges("ababAB", { kind: "substring", needle: "ab" })).toEqual([
      [0, 2],
      [2, 4],
      [4, 6],
    ]);
  });

  it("finds every regex match without hanging on a zero-length pattern", () => {
    const ranges = findMatchRanges("aaa", { kind: "regex", re: /a*/, source: "a*" });
    expect(ranges.length).toBeGreaterThan(0);
  });

  it("returns nothing for kind none", () => {
    expect(findMatchRanges("anything", { kind: "none" })).toEqual([]);
  });
});
