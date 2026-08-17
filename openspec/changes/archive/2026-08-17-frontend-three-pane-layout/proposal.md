## Why

The page is one vertical stack: drop-zone, privacy copy, status line, detection, diagnostics,
filter bar, copy-link button, timeline, then the table — whose viewport is a fixed `height: 480px`
box (`web/src/style.css:543`). The log itself, the reason the tool exists, starts below a screenful
of chrome and is read through a small window while the page scrolls around it. The reference in
`docs/ui.png` is the opposite arrangement: a full-width timeline on top, a filter rail on the left,
the log filling the rest of the width, and a detail pane on the right. Every previous frontend change
(`frontend-visual-redesign`, `frontend-terminal-overhaul`) explicitly kept the stack and touched only
palette, density, typography and badge markup; this is the structural change both of them deferred.

Two things were verified against the running binary before writing this, not assumed:

**Filter clicks do not work during a live stream, and the cause is not the filter code.** In file
mode a click on a level chip works (hash becomes `#filter=level%3DERROR`, 57 of 400 entries, only
ERROR rows). Under a stdin stream at ~25 lines/sec the same click does nothing at all.
`renderChips()` rebuilds the whole chip container through `innerHTML` on every live batch, so the
button a user pressed is detached before they release the mouse; the browser only synthesises a
`click` when `mousedown` and `mouseup` land on the same node. Measured: 120ms after pressing,
`document.contains(chip)` is `false`, and a press-and-release held for 150ms — an ordinary human
click — changes neither the hash nor the selection. Playwright's click passes only because it takes
under a millisecond. The same wholesale re-render also discards the typed-value input for
high-cardinality fields while it is being typed into.

**The layout is built twice.** `looq-app` composes the file-mode shell and `looq-live-tail` composes
the stdin shell, each mounting the same components in its own markup. A layout change made in one
place is invisible in the other, which is how the two modes drift apart.

## What Changes

- **Three-pane workspace, replacing the vertical stack.** Full-width collapsible timeline on top;
  left rail with the filter controls; center pane with the search box and the entry table; right pane
  with the selected entry's detail. The workspace fills the viewport: panes scroll internally and the
  page itself does not scroll, so the log is on screen at load rather than below the fold. The table
  viewport's fixed `480px` height is replaced by "fill the pane" — the virtual-scroll arithmetic
  already reads `clientHeight`, so it adapts without new measurement code.
- **One layout for both modes.** The three-pane shell is extracted so file mode and stdin mode mount
  the same structure, with mode-specific pieces (drop-zone, live indicator) as slots inside it rather
  than as two independent page layouts.
- **Left rail sections, all collapsible.** Search, Time range, Level and per-field sections, each
  with a header showing its state, following the reference. Level becomes full-width colored rows
  with counts rather than small inline chips. Primary sections (Search, Level, the first fields) open
  by default; the rest start collapsed.
- **Secondary shell surfaces move to the bottom of the left rail, collapsed by default:** drop-zone /
  "load another file", format detection, parser diagnostics, the privacy note and the copy-link
  button. Collapsing diagnostics is only allowed because its collapsed header carries the skipped-line
  count and its severity — a warning that has to be opened to be discovered would break `app-shell`'s
  existing "Parser diagnostics reach the user" guarantee, so that requirement is amended rather than
  quietly weakened.
- **Filter controls survive live updates.** A live batch updates counts in place instead of
  rebuilding the control tree, so controls keep their identity, their focus and their in-progress
  text while entries stream. This is the fix for the click bug above; it is specified as behavior, not
  left as an implementation detail, because a control that silently stops responding at a certain
  arrival rate is exactly the kind of quiet failure this project's testing rules call out.
- **Detail pane becomes persistent.** Selecting a row fills the right pane instead of expanding a
  panel under the table; with nothing selected it states so ("select an entry to inspect"). The
  table no longer reflows when an entry is inspected.
- No change to parsing, the WASM boundary, the URL-hash grammar, the filter predicate semantics or
  the security posture. Filter state, search compilation and range ownership keep their current model
  and events; this change moves and re-renders controls, it does not redefine what they mean.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `app-shell`: gains a requirement fixing the workspace layout (three panes, viewport-bound, same
  structure in both modes), and its "Parser diagnostics reach the user" requirement is amended to
  allow a collapsed diagnostics surface only when the collapsed header itself reports the skip count
  and severity.
- `filtering`: gains a requirement that filter controls remain operable while entries arrive, stated
  in terms a user can check (a press-and-release at human speed toggles the filter mid-stream); its
  "Filter chips come from the field inventory" requirement is amended for the rail presentation
  (collapsible per-field sections, counts updated in place).
- `entry-table`: its "Row selection and detail view" requirement is amended for a persistent detail
  pane with an explicit no-selection state, and selection surviving live growth.

## Impact

- `web/index.html`: the shell markup grows the pane skeleton.
- `web/src/components/looq-app.ts`, `web/src/components/looq-live-tail.ts`: both stop composing their
  own page layout and mount the shared workspace instead.
- New component for the workspace layout and the left rail; `looq-filter-bar.ts` is reworked into the
  rail's filter sections with in-place count updates (this is where the click bug is fixed).
- `web/src/components/looq-entry-table.ts`: detail rendering moves out of the table into the right
  pane; the table keeps selection state and emits it.
- `web/src/style.css`: grid layout, pane scrolling, rail section styling, level rows; the fixed
  `480px` table viewport rule goes away.
- No new dependencies, no Rust changes, no change to `crates/looq`'s served routes beyond the rebuilt
  bundle.
