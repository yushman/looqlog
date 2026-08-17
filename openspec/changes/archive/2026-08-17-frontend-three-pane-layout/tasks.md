## 1. Workspace layout element

- [x] 1.1 Add a `looq-workspace` component owning the grid: topbar row, timeline row, then a
  three-column pane row (rail / table / detail). Named slots for the mode-specific surfaces
  (`rail-top`, `rail-secondary`, `table-toolbar`, mode indicator).
- [x] 1.2 CSS grid per design.md D2: `18rem 1fr 22rem`, rows `auto auto 1fr`, host bound to the
  viewport height, `overflow: hidden` on the shell, `min-height: 0` / `min-width: 0` on every
  scrolling pane.
- [x] 1.3 Delete `.entry-table-viewport { height: 480px }` and let the viewport fill its pane
  (`height: 100%`). Confirm the virtual scroller picks up the new height with no code change —
  `looq-entry-table.ts` reads `clientHeight` at render time.
- [x] 1.4 Narrow-window fallback (D8): below ~1100px the grid collapses to a single column with the
  rail collapsed.

## 2. Both shells mount the same layout

- [x] 2.1 `looq-app.ts` renders `<looq-workspace>` instead of its own stacked markup; the drop-zone
  and privacy note become slot content.
- [x] 2.2 `looq-live-tail.ts` renders the same `<looq-workspace>`; the connection indicator and
  lines/sec counter become slot content.
- [x] 2.3 Verify no page-layout CSS remains that only one of the two shells uses.

## 3. Left rail

- [x] 3.1 Rail sections as native `<details>`/`<summary>` (D3): Search, Time range, Level, one
  section per inventory field. Open by default: Search, Level, and the first fields; the rest closed.
- [x] 3.2 Level section renders full-width colored rows with right-aligned counts, using the existing
  six level colors. Toggle semantics unchanged (same `field`/`value` pairs, same events).
- [x] 3.3 Field sections keep the value list with counts and the typed-value input for
  high-cardinality fields. A collapsed section's summary names the field and its value count
  (`filtering` spec, "Many fields stay manageable").
- [x] 3.4 Time-range inputs in the rail: two bounds plus a clear action, wired to the same shell-owned
  active range the timeline drag sets.
- [x] 3.5 Search input moves into the rail's Search section, keeping `re:` handling, the invalid-regex
  error surface, `field=value` promotion and Esc-to-clear.

## 4. Filter controls that survive live updates (the click bug)

- [x] 4.1 Split the rail's render into a structural pass (keyed by `field` + `value`, runs only when
  the set changes) and a count pass (writes count text into existing nodes), per design.md D4.
- [x] 4.2 Reproduce the original failure first, to have a before/after: stream at ~25 lines/sec, press
  a level control and release it ~150ms later, observe that nothing toggles on the current build.
- [x] 4.3 After the fix, the same press-and-release toggles the filter; assert against the emitted
  event and the resulting entry count, not just the hash.
- [x] 4.4 Type a partial value into a high-cardinality field's input, let ten or more live batches
  arrive, confirm the text and caret survive.

## 5. Secondary surfaces in the rail

- [x] 5.1 Move drop-zone / "open another file", detection, diagnostics, privacy note and copy-link to
  the bottom of the rail, collapsed by default.
- [x] 5.2 Diagnostics summary line carries the skip count and severity while collapsed, and the
  section auto-opens when the skip ratio crosses the existing severity threshold (D7).
- [x] 5.3 Detection summary line carries fallback / low-confidence state while collapsed.
- [x] 5.4 Check the privacy copy still reads correctly in stdin mode — the two modes make different
  guarantees (CLAUDE.md, TDR §12); do not let one collapsed block state the file-mode guarantee
  during a live stream.

## 6. Detail pane

- [x] 6.1 Detail rendering moves out of `looq-entry-table.ts` into the right pane; the table keeps
  selection state and emits the selected ordinal.
- [x] 6.2 No-selection state renders explicit copy ("Select an entry to inspect").
- [x] 6.3 Selection is held by ordinal; on eviction the pane says the entry is no longer retained
  rather than describing a different entry.
