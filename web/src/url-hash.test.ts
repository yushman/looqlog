import { describe, expect, it } from "vitest";

import { decodeHash, encodeHash, type HashState } from "./url-hash";

describe("encodeHash / decodeHash round trip (task 5.2)", () => {
  it("round-trips a range, two field filters and a query", () => {
    const state: HashState = {
      range: { startMs: 1000, endMs: 5000 },
      fieldFilters: new Map([
        ["level", new Set(["ERROR", "WARN"])],
        ["service", new Set(["api"])],
      ]),
      query: "connection refused",
      formatOverride: "json",
      tzOffsetMinutes: -300,
    };
    const decoded = decodeHash(encodeHash(state));
    expect(decoded.range).toEqual(state.range);
    expect(decoded.fieldFilters).toEqual(state.fieldFilters);
    expect(decoded.query).toBe(state.query);
    expect(decoded.formatOverride).toBe(state.formatOverride);
    expect(decoded.tzOffsetMinutes).toBe(state.tzOffsetMinutes);
    expect(decoded.unknownKeys).toEqual([]);
    expect(decoded.rangeError).toBeNull();
  });

  it("a field value containing a comma or an equals sign round-trips unchanged", () => {
    const state: HashState = {
      range: null,
      fieldFilters: new Map([["message", new Set(["a=b,c=d", "x,y=z"])]]),
      query: "",
      formatOverride: null,
      tzOffsetMinutes: null,
    };
    const decoded = decodeHash(encodeHash(state));
    expect(decoded.fieldFilters.get("message")).toEqual(new Set(["a=b,c=d", "x,y=z"]));
  });

  it("a query containing & and # round-trips unchanged", () => {
    const state: HashState = {
      range: null,
      fieldFilters: new Map(),
      query: "re:a&b#c=d",
      formatOverride: null,
      tzOffsetMinutes: null,
    };
    const decoded = decodeHash(encodeHash(state));
    expect(decoded.query).toBe("re:a&b#c=d");
  });

  it("an empty state encodes to an empty string", () => {
    const state: HashState = { range: null, fieldFilters: new Map(), query: "", formatOverride: null, tzOffsetMinutes: null };
    expect(encodeHash(state)).toBe("");
  });
});

describe("decodeHash — malformed input", () => {
  it("reports an unknown key while applying the recognised ones", () => {
    const decoded = decodeHash("level=ERROR&q=boom");
    // "level" is not a grammar key (filters are namespaced under "filter="),
    // so it must be reported...
    expect(decoded.unknownKeys).toEqual(["level"]);
    // ...while "q" still applies.
    expect(decoded.query).toBe("boom");
  });

  it("an unparsable range is reported and not applied, the rest still is", () => {
    const decoded = decodeHash("range=not-a-range&q=boom");
    expect(decoded.range).toBeNull();
    expect(decoded.rangeError).not.toBeNull();
    expect(decoded.query).toBe("boom");
  });

  it("accepts a leading #", () => {
    const decoded = decodeHash("#q=boom");
    expect(decoded.query).toBe("boom");
  });

  it("counts a malformed filter entry (no =) without throwing", () => {
    const decoded = decodeHash("filter=noequalsign&q=boom");
    expect(decoded.malformedFilterCount).toBe(1);
    expect(decoded.query).toBe("boom");
  });
});
