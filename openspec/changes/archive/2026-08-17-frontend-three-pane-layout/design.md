## Context

`docs/ui.png` is the target: collapsible full-width timeline, left rail (Time / Level / Tags, each
collapsible, counts on every value), center table with its own search toolbar, right inspect pane
reading "Select a log entry to inspect". The current app has all the same information but arranged as
one column, with the table trapped in a fixed 480px viewport.

Grounded in the current code, verified directly:

- `web/src/style.css:543` — `.entry-table-viewport { height: 480px; overflow-y: auto; }`. The virtual
  scroller reads `this.viewportEl.clientHeight` (`looq-entry-table.ts:234`, with a `ROW_HEIGHT * 20`
  fallback), so it follows whatever height CSS gives it; nothing else needs to change for the table to
  fill a pane.
- `looq-app.ts:146` and `looq-live-tail.ts:139` each write their own page markup with `innerHTML`.
  There is no shared layout element today.
- `looq-filter-bar.ts:98` delegates clicks on `chipsEl`, and `renderChips()` reassigns
  `chipsEl.innerHTML` wholesale. `setFieldInventory` is called on every live batch, so under a stream
  the delegation target's children are replaced continuously. The listener is fine; the nodes are not.
- The user's answers to this change's explore round: three panes as in the reference; Level, fields,
  time range and search all in the left rail; every rail section collapsible with primary ones open by
  default; secondary shell surfaces at the bottom of the rail, collapsed; the click bug fixed inside
  this change rather than before it.

## Goals / Non-Goals

**Goals:**

- The log is on screen without scrolling, at the width of the window.
- One layout definition serving both file mode and stdin mode.
- Filter controls that keep working while entries arrive.
- A detail pane that does not push the table around.

**Non-Goals:**

- No change to the predicate semantics, hash grammar, or WASM boundary.
- No new filter features (negation, saved views, starring rows as in the reference's `★` column).
- No column configuration UI, no resizable panes as a first step (fixed rail/detail widths).
- Not a visual restyle: palette, density and typography from the two previous changes stay as they
  are. This change moves things and changes how they re-render.

## Decisions

### D1 — A single `looq-workspace` element owns the layout; the two mode shells mount it

`looq-app` and `looq-live-tail` keep owning their data flow (parse result vs. live session, predicate
state, range) and stop owning page structure. Both render `<looq-workspace>` and place mode-specific
content into named slots: the file-mode drop-zone and the live-mode connection indicator are slot
content, not layout.

*Alternative rejected:* leaving each shell to lay itself out and duplicating the CSS grid in both.
That is the arrangement that let the two modes drift in the first place, and it doubles the cost of
every future layout fix.

### D2 — CSS grid, viewport-bound, with `min-height: 0` on the scrolling panes

```
grid-template-rows:    auto (topbar) auto (timeline) 1fr (panes)
grid-template-columns: 18rem (rail) 1fr (table) 22rem (detail)
```

`body`/`looq-app` get `height: 100dvh` and `overflow: hidden`; each pane scrolls internally. Panes
that scroll need `min-height: 0` / `min-width: 0` or grid's automatic minimum size keeps them from
shrinking and the page scrolls anyway — this is the specific mistake to avoid, and the acceptance
check for it is "the document itself never gets a scrollbar at 1280×800 with 50k entries".

The table's fixed `480px` height is deleted, not overridden; `.entry-table-viewport` becomes
`height: 100%` inside its pane.

### D3 — Native `<details>`/`<summary>` for every collapsible section

Keyboard operable, screen-reader labelled and state-persistent without a line of JavaScript, versus a
button + `aria-expanded` + click handler for each section. The `open` attribute encodes the default:
Search, Level and the first N field sections open; remaining field sections and every secondary
surface closed. Section state is per-session UI state and is deliberately **not** written to the URL
hash — the hash describes the query, not furniture, and adding drawer state to it would change a
shared link's meaning.

### D4 — Reconcile filter controls in place; never rebuild the tree on a live batch

The fix for the click bug. `renderChips()` splits into:

- a **structural pass** that runs only when the *set* of fields or values changes — it adds and
  removes nodes, keyed by `field` + `value`, keeping existing element instances for values already
  present;
- a **count pass** that runs on every inventory update and writes only into each control's count text
  node.

Consequences that are part of the acceptance criteria: a `mousedown`/`mouseup` pair 150ms apart on a
level row toggles the filter while a stream runs; a half-typed value in a high-cardinality field's
input is still there after ten batches arrive; focus is not lost.

A structural change *can* still replace a node the user is pressing (a new `service` value appearing
mid-press), but that is rare and self-limiting, unlike the current unconditional rebuild at the
stream's frequency.

*Alternative rejected:* debouncing the re-render (say, at most once a second). It narrows the window
without closing it — the click still fails whenever it lands on the wrong side of the tick — and it
makes counts lag for no benefit.

### D5 — Level as full-width colored rows; fields as value lists inside their sections

The reference's left rail uses one full-width row per level with the count right-aligned. Level keeps
the six colors already defined by `theming`'s "every level has its own color" requirement. Field
sections keep the existing value list with counts, plus the existing typed-value input for
high-cardinality fields, now inside a collapsible section per field. Chip semantics are unchanged:
same `field`/`value` toggles, same OR-within/AND-across rule, same events out.

### D6 — The detail pane is persistent and states its empty case

Right pane always exists; with no selection it renders "Select an entry to inspect" (the reference's
wording, matching `error-states`' habit of making empty states explicit rather than blank). Selection
is held by ordinal, so live growth and eviction do not silently swap what is being inspected: if the
inspected entry is evicted, the pane says so rather than showing a different entry's fields.

### D7 — Collapsed diagnostics must report severity on the summary line

Collapsing the diagnostics surface conflicts with `app-shell`'s "Parser diagnostics reach the user"
requirement unless the collapsed header itself carries the information. The summary therefore shows
the skip count and the same severity treatment the expanded panel uses (`0 skipped` when clean; the
count plus the warning styling when not), and the section auto-opens on first render when the skip
ratio crosses the existing severity threshold. Detection gets the same treatment: a fallback or
low-confidence detection is visible on the collapsed summary, not only inside it.

### D8 — Narrow viewports stack instead of squeezing

Below ~1100px the grid collapses to a single column: timeline, rail (collapsed), table, detail below
the table. This is a fallback for a small window, not a mobile design — the tool's stated environment
is a developer's desktop browser.

## Risks / Trade-offs

- **The rework touches the component with the most measured behavior.** `looq-entry-table` carries the
  50k-row/17ms-per-frame result from `timeline-and-table`; moving its detail rendering out and letting
  its height come from the layout must not regress that. Re-measure with the same fixture and record
  the number, per this repo's benchmark rule.
- **Two shells, one layout, two verification passes.** Every acceptance check runs in both file mode
  and stdin mode, because the whole point of D1 is that they no longer differ.
- **Fixed rail/detail widths** will feel wrong to someone with a very wide or very narrow window;
  resizable panes are deliberately deferred rather than half-built.

## Open Questions

None outstanding — the four scoping questions (pane count, rail contents, where the secondary
surfaces go, when the click bug gets fixed) were answered in this change's explore round and are
recorded in D1–D8.