- [x] 6.4 Selecting a row does not reflow the table (verify row positions before/after).

## 7. Verification

- [x] 7.1 Every check below runs in **both** file mode and stdin mode, against the real compiled
  binary (`scripts/build-frontend.sh` + `cargo build -p looq`), not `vite dev` — it cannot serve
  `/wasm/core.wasm`. File mode needs a TTY: `script -q /dev/null looq …`.
- [x] 7.2 At 1280×800 with a 50k-entry fixture: entry rows visible on first paint, `document`
  scrollHeight equals its clientHeight (no page scrollbar).
- [x] 7.3 Re-measure the table's scroll cost against `timeline-and-table`'s recorded 50k-row result
  and write the number in the devlog with the command that produced it.
- [x] 7.4 Filter/search/range still round-trip through the URL hash unchanged; a link produced before
  this change still applies correctly.
- [x] 7.5 Keyboard pass: every rail section reachable and toggleable by keyboard, focus visible, no
  control reachable only by pointer.
- [x] 7.6 Both themes, per `theming` — the rail, panes and level rows legible in each.
- [x] 7.7 `cargo test --workspace`, `npm run test`, `npm run typecheck`, `cargo fmt --check`,
  `cargo clippy --workspace --all-targets -- -D warnings` all clean.
- [x] 7.8 Rebuild embedded assets and commit them with the source change — CI's
  `frontend-artifact-staleness` job compares `crates/looq/assets/` against a fresh build.
  (Rebuilt and verified byte-identical across two consecutive runs; **not** committed — this
  session leaves everything in the working tree for review.)

## 8. Docs

- [x] 8.1 README check: this is a UI-structure change with no install-step, command, flag or output
  difference — confirm that and state it, or update `README.md` and `README.ru.md` together.
  (No behavioral change; one paragraph in each README called the controls "a row of chips", so both
  were updated together to describe the rail's collapsible sections.)
- [x] 8.2 Devlog entry: what shipped, the measured numbers with their commands, and the before/after
  for the live-stream click bug.

## 9. Column budget and vertical budget (found while verifying, fixed in-change)

- [x] 9.1 Timestamp column overflows in the narrower centre pane: measured `scrollWidth 237` against
  `clientWidth 224` for `2026-08-17T10:00:01.600+00:00`, so the text runs under the level badge in
  live mode (sub-second timestamps). Resize the row grid so the common RFC 3339 timestamp fits.
- [x] 9.2 Clip rather than spill regardless of format: `.col-timestamp` gets the same
  `overflow: hidden` / `text-overflow: ellipsis` treatment `.col-message` already has, so a
  nanosecond-precision timestamp truncates instead of colliding. Row grids are per-row, so the
  columns have to stay fixed-width for rows to align — content-based sizing is not an option here.
- [x] 9.3 The level column is 6rem for a 1.6em badge; give the slack to the message column, which is
  the log content and currently the narrowest column at 212px.
- [x] 9.4 Timeline vertical budget: the top zone takes ~330px of an 800px viewport before the first
  row. Put the timeline's summary, outlier note and range controls on one line instead of three and
  trim the chart's `min-height`.
- [x] 9.5 Re-verify in both modes at 1280×800: no page scrollbar, no timestamp overflow
  (`scrollWidth <= clientWidth`), message column wider than before, first row higher up the page.
- [x] 9.6 Rebuild embedded assets; `npm run typecheck`, `npm run test`, `cargo test --workspace`
  clean.
- [x] 9.7 Rows drop the redundant UTC suffix (`+00:00`/`Z`) — `Entry::timestamp` is a
  `DateTime<Utc>`, so it is identical on every row and the column header already states the zone.
  Only that exactly-redundant suffix is stripped; any other offset stays visible, and the full
  RFC 3339 string stays reachable as the cell's `title` and in the detail pane.
- [x] 9.8 Narrow the timestamp column to the shortened form and give the freed width to the message
  column; re-verify no overflow in both modes.
