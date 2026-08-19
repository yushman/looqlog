## Why

The entry table's four columns are fixed at `3rem 12.5rem 2.5rem minmax(0, 1fr)`, chosen
when the only formats were JSON, logfmt and plain text. Real files have since made those
widths wrong in both directions: an Android bugreport's messages are long enough that the
12.5rem timestamp column is wasted space on 78% of its rows (they have no timestamp at all),
while a file of long ISO timestamps with short messages wants the opposite. The user cannot
adjust either.

The fixed widths also just produced a visible defect — the 2.5rem level column, sized for a
badge, rendered the eight-character string `no level` and painted it over the message text.
That was fixed at the source (an em dash, plus clipping on every column) but it is the same
root cause: column widths are a guess made once, for everyone, about every log.

## What Changes

- Each column boundary in the entry table gets a drag handle. Dragging resizes the column to
  its left; the message column absorbs the remainder so the row always fills the viewport.
- Widths persist for the session and round-trip through the URL hash, so a shared link
  reproduces the view its author was looking at — consistent with how filters, search, the
  time range, the format override and the timezone already behave.
- A double-click on a handle resets that column to its default width; a control resets all
  of them.
- Columns have a minimum width so a column cannot be dragged to zero and become unreachable.
- **Row positioning moves off inline `style` attributes onto generated stylesheet rules.**
  The virtual scroller currently writes `style.height` and `style.transform` directly, which
  is why the served CSP carries `'unsafe-inline'` on `style-src`. Column widths would have
  added a third inline write per row per frame; instead both move to a single
  `CSSStyleSheet` whose rules are rewritten on resize, and `style-src` drops back to bare
  `'self'`.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `entry-table`: columns gain user-adjustable widths with a floor and a reset.
- `url-state`: the hash grammar gains column widths.
- `security`: `style-src` no longer needs `'unsafe-inline'`.

## Impact

- `web/src/components/looq-entry-table.ts` — drag handles, the width model, and the move from
  inline styles to a generated stylesheet.
- `web/src/style.css` — the grid template becomes variable-driven; handle styling.
- `web/src/url-hash.ts` — grammar, parsing and serialisation for widths.
- `crates/looq/src/server.rs` — the CSP header drops `'unsafe-inline'` from `style-src`, and
  the comment explaining why it was there is removed rather than left describing a state that
  no longer exists.
- `crates/looq/tests/cli.rs` — the CSP assertion.
- Performance: the table renders 50,000 rows at ~17ms/frame today (`timeline-and-table`), and
  row positioning is the hot path this change rewrites. That number is the gate.
- Both READMEs.

Out of scope: reordering or hiding columns, per-column sorting, and a column for arbitrary
extracted fields. Resizing is the ask; the rest is a different feature with its own questions.
