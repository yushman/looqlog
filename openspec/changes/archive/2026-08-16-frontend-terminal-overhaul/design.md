## Context

`frontend-visual-redesign` (archived 2026-08-16) shipped the palette and density pass and explicitly
deferred typography and markup changes. Its own design.md D7 asked whether monospace should extend
to UI chrome and left it unresolved after two unanswered rounds; the user has now answered it
directly ("шрифт хочется более айтишный" — a more IT/technical feel). This change is the deferred
"approach C" from the original brainstorm: markup-level polish within the existing page structure,
not a layout rebuild.

Grounded in the current code (verified directly, not assumed): `.topbar` (`web/src/style.css:141`)
has `background: var(--bg-elevated)` from the prior change but no padding, so it currently renders
as a tight background patch around the `<h1>looq</h1>` + theme-toggle button rather than a visible
strip — `frontend-visual-redesign` added the token but never gave the element room to show it. The
monospace stack `ui-monospace, SFMono-Regular, Menlo, monospace` is a literal string duplicated
across `.status`, `.conn-indicator`, the entry-table row styles, the detail panel, and diagnostics —
six-plus copies before this change, more if left unaddressed while extending it to `body`.

## Goals / Non-Goals

**Goals:**
- Monospace typography everywhere, via one shared token instead of copy-pasted strings, with a font
  stack that prefers common developer-installed fonts before falling back to what's guaranteed
  present.
- Level indicators in the entry table read as compact, colored, and immediately scannable — closer
  to the reference's feel — without losing the actual level name for anyone who needs it read as
  text (accessibility, tooltips).
- The top bar reads as an actual strip of app chrome with a deliberate accent, fixing the padding gap
  left by the prior change.

**Non-Goals:**
- No layout restructuring. Detail panel, filter bar, and table positions are unchanged.
- No change to filter-chip markup — level and field-value chips keep full text; they're controls a
  user reads before clicking, not a dense repeated display like the table's level column.
- No change to the 24px table row height, the level vocabulary, or any behavioral requirement outside
  the one accessibility guarantee named below.
- No `@font-face`/network font loading — `theming`'s "No external resources" requirement is
  unaffected; the broadened font stack only reorders preference among already-installed system fonts.

## Decisions

### D1 — `--font-mono` token, applied to `body`, duplicate declarations removed

Add one token (same value light and dark — a font stack isn't a themeable color, so it lives once,
not duplicated per appearance block):

```css
--font-mono: ui-monospace, "JetBrains Mono", "Cascadia Code", "SF Mono", Menlo, Consolas,
  "Liberation Mono", monospace;
```

`ui-monospace` stays first because it's the browser's own "give me the system's default monospace"
keyword and already resolves to a good pick (SF Mono on macOS, Cascadia/Consolas on Windows) — the
named fonts after it only matter on platforms where `ui-monospace` isn't supported (older
non-Chromium/Safari browsers) or where a user has one of those specific fonts installed and the
browser's `ui-monospace` resolution doesn't already pick it up. This is a strict broadening, not a
replacement: every browser that handled the old stack still gets the same practical result, and
nothing here can silently fail differently across the light/dark boundary since it's one token, not
two.

`body`'s `font-family` switches from `system-ui, -apple-system, sans-serif` to `var(--font-mono)`.
The six-plus per-rule `font-family: ui-monospace, SFMono-Regular, Menlo, monospace;` declarations
(`.status`, `.conn-indicator`, entry-row, detail panel, diagnostics — exact list in `tasks.md`) are
deleted; they now inherit from `body`. This is a real de-duplication, not incidental to the font
change — six copies of the same literal string is what "extend it to body" turns from "one more
copy" into "delete the other six."

### D2 — Label typography: uppercase + letter-spacing on field-name labels

`.filter-field-name` (currently `font-weight: bold; font-size: 0.85em; color: var(--muted);
min-width: 5em;`) adds `text-transform: uppercase; letter-spacing: 0.04em;`. This is the one
typographic cue borrowed from the reference's sidebar section labels ("Level", "Tags") without
adopting the sidebar itself — a small, cheap signal that reads as "technical UI" without touching
layout. Scoped narrowly to this one class rather than sprinkled onto arbitrary text, so it stays a
deliberate label convention, not decoration applied inconsistently.

### D3 — Level badges: single-letter circles, entry table only

Current: `<span class="level-badge level-trace">TRACE</span>`, styled as a text pill
(`padding: 0 0.4em; border-radius: 3px;`, `.entry-row`'s inherited monospace, colored via the
`--level-*`/`-bg` tokens from the prior change).

