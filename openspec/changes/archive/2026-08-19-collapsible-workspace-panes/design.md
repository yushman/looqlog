## Context

`.workspace` is a CSS grid: `grid-template-columns: 18rem minmax(0, 1fr) 22rem`, three rows
(topbar, timeline, panes). Below `max-width: 1100px` it collapses to a single column and the
panes stack, and that breakpoint is duplicated in `NARROW_LAYOUT_QUERY` in
`looq-workspace.ts`, where it also makes the rail's sections start collapsed. The comment in
`style.css` already warns that the two must stay in sync; this change adds a third consumer of
the same breakpoint and must not make that worse.

The rail is not an inert container. It holds format detection, parser diagnostics, the privacy
note and the shareable-link action, and `app-shell`'s "Secondary surfaces are collapsible
without hiding warnings" requires each of those to keep stating its status while collapsed and
to expand itself when something must not be missed. That requirement was written about
`<details>` sections inside a pane that was always present.

## Goals / Non-Goals

**Goals:**
- Either side pane can be put away and brought back, in both layouts.
- A collapsed pane cannot hide a warning that an expanded one would have shown.
- The collapsed state travels in a shared link like every other view preference.

**Non-Goals:**
- Resizing the panes. Separate parked item; collapsing is the ask.
- Reordering panes, or moving a surface from one pane to another.
- A mobile layout. The narrow stacked layout stays what it is — a fallback for a narrow
  desktop window (`frontend-three-pane-layout` D8), not a phone design.
- Animating the collapse.

## Decisions

### D1: The control lives on the pane, and collapsing leaves a labelled strip

Each pane carries its own icon button. Collapsing does not take the pane to zero: it leaves a
narrow vertical strip — the reopen button plus the pane's name written vertically ("Filters",
"Details") — and gives the rest of the width to the table.

**This decision was reversed after the first implementation was seen running.** The original D1
put both toggles in the topbar and collapsed the pane's grid track to exactly `0`, reasoning
that a strip costs permanent width, that a pane spends most of its time collapsed and would
need a second visual design for that state, and that a reopen control should never live inside
the thing it reopens. That version was built, verified and rejected on sight. Recorded rather
than quietly rewritten, because the argument was not wrong on its own terms — it optimised for
reclaimed pixels, and the call was that discoverability wins: a control that belongs to the
pane is where a user looks for it, and a labelled strip says what is hidden and how to get it
back without anyone having to learn a topbar.

Consequences that follow from the reversal and are not optional:

- A collapsed pane is no longer `visibility: hidden` wholesale. The strip must stay visible and
  focusable — it is now the only way back — while everything else in the pane must stay out of
  the tab order. The current implementation hides the whole pane, which would make the strip
  unreachable.
- The attention indicator from D3 moves onto the strip's button. This is strictly better than
  the topbar badge it replaces: the warning now sits on the collapsed pane itself, which is the
  thing the user would look at.
- The strip has a width, so "collapsed" is no longer a zero track. The grid template carries a
  strip width instead, and the table's gain is `pane − strip` rather than the whole pane.
- The vertical label needs `writing-mode: vertical-rl`; it must remain selectable text, not an
  image or a rotated background, so it stays legible to assistive technology and at any zoom.

### D2: Collapse changes the grid template, not the panes' own display

Collapsing sets `.workspace`'s template to `0 minmax(0, 1fr) 22rem`, `18rem minmax(0, 1fr) 0`
or `0 minmax(0, 1fr) 0`, driven by two custom properties. The panes themselves keep their
`overflow` and `min-width: 0`; they simply have no width.

Setting `display: none` on the pane instead would work, but it destroys and recreates layout
and scroll position on every toggle, and the rail's scroll position is worth keeping across an
accidental collapse. A zero track keeps the element in the tree with its scroll offset intact.

Note the pane must also stop being focusable when collapsed — a zero-width pane still contains
tabbable controls, and tabbing into an invisible filter list is the accessibility version of
the same bug this change exists to avoid.

### D3: The no-hidden-warnings rule is lifted from sections to panes

This is the decision that makes the change more than CSS. `app-shell` currently guarantees that
a severe skip ratio expands the diagnostics section on its own, and that a fallback detection is
readable on the collapsed summary. Collapse the rail and both guarantees evaporate — the section
is still "expanded", inside a pane of zero width.

So:
- The rail's toggle carries an indicator when any surface inside it is in a state the existing
  requirement calls out (skipped lines, fallback or low-confidence detection).
- A condition severe enough to auto-expand a section also auto-expands the pane containing it.

Alternative considered: forbid collapsing the rail while a warning is live. Rejected — it makes
the control unpredictable ("why is this button dead?") and punishes the user for the log's
contents. Surfacing on the toggle keeps the user in charge while keeping the guarantee.

### D4: Selecting a row expands the detail pane

A selection whose only visible effect is inside a collapsed pane looks like a broken click —
the same class of failure as the row-selection bug `entry-table` already carries a requirement
about. Selecting expands. Collapsing the detail pane does not clear the selection, so
re-expanding shows what was already selected.

### D5: In the stacked layout, collapse hides the block

Below 1100px there are no side columns to zero out. Collapse hides the corresponding stacked
block instead, and the toggles keep their meaning ("show/hide the filters", "show/hide the
detail"). The same state serialises the same way in both layouts, so a link shared from a wide
window applies sensibly in a narrow one.

The breakpoint is now consumed by `style.css`, `NARROW_LAYOUT_QUERY` and this logic. It gets
defined once and imported rather than written a third time.

### D6: `panes=` in the hash, absent meaning both expanded

The grammar gains `panes=<rail|detail|rail+detail>` naming what is *collapsed*, absent meaning
nothing is. Naming the collapsed set rather than the expanded one keeps the common case out of
the URL entirely, the same way default column widths are omitted.

An unrecognised pane name is reported with the other unapplied hash parts and ignored — a
presentation preference must never stop a log from loading, exactly as `url-state` already
requires for column widths.

## Risks / Trade-offs

- **A collapsed rail hides a warning** → D3, which is the reason this change touches behavior
  and not only layout. The scenarios for it are in the `app-shell` delta and are the ones to
  verify first.
- **Tabbing into a collapsed pane** → D2: collapsed panes are removed from the tab order, and
  that is asserted, not assumed.
- **The 1100px breakpoint drifts across three definitions** → D5 defines it once and imports
  it; the existing comment in `style.css` warning about two copies gets updated rather than
  left describing two.
- **A shared link now carries layout state** → collapsed panes are not log content, so the
  existing sharing caveat is unaffected.
- **Interaction with `resizable-table-columns`** → both modify the same `url-state`
  requirement. Archive order is stated in the proposal; getting it wrong drops `cols=` from
  the main spec while still validating.

## Open Questions

- Whether collapsing both panes should get a single shortcut ("focus mode"). Two toggles cover
  it; a third control that does what two already do is worth having only if the pair turns out
  to be used together in practice.
