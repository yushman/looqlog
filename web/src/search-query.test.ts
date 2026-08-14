import { describe, expect, it } from "vitest";

import { compileQuery, extractFieldTokens } from "./search-query";

describe("compileQuery", () => {
  it("empty text compiles to kind none", () => {
    expect(compileQuery("")).toEqual({ compiled: { kind: "none" }, error: null });
  });

  it("plain text compiles to a lowercased substring", () => {
    const result = compileQuery("Connection Refused");
    expect(result.error).toBeNull();
    expect(result.compiled).toEqual({ kind: "substring", needle: "connection refused" });
  });

  it("re: prefix compiles a regex applied as written", () => {
    const result = compileQuery("re:^ERROR.*timeout");
    expect(result.error).toBeNull();
    expect(result.compiled?.kind).toBe("regex");
    if (result.compiled?.kind === "regex") {
      expect(result.compiled.re.test("ERROR: timeout")).toBe(true);
      expect(result.compiled.re.test("error: timeout")).toBe(false); // no implicit `i`
    }
  });

  it("an unbalanced bracket fails loudly, not silently", () => {
    const result = compileQuery("re:[unclosed");
    expect(result.compiled).toBeNull();
    expect(result.error).toMatch(/invalid regex/);
  });

  it("the documented escape produces a literal substring search for text starting with re:", () => {
    const result = compileQuery("\\re:something");
    expect(result.error).toBeNull();
    expect(result.compiled).toEqual({ kind: "substring", needle: "re:something" });
  });
});

describe("extractFieldTokens", () => {
  const known = new Set(["level", "service"]);

  it("converts a complete field=value token followed by a space", () => {
    const result = extractFieldTokens("service=api ", known);
    expect(result.tokens).toEqual([{ field: "service", value: "api" }]);
    expect(result.remainingText).toBe("");
  });

  it("leaves the in-progress last token alone when there's no trailing space", () => {
    const result = extractFieldTokens("service=api", known);
    expect(result.tokens).toEqual([]);
    expect(result.remainingText).toBe("service=api");
  });

  it("mixes a converted chip with remaining free text", () => {
    const result = extractFieldTokens("level=ERROR connection refused", known);
    expect(result.tokens).toEqual([{ field: "level", value: "ERROR" }]);
    expect(result.remainingText).toBe("connection refused");
  });

  it("leaves an unknown field as literal text rather than converting it", () => {
    const result = extractFieldTokens("unknownfield=x ", known);
    expect(result.tokens).toEqual([]);
    expect(result.remainingText).toBe("unknownfield=x");
  });

  it("handles multiple complete tokens for different fields", () => {
    const result = extractFieldTokens("level=ERROR service=api ", known);
    expect(result.tokens).toEqual([
      { field: "level", value: "ERROR" },
      { field: "service", value: "api" },
    ]);
    expect(result.remainingText).toBe("");
  });
});
