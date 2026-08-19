// Source-level assertions about the entry table's styling mechanism.
//
// These are deliberately checks on the *source text*, not on a rendered table:
// `web/` has no DOM test environment (no `jsdom` dependency, and adding one is out
// of scope for this change), and the properties at stake — "the scroller writes no
// inline style", "the row height in the stylesheet matches the scroller's
// arithmetic" — are exactly the kind of thing that regresses silently in a code
// review and then only shows up as a blank table under a strict CSP. The
// browser-side half of the same guarantee is task 5.10 in this change's tasks.md:
// load a real file under the strict policy — SERVED on the response, not injected
// as a `<meta>` into a loaded page — and confirm zero console violations.

// Sources are pulled in with Vite's `?raw` (typed by `vite/client`) rather than
// `node:fs`, which would need `@types/node` — a dependency this change is not
// entitled to add.
import cssSource from "./style.css?raw";
import tableSource from "./components/looq-entry-table.ts?raw";

import { describe, expect, it } from "vitest";

/** Comments in this file name the very APIs the assertions below forbid (that is
 * what a comment explaining a decision looks like), so the code-shape checks run
 * against the code only. Crude on purpose — a `//` inside a string literal would
 * be mangled, and there is none in the file this reads. */
function codeOnly(source: string): string {
  return source.replace(/\/\*[\s\S]*?\*\//g, "").replace(/^\s*\/\/.*$/gm, "");
}

const tableCode = codeOnly(tableSource);

describe("the virtual scroller writes no inline styles (`entry-table` spec, `security` spec)", () => {
  it("never assigns to an element's `style` property", () => {
    // Every `.style` in this file must belong to a `CSSStyleRule` (the D2/D4
    // mechanism), never to an element — `el.style.height = …` is the inline write
    // that forced `'unsafe-inline'` onto `style-src` in the first place.
    const receivers = [...tableCode.matchAll(/(\w+)\.style\b/g)].map((m) => m[1]);
    expect(receivers.length).toBeGreaterThan(0); // the mechanism is still there at all
    expect(receivers.filter((name) => !/rule$/i.test(name!))).toEqual([]);
  });

  it("never sets a `style` attribute", () => {
    expect(tableCode).not.toMatch(/setAttribute\(\s*["']style["']/);
    expect(tableCode).not.toMatch(/style\s*=\s*["'`]/);
  });

  it("uses insertRule + rule mutation, not adoptedStyleSheets and not per-frame reparsing", () => {
    // Safari shipped `adoptedStyleSheets` in 16.4 while PRD §11 targets Safari 16+,
    // so 16.0–16.3 would get a table that never positions a row (design D2).
    expect(tableCode).not.toContain("adoptedStyleSheets");
    // `replaceSync`/`cssText` would re-parse the whole stylesheet on every scroll
    // event instead of mutating the existing declaration (design D4).
    expect(tableCode).not.toContain("replaceSync");
    expect(tableCode).not.toContain("cssText");
    expect(tableCode).toContain("insertRule");
  });

  it("inserts into the already-linked application stylesheet, creating no element", () => {
    // `style-src` gates the *insertion of a `<style>` element*, not the presence of
    // text inside it: appending an empty one was refused under the served strict
    // policy, and Chrome named the SHA-256 of the empty string as the hash it
    // wanted (design D2, tasks 5.4/5.7). `/assets/index.css` is already loaded via
    // `<link rel="stylesheet">`, so `style-src 'self'` permits it and no new
    // element exists to be gated.
    expect(tableCode).not.toMatch(/createElement\(\s*["']style["']/);
    expect(tableCode).toContain("document.styleSheets");
    expect(tableCode).toContain("/assets/index.css");
  });

  it("fails loud when the stylesheet is missing, instead of returning silently", () => {
    // The soft `if (!sheet) return` is what turned a blocked stylesheet into a
    // table that showed 36 rows of 50,000 and scrolled nowhere — CLAUDE.md's
    // silent-failure list (task 5.8). A missing stylesheet must throw AND be
    // reported to the user, because the scroller cannot position a row without it.
    const create = /private createStyleSheet\([\s\S]*?\n  \}/.exec(tableCode);
    expect(create).not.toBeNull();
    expect(create![0]).toContain("throw new Error");
    expect(create![0]).toContain("reportFatal");
    expect(create![0]).not.toMatch(/if \(!sheet\) \{\s*return;/);
  });

  it("removes its rules from the shared stylesheet when it disconnects", () => {
    // The rules live in a sheet this component does not own, so a remount that
    // left them behind would accumulate dead rules in `/assets/index.css`.
    expect(tableCode).toContain("deleteRule");
  });
});

describe("the row height in style.css matches ROW_HEIGHT (design D3)", () => {
  it("agrees, so the scroller's arithmetic and the stylesheet cannot drift", () => {
    const constant = /export const ROW_HEIGHT = (\d+);/.exec(tableSource);
    expect(constant).not.toBeNull();
    const rule = /\.entry-row \{[^}]*?height:\s*(\d+)px;/s.exec(cssSource);
    expect(rule).not.toBeNull();
    expect(rule![1]).toBe(constant![1]);
  });
});

describe("the grid template is variable-driven (design D1)", () => {
  it("drives the three resizable columns from custom properties", () => {
    expect(cssSource).toContain("var(--col-ordinal");
    expect(cssSource).toContain("var(--col-timestamp");
    expect(cssSource).toContain("var(--col-level");
  });

  it("leaves the message column as the remainder, not a fourth resizable width", () => {
    // `minmax(0, 1fr)` is what makes a row exactly fill the viewport whatever the
    // other three are set to — and why the message column has no drag handle and no
    // place in the `cols=` hash grammar.
    expect(cssSource).toMatch(/grid-template-columns:[^;]*minmax\(0, 1fr\)/);
    expect(cssSource).not.toContain("--col-message");
  });
});
