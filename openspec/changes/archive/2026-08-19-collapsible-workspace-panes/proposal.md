## Why

The workspace is a fixed three-column grid — `18rem` filter rail, the table, `22rem` detail
pane — and the side panes cannot be put away. On a 13MB Android bugreport the table is where
all the work happens and 40rem of the window is permanently spent on two panes the user is not
reading. The rail's individual sections already collapse; the panes containing them do not.

`resizable-table-columns` gave the table adjustable columns. This is the same complaint one
level up: the layout is a decision made once, for everyone, and the user cannot change it.

## What Changes

- The filter rail and the detail pane can each be collapsed away entirely, giving their width
  to the table.
- The toggles live in the topbar, which is always visible in both the side-by-side and the
  narrow stacked layout — a collapsed pane is therefore always reopenable, with no reliance on
  a sliver of the pane itself remaining on screen.
- Collapsed state round-trips through the URL hash alongside the range, filters, query, format,
  timezone and column widths.
- **A collapsed pane must not hide a warning.** `app-shell` already requires that a collapsed
  *section* keep stating its status and open itself for something the user must not miss.
  Collapsing the whole rail would swallow the diagnostics and detection surfaces along with
  that guarantee, so the same rule is lifted to the pane level: the rail's toggle carries an
  indicator when a surface inside it needs attention, and a condition severe enough to expand a
  section on its own also expands the pane containing it.
- Selecting a row while the detail pane is collapsed expands it, so a selection never appears
  to do nothing.
- In the narrow stacked layout (below the existing 1100px breakpoint) collapsing hides the
  stacked block rather than a grid column, and the toggles keep working.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `app-shell`: the three-pane layout gains collapsible side panes, and the no-hidden-warnings
  rule extends from sections to panes.
- `url-state`: the hash grammar gains the collapsed state.

## Impact

- `web/src/components/looq-workspace.ts` — the toggles, the collapsed state, and the
  interaction with `NARROW_LAYOUT_QUERY`.
- `web/src/style.css` — `.workspace`'s grid template becomes collapse-aware; the `max-width:
  1100px` block must stay consistent with it.
- `web/src/url-hash.ts` — grammar, parsing and serialisation.
- `web/src/components/looq-entry-detail.ts` and whichever surface owns diagnostics severity —
  the auto-expand path.
- Both READMEs.

**Sequencing note:** this change modifies `url-state`'s "Hash grammar" requirement, and so does
`resizable-table-columns`. This change's delta is written against the text that already carries
`cols=`, so `resizable-table-columns` MUST be archived first. Archiving them in the other order
would drop `cols=` from the main spec silently — the delta would still validate.

Out of scope: resizing the panes (a separate parked item), reordering them, and collapsing the
timeline (already a `<details>` that collapses on its own).
