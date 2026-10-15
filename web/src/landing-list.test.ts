import { describe, expect, it } from "vitest";

import { INITIAL_LANDING, landingRows, reduceLanding } from "./landing-list";
import type { SourceRowDto } from "./query-types";

const row = (over: Partial<SourceRowDto>): SourceRowDto => ({
  id: "aaaaaaaa",
  name: "a.log",
  kind: "argv",
  size: 2048,
  state: "ended",
  tabs: 0,
  error: null,
  ...over,
});

describe("reduceLanding", () => {
  it("replaces the list with each snapshot", () => {
    let state = reduceLanding(INITIAL_LANDING, { type: "sources", sources: [row({})] });
    state = reduceLanding(state, { type: "sources", sources: [row({ id: "bbbbbbbb", name: "b.log" })] });
    expect(landingRows(state).map((r) => r.name)).toEqual(["b.log"]);
  });

  it("remembers a close in flight until the source leaves the list", () => {
    let state = reduceLanding(INITIAL_LANDING, { type: "sources", sources: [row({})] });
    state = reduceLanding(state, { type: "closing", id: "aaaaaaaa" });
    expect(landingRows(state)[0]!.closing).toBe(true);
    state = reduceLanding(state, { type: "sources", sources: [row({ tabs: 1 })] });
    expect(landingRows(state)[0]!.closing).toBe(true);
    state = reduceLanding(state, { type: "sources", sources: [] });
    expect(state.closing.size).toBe(0);
  });

  it("forgets a close that failed", () => {
    let state = reduceLanding(INITIAL_LANDING, { type: "sources", sources: [row({})] });
    state = reduceLanding(state, { type: "closing", id: "aaaaaaaa" });
    state = reduceLanding(state, { type: "close_failed", id: "aaaaaaaa" });
    expect(landingRows(state)[0]!.closing).toBe(false);
  });
});

describe("landingRows", () => {
  it("is empty before the first snapshot", () => {
    expect(INITIAL_LANDING.sources).toBeNull();
    expect(landingRows(INITIAL_LANDING)).toEqual([]);
  });

  it("labels kind, size, state and tabs, links each source and never offers to close stdin", () => {
    const state = reduceLanding(INITIAL_LANDING, {
      type: "sources",
      sources: [
        row({}),
        row({ id: "ssssssss", name: "stdin", kind: "stdin", state: "live", tabs: 1 }),
        row({ id: "uuuuuuuu", name: "up.jsonl", kind: "upload", state: "parsing", tabs: 2 }),
        row({ id: "eeeeeeee", name: "gone.log", state: "error", error: "could not open gone.log" }),
      ],
    });
    expect(landingRows(state)).toEqual([
      expect.objectContaining({ kind: "file", size: "2.0 KB", state: "loaded", tabs: "0 tabs", href: "/l/aaaaaaaa", closable: true }),
      expect.objectContaining({ kind: "stdin", state: "live", tabs: "1 tab", closable: false }),
      expect.objectContaining({ kind: "upload", state: "parsing", tabs: "2 tabs", closable: true }),
      expect.objectContaining({ state: "error", error: "could not open gone.log" }),
    ]);
  });

  it("says ended for a finished stream", () => {
    const state = reduceLanding(INITIAL_LANDING, {
      type: "sources",
      sources: [row({ kind: "stdin", state: "ended" })],
    });
    expect(landingRows(state)[0]!.state).toBe("ended");
  });
});
