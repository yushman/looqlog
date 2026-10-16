import { describe, expect, it } from "vitest";

import { DEFAULT_COLUMN_WIDTHS, MIN_COLUMN_WIDTHS } from "./column-widths";
import { decodeHash, EMPTY_HASH_STATE, encodeHash, formatOverrideFromHash, type HashState } from "./url-hash";

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
      tzName: null,
      columnWidths: DEFAULT_COLUMN_WIDTHS,
      collapsedPanes: new Set(),
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
      tzName: null,
      columnWidths: DEFAULT_COLUMN_WIDTHS,
      collapsedPanes: new Set(),
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
      tzName: null,
      columnWidths: DEFAULT_COLUMN_WIDTHS,
      collapsedPanes: new Set(),
    };
    const decoded = decodeHash(encodeHash(state));
    expect(decoded.query).toBe("re:a&b#c=d");
  });

  it("an empty state encodes to an empty string", () => {
    const state: HashState = {
      range: null,
      fieldFilters: new Map(),
      query: "",
      formatOverride: null,
      tzOffsetMinutes: null,
      tzName: null,
      columnWidths: DEFAULT_COLUMN_WIDTHS,
      collapsedPanes: new Set(),
    };
    expect(encodeHash(state)).toBe("");
  });

  it("round-trips resized column widths (`url-state` spec, \"Column widths round-trip\")", () => {
    const state: HashState = {
      range: null,
      fieldFilters: new Map(),
      query: "",
      formatOverride: null,
      tzOffsetMinutes: null,
      tzName: null,
      columnWidths: { ordinal: 4.5, timestamp: 6, level: 2.5 },
      collapsedPanes: new Set(),
    };
    const encoded = encodeHash(state);
    expect(encoded).toBe("cols=4.5%2C6%2C2.5");
    expect(decodeHash(encoded).columnWidths).toEqual(state.columnWidths);
  });

  it("a hash with no widths means the defaults, not an error", () => {
    const decoded = decodeHash("q=boom");
    expect(decoded.columnWidths).toEqual(DEFAULT_COLUMN_WIDTHS);
    expect(decoded.columnWidthErrors).toEqual([]);
  });

  it("round-trips a collapsed pane (`url-state` spec, \"Collapsed panes round-trip\")", () => {
    const state: HashState = {
      range: null,
      fieldFilters: new Map(),
      query: "",
      formatOverride: null,
      tzOffsetMinutes: null,
      tzName: null,
      columnWidths: DEFAULT_COLUMN_WIDTHS,
      collapsedPanes: new Set(["detail"]),
    };
    const encoded = encodeHash(state);
    expect(encoded).toBe("panes=detail");
    expect(decodeHash(encoded).collapsedPanes).toEqual(new Set(["detail"]));
  });

  it("round-trips both panes collapsed, in a stable order", () => {
    const encoded = encodeHash({
      ...EMPTY_HASH_STATE,
      collapsedPanes: new Set(["detail", "rail"]),
    });
    // `+` is the documented separator; in a form-urlencoded fragment a literal
    // `+` means a space, so it travels as %2B.
    expect(encoded).toBe("panes=rail%2Bdetail");
    expect(decodeHash(encoded).collapsedPanes).toEqual(new Set(["rail", "detail"]));
  });

  it("nothing collapsed means no key at all (`url-state` spec, \"Nothing collapsed means no key\")", () => {
    const encoded = encodeHash({ ...EMPTY_HASH_STATE, query: "boom" });
    expect(encoded).toBe("q=boom");
    expect(encoded).not.toContain("panes");
  });

  it("a hash with no panes key collapses nothing and reports nothing", () => {
    const decoded = decodeHash("q=boom");
    expect(decoded.collapsedPanes).toEqual(new Set());
    expect(decoded.collapsedPaneErrors).toEqual([]);
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

  it("a malformed width falls back to the default, is reported, and the rest still applies", () => {
    const decoded = decodeHash("cols=3,wide,2.5&q=boom");
    expect(decoded.columnWidths.timestamp).toBe(DEFAULT_COLUMN_WIDTHS.timestamp);
    expect(decoded.columnWidthErrors).toHaveLength(1);
    // The whole point of the requirement: a bad width is a presentation problem,
    // never a reason for the rest of the view — or the log — not to load.
    expect(decoded.query).toBe("boom");
    expect(decoded.unknownKeys).toEqual([]);
  });

  it("an out-of-range width is clamped to the column minimum, not rejected", () => {
    const decoded = decodeHash("cols=3,0.01,2.5");
    expect(decoded.columnWidths.timestamp).toBe(MIN_COLUMN_WIDTHS.timestamp);
    expect(decoded.columnWidthErrors).toHaveLength(1);
  });

  it("`cols` is a known key, so it is never reported as unrecognised", () => {
    expect(decodeHash("cols=3,12.5,2.5").unknownKeys).toEqual([]);
  });

  it("an unknown pane name is dropped and reported while the rest of the value applies", () => {
    // `url-state` spec, "An unknown name is dropped, the rest applies".
    const decoded = decodeHash("panes=rail%2Bsidebar&q=boom");
    expect(decoded.collapsedPanes).toEqual(new Set(["rail"]));
    expect(decoded.collapsedPaneErrors).toHaveLength(1);
    expect(decoded.collapsedPaneErrors[0]).toContain("sidebar");
    // A collapsed pane is a presentation preference, never a reason for the log
    // — or the rest of the view — not to load.
    expect(decoded.query).toBe("boom");
    expect(decoded.unknownKeys).toEqual([]);
  });

  it("a value that cannot be parsed at all leaves both panes expanded and is reported", () => {
    const decoded = decodeHash("panes=&q=boom");
    expect(decoded.collapsedPanes).toEqual(new Set());
    expect(decoded.collapsedPaneErrors).toHaveLength(1);
    expect(decoded.query).toBe("boom");
  });

  it("a value of nothing but junk collapses nothing, reports, and still loads", () => {
    const decoded = decodeHash("panes=%3F%3F%3F&q=boom");
    expect(decoded.collapsedPanes).toEqual(new Set());
    expect(decoded.collapsedPaneErrors).toHaveLength(1);
    expect(decoded.query).toBe("boom");
  });

  it("accepts the documented `+` spelling typed by hand (which arrives as a space)", () => {
    expect(decodeHash("panes=rail detail").collapsedPanes).toEqual(new Set(["rail", "detail"]));
    expect(decodeHash("panes=rail detail").collapsedPaneErrors).toEqual([]);
  });

  it("`panes` is a known key, so it is never reported as unrecognised", () => {
    expect(decodeHash("panes=rail").unknownKeys).toEqual([]);
  });
});

