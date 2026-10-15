import { describe, expect, it } from "vitest";

import { goneMessage, routeFor, sourcePath } from "./routing";

describe("routeFor", () => {
  it("routes / to the landing", () => {
    expect(routeFor("/")).toEqual({ kind: "landing" });
  });

  it("routes /l/<id> to that source, with or without a trailing slash", () => {
    expect(routeFor("/l/abcdefgh")).toEqual({ kind: "source", id: "abcdefgh" });
    expect(routeFor("/l/abcdefgh/")).toEqual({ kind: "source", id: "abcdefgh" });
  });

  it("treats anything else as the landing", () => {
    expect(routeFor("/l/")).toEqual({ kind: "landing" });
    expect(routeFor("/l/a/b")).toEqual({ kind: "landing" });
    expect(routeFor("/other")).toEqual({ kind: "landing" });
  });

  it("round-trips through sourcePath", () => {
    expect(routeFor(sourcePath("a b"))).toEqual({ kind: "source", id: "a b" });
  });
});

describe("goneMessage", () => {
  it("says something distinct for every reason", () => {
    const reasons = ["unknown", "closed", "unloaded", "budget"] as const;
    expect(new Set(reasons.map(goneMessage)).size).toBe(reasons.length);
  });
});
