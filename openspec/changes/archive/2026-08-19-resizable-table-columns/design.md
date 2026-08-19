## Context

The entry table is a virtual scroller: one `.entry-row` div per *visible* row, each its own
CSS grid, positioned by translating a container. Rows are reused rather than rebuilt, because
replacing `innerHTML` mid-interaction detached the node under the pointer and silently broke
row selection under a live stream (`entry-table` spec, "Rows stay selectable while entries
arrive").

Reading the code rather than the CSP comment that describes it, there are exactly **three**
inline-style writes, not one per row per frame:

| Site | Frequency |
|---|---|
| `spacerEl.style.height` | once per render |
| `rowsEl.style.transform` | once per render |
| `el.style.height` in `createRowElement` | once per row **creation** |

That matters: the devlog parked this rework as "larger, riskier" on the assumption it was
per-row. It is two per-frame writes on container elements plus a constant that does not need
to be inline at all. Column widths would have added a third per-frame write, which is the
reason to do both together rather than bolt widths onto the inline path and make the CSP
concession permanent.

Because every row is its own grid, all rows must be driven by **one** shared template — a
per-row width would not line up with its neighbours, which is why the current template is
fixed rather than content-based.

## Goals / Non-Goals

**Goals:**
- Drag a boundary, the column resizes, every row stays aligned.
- Widths survive a reload and travel in a shared link.
- No column can be dragged to unusability.
- `style-src` drops `'unsafe-inline'`.
- No regression against the measured 50,000-row / ~17ms-per-frame result.

**Non-Goals:**
- Reordering, hiding, or adding columns; per-column sorting.
- Per-column widths that differ between file mode and stdin mode.
- Remembering widths across sessions in `localStorage` — the URL hash is this project's
  existing state-persistence mechanism and adding a second one needs its own argument.

## Decisions

### D1: One CSS custom property per column, on a rule the JS owns

The grid template becomes
`grid-template-columns: var(--col-ordinal) var(--col-timestamp) var(--col-level) minmax(0, 1fr)`,
with the four variables defined on a single `.entry-table-wrap` rule that JavaScript rewrites.
The message column stays `1fr` and is not directly resizable — it absorbs whatever the other
three leave, so the row always exactly fills the viewport and there is no horizontal scrollbar
to reconcile.

Alternative considered: writing `style.gridTemplateColumns` on each row. Rejected on both
counts that matter here — it is an inline style (the thing this change removes) and it is a
per-row write in the hot path.

### D2: Rules are inserted into the application's own linked stylesheet

The component finds `/assets/index.css` — already loaded through the `<link rel="stylesheet">`
that `style-src 'self'` permits — in `document.styleSheets`, and calls `insertRule` on it. No
new element is created at all, and every subsequent width or offset change mutates the
resulting `CSSStyleRule` through the object model.

**This decision replaces a wrong one, and the reason is worth keeping.** The original D2 called
for `document.createElement("style")` with no text content, on the premise that "an empty
`<style>` element has no content to parse, so nothing is blocked". That premise is false.
`style-src` gates the *insertion of the element*, not the parsing of its content: Chrome
refused it under `style-src 'self'` and named the hash it wanted —
`sha256-47DEQpj8HBSa+/TImW+5JCeuQeRkm5NMpJWZG3hSuFU=`, which is the SHA-256 of the **empty
string**. `styleEl.sheet` came back `null`, the three rules were never created, and
`renderVisibleRows` silently skipped both writes: the table rendered 36 rows of 50,000, with a
`scrollHeight` of 864 instead of 1,200,000.

Two things this cost us, both worth remembering:

- The dry run that "proved" the mechanism (task 2.4) injected a `<meta>` policy into an
  *already-loaded* page. That exercises CSSOM writes, which genuinely are not CSP-gated, and
  never exercises element insertion — it tested the half that was never in doubt. A policy has
  to be served on the response to test what a served policy does.
- `createStyleSheet` failing soft (`if (!sheet) return`) turned a blocked insertion into a
  table that quietly showed the first screenful and no more. Under the chosen mechanism the
  same guard should fail loud, because "the stylesheet is missing" is not a survivable state.

Alternatives, both probed live against the served strict header before choosing:
`new CSSStyleSheet()` + `adoptedStyleSheets` works, but Safari shipped it in **16.4** while
PRD §11 targets **Safari 16+**, so 16.0–16.3 would need a second code path that this
environment cannot test; allowlisting the empty-string hash also works, but it is a permanent
CSP concession bought to keep a mechanism that has a concession-free alternative.

The trade-off accepted here is that the component writes into a stylesheet it does not own.
Its rules are scoped per instance (`looq-entry-table[data-looq-table="tN"]`), and they are
appended rather than inserted at an index, so they cannot disturb the authored rules' cascade.

### D3: Row height stops being inline entirely

`ROW_HEIGHT` is a constant. `createRowElement` writing it as an inline style is the easiest of
the three sites to remove: it becomes a plain `.entry-row { height: … }` declaration in
`style.css`, with the constant exported so the scroller's arithmetic and the stylesheet cannot
drift apart. The remaining two sites — spacer height and the container translate — become two
rules in the D2 stylesheet, rewritten per frame.

### D4: Per-frame rule mutation is the risk, and is measured, not assumed

Rewriting a rule every frame is not obviously as cheap as assigning `style.transform`. Mutating
an existing `CSSStyleDeclaration` (`rule.style.transform = …`) avoids re-parsing a whole
stylesheet the way `replaceSync` would, and is the form to use — but "avoids the obviously slow
thing" is not a measurement.

The gate is the existing 50,000-row / ~17ms-per-frame figure from `timeline-and-table`,
re-measured the same way. If per-frame rule mutation cannot hold it, the fallback is to keep
the two container writes inline (and the CSP concession with them) while still taking column
widths off the inline path — a worse outcome that must be reported, not silently chosen.

### D5: A minimum width, and a reset

Each resizable column has a floor (the ordinal and level columns are narrow by nature, so the
floor is per-column, not global). Dragging below it clamps rather than refuses, so the handle
never appears stuck.

Reset works at two levels: double-clicking a handle restores that column's default, and a
control restores all of them. Without a reset, a user who drags a column to its floor and reloads a shared
link has no way back except editing the URL — the same trap the format override has today, and
not one worth reproducing.

### D6: Widths in the hash are defaults-relative and tolerant

The hash gains `cols=<ordinal>,<timestamp>,<level>` in `rem`. Absent means defaults; a
malformed or out-of-range value applies the default for that column and reports what it could
not apply, exactly as `url-state` already requires for every other key ("A malformed hash is
reported, not ignored"). Widths are a presentation preference, so a bad one must never be a
reason to fail loading a log.

The message column is not in the grammar — it is derived, per D1.

## Risks / Trade-offs

- **Per-frame rule mutation is slower than inline style writes** → D4: measured against the
  50k/~17ms baseline before the change is considered done, with an explicit fallback that is
  reported rather than quietly taken.
- **Safari 16.0–16.3 lacks `adoptedStyleSheets`** → D2 inserts into the already-linked
  `/assets/index.css` instead; `CSSStyleSheet.insertRule` predates every browser in the PRD §11
  range, so there is no second code path. This sandbox cannot drive Safari interactively; that
  stays a recorded environment limitation, as it already is for the CSP work.
- **Dropping `'unsafe-inline'` breaks the page if any inline style survives** → the CSP change
  lands only after a full run with the strict header produces zero console violations. The
  previous attempt produced 144 violations and a table that rendered nothing, which is the
  failure mode to check for, and `crates/looq/tests/cli.rs` asserts the header itself.
- **Drag handles interfere with row selection** → handles live in the header row only, not in
  data rows, so a drag can never start on a row the user meant to click.
- **A shared link now carries layout as well as data** → widths are not log content, so the
  existing sharing caveat does not get worse.

## Open Questions

- Whether the ordinal column should be resizable at all, or simply auto-size to the widest
  ordinal present. Auto-sizing is arguably better behaviour and is one fewer handle, but it
  reintroduces content-dependent width on a per-row grid, which is what the fixed template was
  chosen to avoid. Left resizable for now.