New: `<span class="level-badge level-trace" aria-label="TRACE" title="TRACE">T</span>` — visible
text becomes the first letter (T/D/I/W/E/F, confirmed unique across all six levels), `aria-label`
carries the full word for assistive technology, `title` gives sighted users a hover tooltip. CSS
reworks `.level-badge` from a padded pill to a fixed-size circle:

```css
.level-badge {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 1.6em;
  height: 1.6em;
  border-radius: 50%;
  font-weight: bold;
}
```

(`padding: 0 0.4em` removed — a fixed-size circle sizes itself, padding would fight the centering.)
The six `.level-badge.level-*` color rules (`background`/`color` from the `--level-*` token pairs)
are unchanged — this is a shape and text-content change, not a recoloring.

**Why the table only, not the filter chips too:** the reference itself does this — its sidebar
checkboxes show full words ("Verbose", "Debug", …), only the table's own level column uses a compact
per-row treatment. A filter chip is clicked, not scanned across fifty rows at once; collapsing
"ERROR" to "E" on a button someone has to correctly identify before clicking trades a real usability
cost for a compression benefit that only pays off at high repetition. The entry table is exactly the
high-repetition context (one badge per row, potentially thousands of rows) where the trade is worth
it, and it's also the one place `entry-table`'s existing "Levels SHALL be visually distinguishable"
requirement actually lives — see the delta in `specs/entry-table/spec.md`.

**Accessibility, stated explicitly because it's easy to get wrong silently:** dropping to one letter
without `aria-label` would make a screen reader announce "T" instead of "TRACE" — technically still
"distinguishable" by the letter of the current requirement, but a real regression in practice. The
`aria-label`/`title` pair is not decoration, it's the thing that keeps this change from being a
silent accessibility regression wearing a compliant-looking diff.

### D4 — Top bar: padding fix + accent border + accent mark

`.topbar` gains real padding (proposed `0.5rem 0.75rem`, matching the density scale
`frontend-visual-redesign` already established for other containers) so `--bg-elevated` actually
reads as a strip instead of a tight patch — this is completing what the prior change started, not a
new idea. Add `border-bottom: 2px solid var(--accent);` as the accent treatment. Add a small
accent-colored mark before the `<h1>looq</h1>` text — a `::before` pseudo-element or a small
`<span class="brand-mark">` in `index.html`, a filled square or dot roughly `0.6em` square, colored
`var(--accent)` — a lightweight echo of the reference's colored logo corner, not an attempt to
recreate a browser-chrome mockup.

This stays inside the existing centered `max-width: 900px` body layout — no full-bleed/edge-to-edge
treatment. Going full-bleed would mean pulling `.topbar` outside the body's margin/padding flow,
which is a real layout change and out of scope per the Non-Goals above; the padding+border+mark
combination delivers "reads as chrome" without it.

## Risks / Trade-offs

- **Single-letter badges could still read as too compressed for a first-time user** — mitigated by
  the `title` tooltip and by the color-coding carrying most of the recognition weight after the first
  few rows (this is exactly how the reference itself reads); not verified against a real first-time
  user, only against the accessibility and distinguishability requirements.
- **Broadened font stack lists fonts most users won't have installed** — harmless (browsers skip
  unavailable names silently) but worth being honest that "JetBrains Mono"/"Cascadia Code" are
  aspirational entries for developers who already have them, not a guarantee most users see anything
  different from today's `ui-monospace` resolution.
- **Removing the six duplicate `font-family` declarations is a real edit surface** — touches several
  unrelated-looking CSS rules in one change; mitigated by doing it as a mechanical find-and-delete
  against `var(--font-mono)` inheritance, verified by confirming computed style is unchanged for
  every element that previously had an explicit declaration.
- **`.topbar`'s padding fix changes vertical rhythm at the top of the page** — small, but should be
  eyeballed in both themes since it's the very first thing rendered; not verified as part of this
  design.

## Migration Plan

Direct CSS/markup edit to `web/src/style.css`, `web/src/components/looq-entry-table.ts`, and
`web/index.html`. No data migration, no feature flag. Rollback is a revert of the same files.

## Open Questions

None left open by design — the two questions the prior change deferred (typography scope, and
whether to pursue "approach C") were both resolved directly by the user before this change was
written. Implementation-time judgment calls (exact accent-mark size/shape, exact `.topbar` padding
value if `0.5rem 0.75rem` reads wrong against the accent border) are implementation's to make and
report, not open questions to re-litigate.