describe("tz grammar", () => {
  it("decodeHash reads an IANA name as tzName", () => {
    const d = decodeHash("#tz=Europe/Moscow");
    expect(d.tzName).toBe("Europe/Moscow");
    expect(d.tzOffsetMinutes).toBeNull();
    expect(decodeHash("#tz=Etc/GMT+3").tzName).toBe("Etc/GMT+3");
    expect(decodeHash("#tz=Etc/GMT%2B3").tzName).toBe("Etc/GMT+3");
  });

  it("an integer offset stays tzOffsetMinutes and leaves tzName null", () => {
    const d = decodeHash("#tz=180");
    expect(d.tzOffsetMinutes).toBe(180);
    expect(d.tzName).toBeNull();
    expect(decodeHash("#tz=1439").tzOffsetMinutes).toBe(1439);
    expect(decodeHash("#tz=-1439").tzOffsetMinutes).toBe(-1439);
  });

  it("drops values that are neither an offset nor a name", () => {
    for (const v of ["1.5", "+03:00", "1e10", "1440", "-1440", ""]) {
      const d = decodeHash(`#tz=${v}`);
      expect(d.tzOffsetMinutes, v).toBeNull();
      expect(d.tzName, v).toBeNull();
    }
  });
});

describe("tz writeback", () => {
  it("encodeHash writes a tzName back verbatim and round-trips", () => {
    for (const tzName of ["Etc/GMT+3", "Europe/Moscow"]) {
      const hash = encodeHash({ ...EMPTY_HASH_STATE, tzName });
      expect(hash).toBe(`tz=${new URLSearchParams({ tz: tzName }).toString().slice(3)}`);
      const d = decodeHash(hash);
      expect(d.tzName).toBe(tzName);
      expect(d.tzOffsetMinutes).toBeNull();
    }
  });

  it("#tz=180 round-trips unchanged", () => {
    expect(encodeHash({ ...EMPTY_HASH_STATE, tzOffsetMinutes: 180 })).toBe("tz=180");
  });

  it("formatOverrideFromHash returns tzName and is non-null when only it is set", () => {
    expect(formatOverrideFromHash("#tz=Europe/Moscow")).toEqual({
      formatOverride: null,
      tzOffsetMinutes: null,
      tzName: "Europe/Moscow",
    });
  });
});

describe("formatOverrideFromHash", () => {
  it("reads only format and tz, and returns null when neither is set", () => {
    expect(formatOverrideFromHash("#format=plain&tz=180")).toEqual({ formatOverride: "plain", tzOffsetMinutes: 180, tzName: null });
    expect(formatOverrideFromHash("#tz=-60")).toEqual({ formatOverride: null, tzOffsetMinutes: -60, tzName: null });
    expect(formatOverrideFromHash("#range=1,2&filter=a%3Db&q=x&cols=3,12,2.5&panes=rail")).toBeNull();
    expect(formatOverrideFromHash("#tz=+03:00")).toBeNull();
    expect(formatOverrideFromHash("#tz=1.5")).toBeNull();
    expect(formatOverrideFromHash("#tz=1e10")).toBeNull();
    expect(formatOverrideFromHash("")).toBeNull();
  });
});
