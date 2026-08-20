// Source-level assertions about the collapsible workspace panes
// (`app-shell` spec, collapsible-workspace-panes design D1/D2/D5).
//
// Same reasoning as `entry-table-styles.test.ts`: `web/` has no DOM test
// environment, and these are exactly the properties that regress silently — a
// second copy of the breakpoint, a `display: none` that quietly discards a pane's
// scroll position on every toggle, or a `visibility: hidden` widened back to the
// whole pane, which would take the strip with it and leave a collapsed pane with
// no way back. The browser-side half of the guarantee is tasks 6.2–6.5 and 8.4–8.7
// in this change's tasks.md.

import cssSource from "./style.css?raw";
import workspaceSource from "./components/looqlog-workspace.ts?raw";

import { describe, expect, it } from "vitest";

describe("the stacking breakpoint is defined once (design D5)", () => {
  it("appears exactly once in the stylesheet", () => {
    expect(cssSource.match(/1100px/g)).toHaveLength(1);
  });

  it("is not repeated in the script, which reads the stylesheet's own definition", () => {
    expect(workspaceSource).not.toContain("1100");
    expect(workspaceSource).toContain("--narrow-layout");
  });

  it("is published as a custom property the script can read", () => {
    expect(cssSource).toMatch(/@media \(max-width: 1100px\) \{\s*:root \{\s*--narrow-layout: 1;/);
  });
});

describe("collapse changes the grid template, not the pane's display (design D2)", () => {
  it("drives both side tracks from custom properties", () => {
    expect(cssSource).toContain("var(--ws-rail-width)");
    expect(cssSource).toContain("var(--ws-detail-width)");
    expect(cssSource).toMatch(
      /\.workspace\[data-collapsed~="rail"\] \{\s*--ws-rail-width: var\(--ws-strip-width\);/,
    );
    expect(cssSource).toMatch(
      /\.workspace\[data-collapsed~="detail"\] \{\s*--ws-detail-width: var\(--ws-strip-width\);/,
    );
  });

  it("collapses to the strip's width, never to zero (design D1)", () => {
    // A zero track would take the strip with it, and the strip is the only way
    // back now that the reopen control lives on the pane.
    expect(cssSource).toMatch(/--ws-strip-width: [^0][^;]*;/);
    expect(cssSource).not.toMatch(/--ws-(rail|detail)-width: 0;/);
  });

  it("hides only the collapsed pane's content, with visibility rather than display", () => {
    // `display: none` would discard the pane's scroll position on every toggle;
    // a narrow pane with default visibility would still be tabbable. Scoped to
    // `.ws-pane-content` so the strip inside the same pane stays reachable.
    const rule = /\.workspace\[data-collapsed~="rail"\] \.ws-rail > \.ws-pane-content,\s*\.workspace\[data-collapsed~="detail"\] \.ws-detail > \.ws-pane-content \{([^}]*)\}/.exec(
      cssSource,
    );
    expect(rule).not.toBeNull();
    expect(rule![1]).toContain("visibility: hidden");
    expect(rule![1]).not.toContain("display: none");
  });
});

describe("the control belongs to the pane it collapses (design D1)", () => {
  it("renders a strip with a toggle inside each side pane", () => {
    for (const [cls, pane] of [
      ["ws-rail", "rail"],
      ["ws-detail", "detail"],
    ]) {
      const aside = new RegExp(`<aside class="${cls}"[\\s\\S]*?</aside>`).exec(workspaceSource);
      expect(aside).not.toBeNull();
      expect(aside![0]).toContain(`paneStrip("${pane}")`);
    }
    const strip = /function paneStrip\([\s\S]*?\n\}/.exec(workspaceSource);
    expect(strip).not.toBeNull();
    expect(strip![0]).toContain("ws-pane-strip");
    expect(strip![0]).toContain("ws-pane-toggle");
    expect(strip![0]).toContain("aria-expanded");
  });

  it("leaves no second copy of the control in the topbar", () => {
    const topbar = /<header class="ws-topbar">[\s\S]*?<\/header>/.exec(workspaceSource);
    expect(topbar).not.toBeNull();
    expect(topbar![0]).not.toContain("ws-pane-toggle");
    expect(workspaceSource).not.toContain("ws-pane-toggles");
    expect(cssSource).not.toContain("ws-pane-toggles");
  });

  it("writes the pane's name as real vertical text, not an image or a rotation", () => {
    expect(workspaceSource).toContain("ws-pane-strip-label");
    const rule = /\.workspace\[data-collapsed~="rail"\] \.ws-rail \.ws-pane-strip-label,\s*\.workspace\[data-collapsed~="detail"\] \.ws-detail \.ws-pane-strip-label \{([^}]*)\}/.exec(
      cssSource,
    );
    expect(rule).not.toBeNull();
    expect(rule![1]).toContain("writing-mode: vertical-rl");
    // A `transform: rotate` would take the text out of the flow and out of
    // selection; a background image would take it out of the accessibility tree.
    expect(rule![1]).not.toContain("rotate");
    expect(rule![1]).not.toContain("background-image");
  });
});
