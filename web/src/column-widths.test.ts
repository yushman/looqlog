import { describe, expect, it } from "vitest";

import {
  clampColumnWidth,
  DEFAULT_COLUMN_WIDTHS,
  encodeColumnWidths,
  MAX_COLUMN_WIDTH_REM,
  MIN_COLUMN_WIDTHS,
  parseColumnWidths,
} from "./column-widths";

describe("clampColumnWidth (design D5 — a floor, per column)", () => {
  it("clamps below the floor rather than refusing the value", () => {
    expect(clampColumnWidth("timestamp", 0)).toBe(MIN_COLUMN_WIDTHS.timestamp);
    expect(clampColumnWidth("ordinal", -50)).toBe(MIN_COLUMN_WIDTHS.ordinal);
  });

  it("clamps above the ceiling", () => {
    expect(clampColumnWidth("timestamp", 5000)).toBe(MAX_COLUMN_WIDTH_REM);
  });

  it("leaves an in-range width alone", () => {
    expect(clampColumnWidth("timestamp", 8.25)).toBe(8.25);
  });
});

describe("encodeColumnWidths / parseColumnWidths round trip", () => {
  it("omits the key entirely at the defaults (absent means defaults)", () => {
    expect(encodeColumnWidths(DEFAULT_COLUMN_WIDTHS)).toBeNull();
    expect(parseColumnWidths(null).widths).toEqual(DEFAULT_COLUMN_WIDTHS);
    expect(parseColumnWidths(null).errors).toEqual([]);
  });

  it("round-trips two resized columns", () => {
    const widths = { ordinal: 4.5, timestamp: 7.25, level: 2.5 };
    const encoded = encodeColumnWidths(widths);
    expect(encoded).toBe("4.5,7.25,2.5");
    const parsed = parseColumnWidths(encoded);
    expect(parsed.widths).toEqual(widths);
    expect(parsed.errors).toEqual([]);
  });

  it("rounds a dragged width to two decimals rather than putting a float's tail in the URL", () => {
    expect(encodeColumnWidths({ ...DEFAULT_COLUMN_WIDTHS, timestamp: 7.123456789 })).toBe("3,7.12,2.5");
  });
});

describe("parseColumnWidths — a bad width never blocks the log (`url-state` spec)", () => {
  it("falls back to the default for a value that is not a number, and reports it", () => {
    const parsed = parseColumnWidths("3,wide,2.5");
    expect(parsed.widths.timestamp).toBe(DEFAULT_COLUMN_WIDTHS.timestamp);
    expect(parsed.widths.ordinal).toBe(3);
    expect(parsed.errors).toHaveLength(1);
    expect(parsed.errors[0]).toContain("timestamp");
  });

  it("clamps an out-of-range width to the minimum rather than rejecting it, and reports it", () => {
    const parsed = parseColumnWidths("3,0.001,2.5");
    expect(parsed.widths.timestamp).toBe(MIN_COLUMN_WIDTHS.timestamp);
    expect(parsed.errors).toHaveLength(1);
    expect(parsed.errors[0]).toContain("out of range");
  });

  it("clamps an absurdly large width to the ceiling", () => {
    expect(parseColumnWidths("3,99999,2.5").widths.timestamp).toBe(MAX_COLUMN_WIDTH_REM);
  });

  it("falls back wholesale when the arity is wrong — there is no way to assign positions", () => {
    const parsed = parseColumnWidths("3,12.5");
    expect(parsed.widths).toEqual(DEFAULT_COLUMN_WIDTHS);
    expect(parsed.errors).toHaveLength(1);
  });

  it("never throws and never returns an unusable width for hostile input", () => {
    for (const raw of ["", ",,", "NaN,NaN,NaN", "Infinity,-Infinity,0", "a,b,c", "1e400,1,1"]) {
      const parsed = parseColumnWidths(raw);
      expect(parsed.widths.ordinal).toBeGreaterThanOrEqual(MIN_COLUMN_WIDTHS.ordinal);
      expect(parsed.widths.timestamp).toBeGreaterThanOrEqual(MIN_COLUMN_WIDTHS.timestamp);
      expect(parsed.widths.level).toBeGreaterThanOrEqual(MIN_COLUMN_WIDTHS.level);
    }
  });
});
