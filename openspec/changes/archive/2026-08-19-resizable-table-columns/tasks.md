## 1. Baseline

- [x] 1.1 Re-measure the scroll baseline the way `timeline-and-table` did: 50,000 entries,
  per-frame time while scrolling. Record the number and the method — this is the gate for
  task 5.1, and comparing against a figure from a different machine would be meaningless —
  **method**: `./scripts/build-frontend.sh && cargo build --release -p looq`, fixture
  `python3 scripts/gen-timeline-fixture.py 50000 --jitter 5 > target/perf-fixtures/fixture-50k.jsonl`,
  served by the real release binary with a pty on stdin so `mode_for` picks file mode
  (`run_file_mode.py 7803 ./target/release/looq target/perf-fixtures/fixture-50k.jsonl`),
  Playwright at 1280x800, file opened through the picker, `requestAnimationFrame` frame times
  around a 2-second programmatic `scrollTop` sweep from 0 to `scrollHeight - clientHeight`
  (= 1,199,543 px). **Result (3 runs, identical): 119–120 frames, avg 16.67 ms/frame, max
  17.70 ms, 0 frames over the 33.3 ms threshold; 36 DOM rows for 50,000 entries.** A 500 ms
  (4x faster) sweep gave the same avg 16.65–16.67 ms, max 17.6 ms, 0 over — i.e. frame time is
  vsync-locked on this machine and has almost no headroom sensitivity, so a second, *sensitive*
  baseline was recorded for the same scroll: a capture-phase `scroll` listener (runs before the
  component's) and a bubble-phase one (runs after it) bracket `renderVisibleRows()`, over 120
  rAF-paced scroll steps across the whole dataset — **avg 1.659 ms, p50 1.600 ms, p95 3.000 ms,
  max 3.500 ms of synchronous render per scroll event.** Task 5.1 must compare both numbers;
  the frame-time figure alone would not detect a 2x regression in the render path
- [x] 1.2 Confirm the suite is green (`cargo test --workspace`, `npm run typecheck`,
  `npm run test` in `web/`) so later failures are attributable — `cargo test --workspace` all
  green, `npm run typecheck` clean, `npm run test` 5 files / 55 tests passed
- [x] 1.3 Record the current CSP header verbatim from a running server, so the change to it is
  a diff and not a rewrite from memory — `curl -s -D - -o /dev/null http://127.0.0.1:7803/`:
  `content-security-policy: default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; style-src 'self' 'unsafe-inline'`

## 2. Move row positioning off inline styles

- [x] 2.1 Create the stylesheet the component owns: `document.createElement("style")` appended
  with NO text content, then `insertRule` — not `adoptedStyleSheets`, which Safari only shipped
  in 16.4 while PRD §11 targets Safari 16+ (design D2) — `createStyleSheet()` in
  `looq-entry-table.ts`; rules are scoped per instance via `looq-entry-table[data-looq-table="tN"]`
  so file mode and stream mode cannot position each other's rows. Verified in the browser: the
  element's `textContent` is `""` and its `sheet.cssRules` holds the three rules
- [x] 2.2 Move `ROW_HEIGHT` out of `createRowElement`'s inline write into a `.entry-row` rule in
  `style.css`, exporting the constant so the scroller's arithmetic and the stylesheet cannot
  drift (design D3) — `.entry-row { height: 24px }`; `entry-table-styles.test.ts` asserts the
  CSS number and `export const ROW_HEIGHT` agree. Browser: `getComputedStyle(row).height === "24px"`
- [x] 2.3 Move the spacer height and the container translate onto rules in the 2.1 stylesheet,
  mutating the existing `CSSStyleDeclaration` (`rule.style.transform = …`) rather than replacing
  the sheet's text, which would re-parse it every frame (design D4) — `renderVisibleRows` now
  assigns `this.spacerRule.style.height` / `this.rowsRule.style.transform`; no `replaceSync`, no
  `cssText`, asserted in `entry-table-styles.test.ts`
- [x] 2.4 Verify no `style` attribute is written by the scroller — assert it in a test, not by
  eye, since this is what the CSP change depends on — `entry-table-styles.test.ts` asserts every
  `.style` receiver in the component is a `CSSStyleRule` and that no `setAttribute("style")` or
  `style=` literal exists; in the browser after loading 50k entries, scrolling and dragging,
  `document.querySelectorAll('looq-entry-table [style]').length === 0`. For task 5.2: a
  `grep` over `web/src` finds no other `.style.` / `setAttribute("style")` / `style="` write in
  application code, and a dry run of the strict policy — `<meta http-equiv="Content-Security-Policy"
  content="style-src 'self'">` injected into a loaded page, with a `securitypolicyviolation`
  listener — scrolled 50k entries and resized the window (forcing a `uPlot` redraw, and `uPlot`
  does write `el.style.*` internally) for **0 violations**, with rows still positioned
  (`transform: matrix(1,0,0,1,0,79800)`) and the chart still resizing. Probes confirmed the
  injected policy was live: `setAttribute("style", …)` and a `<style>` with `textContent` were
  both blocked, while CSSOM `el.style.color = …` was NOT — Chrome does not gate CSSOM property
  writes, which is why `uPlot` survives. Note this used a *meta* policy on an already-loaded page,
  not the served header, so task 5.4's full run against the real header still stands

## 3. Resizable columns

- [x] 3.1 Turn the grid template into four custom properties on one rule the JS owns; the
  message column stays `minmax(0, 1fr)` and is not directly resizable (design D1) — three
  properties, not four: `grid-template-columns: var(--col-ordinal) var(--col-timestamp)
  var(--col-level) minmax(0, 1fr)` is the template D1 itself spells out, and the message track is
  a literal, so there is no `--col-message` to own (asserted in `entry-table-styles.test.ts`).
  Defaults are kept in `style.css` as `var(--col-…, 3rem)` fallbacks so the table still renders
  correctly if the generated sheet is never created
- [x] 3.2 Add drag handles to the header row only — never to data rows, so a drag can never
  begin on a row the user meant to select — `resizerHtml()` is called only from the header
  markup; `.col-resizer` is absolutely positioned in the grid gap at each resizable column's
  right edge, `.entry-table-header > span` is `position: relative` and its label clips via a new
  `.header-label` (the old shared `overflow: hidden` on header cells would have cut the handle)
- [x] 3.3 Implement the drag: pointer events, width follows the pointer, all rows stay aligned
  because they read the same variables — `pointerdown/move/up/cancel` with `setPointerCapture`.
  Browser, real mouse events: dragging the timestamp handle 80px left took the column 200px →
  120px and the message column 300.016px → 380.016px, row right edge 919px vs viewport 920px
- [x] 3.4 Per-column minimum widths, clamping rather than refusing to move (design D5) —
  `MIN_COLUMN_WIDTHS` = ordinal 2rem, timestamp 3rem, level 2rem, ceiling 60rem. Dragging the
  timestamp handle 600px left stopped at 48px (= 3rem) with the handle still present and
  re-draggable
- [x] 3.5 Double-click a handle to reset that column; a control to reset all — double-clicking
  the clamped timestamp handle restored 200px with the other columns untouched; the "Reset
  columns" button (shown only once a width is non-default) restored `48px 200px 40px` from
  `88px 200px 64px` and cleared `cols=` from the hash
- [x] 3.6 Test the alignment property directly: after a resize, a data row's column boundaries
  match the header's — after the drag, header and row grids are both `48px 120px 40px`, and
  consecutive cell-boundary gaps match exactly (55/126/47 px in both). The header sits 2px left
  of the rows throughout, before and after the change: the viewport's 1px border plus the 0.4px
  padding difference between `.entry-table-header` (0.5em at 0.8em) and `.entry-row` (0.5em at
  0.85em) — pre-existing, and not a per-column drift

## 4. URL hash

- [x] 4.1 Add `cols=` to the grammar in `web/src/url-hash.ts`, encoding only the directly
  resizable columns (design D6) — `cols=<ordinal>,<timestamp>,<level>` in `rem`, added to
  `KNOWN_KEYS`, omitted entirely when every width is its default. The model itself lives in the
  new `web/src/column-widths.ts` so `url-hash.ts` can clamp a shared width without importing a
  Web Component (and so the rules are testable with no DOM)
- [x] 4.2 Round-trip tests: resize, serialise, parse, and get the same widths back —
  `column-widths.test.ts` and `url-hash.test.ts` (`encodeHash` → `"cols=4.5%2C6%2C2.5"` →
  `decodeHash` → the same widths). In the browser: dragging produced
  `#cols=3%2C7.5%2C2.5`, and reopening `#cols=5,7,3` rendered `80px 112px 48px`
- [x] 4.3 Malformed and out-of-range widths fall back to the default or clamp, are reported with
  the other unapplied hash parts, and never block the log from loading — browser, 50k file
  opened for each: `#cols=3,wide,2.5&q=job` → timestamp at its default, notice `column width
  "wide" for the timestamp column is not a number`, and `q=job` still applied (7,143 of 50,000
  entries shown); `#cols=3,0.01,2.5` → clamped to 3rem with a notice; `#cols=3,12.5` → all
  defaults with a notice (a wrong arity cannot be assigned positionally, so it is the one case
  that falls back wholesale). All four loaded the log with 36 rows rendered
- [x] 4.4 Debounce the write like the other hash keys, so dragging does not produce a history
  entry per pointer move — the table emits `columns-change`, the shell folds it into the same
  `HashWriter.schedule` every other key uses (300ms, `replaceState`); `copyShareableLink`'s
  existing `flush()` means a link copied mid-drag is still current

## 5. Verification

- [x] 5.1 Re-measure 50,000-row scrolling against task 1.1. If per-frame rule mutation cannot
  hold the baseline, STOP and report — the documented fallback (keep the two container writes
  inline and drop only the column widths off that path) is a worse outcome that the user
  chooses, not one to take silently (design D4) — **PASSED.** Same method as 1.1: `bash
  scripts/build-frontend.sh && cargo build --release -p looq`, `script -q /dev/null
  ./target/release/looq --port 7811 --no-browser target/perf-fixtures/fixture-50k.jsonl`,
  Playwright at 1280x800, file through the picker, 50,000 entries / 36 DOM rows, 2-second
  `scrollTop` sweep over 1,199,543 px. **Frame time (9 runs): avg 16.66–16.81 ms (baseline
  16.67), max 17.6–17.8 ms, 0 frames over 33.3 ms in 8 of 9 runs** — the one exception was the
  first run of a batch, with a single 33.68 ms frame; the 8 runs after it were clean, so it reads
  as warm-up, and it is recorded rather than dropped. **Sensitive metric** (capture-phase +
  bubble-phase `scroll` listeners bracketing `renderVisibleRows()`, 120 rAF-paced steps, baseline
  avg 1.659 / p50 1.600 / p95 3.000 / max 3.500 ms): **cold first sweep after a page load
  avg 2.27 ms (p50 1.8, p95 3.6, max 10.7)** — inside the 2.16–2.57 ms band the implementing
  agent reported, which the user was shown and accepted — **but warm repeats of the same sweep
  run at avg 0.89–1.95 ms**, i.e. at or below the baseline. So the cold/warm spread is wider than
  the regression, and the "+30%" is a cold number against a baseline of unrecorded warmth. Either
  way no run approaches the frame budget; proceeding as the user directed. Bracket verified real,
  not assumed: the row container's computed transform is `matrix(1,0,0,1,0,0)` in the capture
  listener and `matrix(1,0,0,1,0,599568)` in the bubble one, so the render does happen between them
- [x] 5.2 Only after 5.1 passes and 2.4 holds: drop `'unsafe-inline'` from `style-src` in
  `crates/looq/src/server.rs`, and delete the comment explaining why it was needed rather than
  leaving it describing a state that no longer exists — **re-applied after 5.7 and verified by
  5.10.** Served header, read off a running release binary with
  `curl -s -D - -o /dev/null http://127.0.0.1:7821/`: before
  `default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; style-src 'self' 'unsafe-inline'`,
  after `default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; style-src 'self'`. The
  `'unsafe-inline'` bullet in `add_security_headers`' doc comment is gone; in its place is a short
  note that the scroller inserts its rules into the already-linked `/assets/index.css`, so nothing
  describes a state the binary no longer has
- [x] 5.3 Update the CSP assertion in `crates/looq/tests/cli.rs` — `every_response_carries_the_csp_header`
  now also asserts `style-src 'self'` is present and that `'unsafe-inline'` appears nowhere in the
  policy, so a silent re-widening fails the suite
- [x] 5.4 Load a real file under the strict policy and confirm ZERO console violations and a
  table that renders. The previous attempt at this produced 144 violations and a table with no
  rows — that is the exact failure mode to check for — **PASSED on the re-run, see 5.10.** The
  original run against the `<style>`-element mechanism is kept below because it is the evidence
  design D2 was rewritten from: **FAILED. 1 violation, and a table that
  renders 36 rows and cannot scroll to the other 49,964.** On page load, before any file is
  opened: `Applying inline style violates the following Content Security Policy directive
  'style-src 'self''. Either the 'unsafe-inline' keyword, a hash
  ('sha256-47DEQpj8HBSa+/TImW+5JCeuQeRkm5NMpJWZG3hSuFU='), or a nonce ('nonce-...') is required`
  — at `assets/index.js:91`, i.e. `createStyleSheet`. That hash is the SHA-256 of the **empty
  string**: `style-src` gates the *insertion* of a `<style>` element regardless of whether it has
  content, so design D2's premise ("An empty `<style>` element has no content to parse, so nothing
  is blocked") does not hold in Chrome. Consequence, measured with the 50k file loaded under the
  strict header: `styleEl.sheet === null`, so `columnsRule`/`spacerRule`/`rowsRule` all stay
  `null`, `renderVisibleRows` skips both writes, spacer `height: 0px`, viewport `scrollHeight` 864
  (not 1,200,000), rows `transform: none`, resizing inert. Task 2.4's dry run could not have
  caught this: a `<meta>` policy injected into an already-loaded page never exercises element
  insertion, only the CSSOM writes that genuinely are not gated
- [x] 5.5 Full suite: `cargo test --workspace`, `cargo fmt --all --check`,
  `cargo clippy --all-targets --all-features -- -D warnings`, `npm run typecheck`, `npm run test`
  — all green on the reverted tree, first run each, no flakes and no re-runs needed: `cargo test
  --workspace` 193 passed / 0 failed across 7 binaries (19 + 27 + 114 + 33 + three empty),
  `cargo fmt --all --check` clean, `cargo clippy --all-targets --all-features -- -D warnings`
  clean, `npm run typecheck` clean, `npm run test` 7 files / 78 tests passed. **Re-run after
  5.7–5.10, all green again on the first run with no flakes:** `cargo test --workspace` 193 passed /
  0 failed (19 + 27 + 114 + 33 + three empty), `cargo fmt --all --check` clean, `cargo clippy
  --all-targets --all-features -- -D warnings` clean, `npm run typecheck` clean, `npm run test`
  7 files / **80** tests (the two new `entry-table-styles.test.ts` cases)
- [x] 5.6 Drive the resize end to end in a browser: drag, clamp at the minimum, double-click
  reset, reload from the shared link and confirm the widths return — all five, against the release
  binary with real mouse events (`page.mouse.down/move/up`, not synthetic `PointerEvent`s).
  **Drag:** timestamp handle 80 px left took the column 200px → 120px, message absorbed it
  (300.016px → 380.016px), header and row grids agreed at every step (`48px 120px 40px`), row
  width stayed 622 px against a 622 px viewport, and `.entry-row.selected` count stayed 0 — a drag
  across the table selects nothing. **Clamp:** 600 px left stopped at 48px = the 3rem timestamp
  floor, handle still present and re-draggable. **Double-click:** on the clamped timestamp handle,
  restored 200px while leaving a separately-widened ordinal column at 88px (`88px 200px 40px`,
  hash `#cols=5.5%2C12.5%2C2.5`). **Reset all:** the button is `hidden` at defaults, appears once
  a width differs, and restored `48px 200px 40px` and cleared `cols=` from the hash entirely.
  **Reload:** a genuinely fresh load (`about:blank` first, so it is a document load and not a
  same-document hash change) of `#cols=5,7,3` rendered `80px 112px 48px` with the reset button
  shown and the hash normalised to `#cols=5%2C7%2C3`. Noted while testing: editing the hash on an
  *already-loaded* page does nothing, because the app applies the hash only at startup — verified
  pre-existing and not specific to `cols=` by setting `#q=job` the same way on a loaded page,
  which was likewise ignored
- [x] 5.7 **(added during 5.4)** Replace the `<style>`-element mechanism with rule insertion into
  the application's own already-linked `/assets/index.css`, found via `document.styleSheets`
  (design D2, rewritten after the original premise was disproved). No element is created, so
  nothing is gated by `style-src`; the sheet is same-origin and predates every PRD §11 browser.
  Chosen over `adoptedStyleSheets` (Safari 16.4 vs a Safari 16+ target, and a second untestable
  code path) and over allowlisting the empty-string hash (a permanent CSP concession where a
  concession-free option exists) — `createStyleSheet` now calls `findAppStyleSheet()` and appends
  its three rules with `insertRule(…, sheet.cssRules.length)`; `document.createElement("style")` is
  gone from the file. Per-instance scoping is unchanged
  (`looq-entry-table[data-looq-table="tN"]`), verified in the browser under the served strict
  header: the sheet at `http://127.0.0.1:7821/assets/index.css` ends with exactly
  `… .entry-table-wrap { --col-ordinal: 3rem; --col-timestamp: 12.5rem; --col-level: 2.5rem; }`,
  `… .entry-table-spacer`, `… .entry-table-rows`. Ordering is not assumed: a `<link>` stylesheet
  blocks script execution, so `index.css` is always in `document.styleSheets` before any module —
  and therefore any `connectedCallback` — runs; if it is not there, 5.8 fires instead of waiting.
  `disconnectedCallback` removes the three rules by identity (`deleteRule`) rather than by a
  remembered index, since the sheet is shared and a remount inserts a fresh set. A `vite dev`
  fallback identifies the same sheet by a marker selector (`.entry-row`), because Vite injects each
  imported `.css` as a separate href-less `<style>` there
- [x] 5.8 Make the missing-stylesheet guard fail LOUD, not soft. `createStyleSheet`'s
  `if (!sheet) return` is what turned a blocked insertion into a table that silently showed 36
  rows of 50,000 — exactly the class of quiet failure CLAUDE.md's silent-failure list exists to
  prevent. A stylesheet that cannot be found is not a survivable state — the guard now calls
  `reportFatal()` and then throws. `reportFatal` replaces the table's markup with a persistent
  `.error-banner`, the same element `error-states` uses for the app shell's "errors are reachable,
  not transient" banner, so the failure is visible to the user and cannot be read as an empty log.
  The three rule fields stopped being nullable, so both `renderVisibleRows` writes and
  `applyColumnWidths` lost their `if (rule)` guards — there is no code path left that can skip them.
  Verified in the real browser by shadowing `Document.prototype.styleSheets` with `[]` and mounting
  a table: banner text *"The table's stylesheet (/assets/index.css) could not be found, so rows
  cannot be positioned. The table is disabled rather than showing part of the log as if it were all
  of it."*, rendered in `rgb(185, 28, 28)`, no `.entry-table-viewport` at all, and
  `Error: looq-entry-table: /assets/index.css is not in document.styleSheets` in the console
- [x] 5.9 Update `entry-table-styles.test.ts`, which asserts the old mechanism — the
  "creates its `<style>` element with no text content" test is replaced by three: it must NOT call
  `createElement("style")` and must reference `document.styleSheets` / `/assets/index.css`; its
  `createStyleSheet` body must contain `throw new Error` and `reportFatal` and must not contain the
  soft `if (!sheet) { return; }`; and it must `deleteRule` on disconnect. The existing
  no-inline-style, no-`adoptedStyleSheets`, no-`replaceSync`/`cssText` and `ROW_HEIGHT`-agreement
  assertions are unchanged and still pass
- [x] 5.10 Re-apply and re-verify 5.2, 5.3 and 5.4 against the new mechanism, with the strict
  policy SERVED on the response — not injected as a `<meta>` into a loaded page, which is what
  made task 2.4's dry run miss this defect entirely — **PASSED.**
  `bash scripts/build-frontend.sh && cargo build --release -p looq`, fixture
  `python3 scripts/gen-timeline-fixture.py 50000 --jitter 5 > target/perf-fixtures/fixture-50k.jsonl`,
  `script -q /dev/null ./target/release/looq --port 7821 --no-browser target/perf-fixtures/fixture-50k.jsonl`,
  Playwright, file opened through the picker. Served header confirmed by `curl`:
  `content-security-policy: default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; style-src 'self'`.
  **ZERO CSP violations** — both a `securitypolicyviolation` listener (`[]`) and the console
  (1 message total, a pre-existing `favicon.ico` 404) across page load, file open, a full-dataset
  scroll, a column drag and a window resize (which forces a `uPlot` redraw, and `uPlot` writes
  `el.style.*` internally — not gated, since CSSOM writes never were). **The table really works,
  not just its first screenful:** 50,000 entries / 36 DOM rows, viewport `scrollHeight` **1,200,000**
  (the failing run's was 864), spacer `height: 1.2e+06px`, and scrolled to the end the row container's
  computed transform is `matrix(1, 0, 0, 1, 0, 1.19933e+06)` showing ordinals **49,973–50,000**
  (mid-dataset: `matrix(1, 0, 0, 1, 0, 599808)`, ordinals 24,993–25,028).
  `document.querySelectorAll('looq-entry-table [style]').length === 0` throughout. Dragging the
  timestamp handle left clamped it to 48px = its 3rem floor with header and rows agreeing
  (`48px 48px 40px 916.008px` / `…912.016px`, the pre-existing 4px padding difference from 3.6) and
  wrote `#cols=3%2C3%2C2.5`. Also re-run in **stdin mode** (`head -5000 … | looq --port 7822`), since
  that mounts a second table through `looq-live-tail`: 5,000 entries, `scrollHeight` 120,000,
  transform `matrix(1, 0, 0, 1, 0, 119400)`, three scoped rules present, 0 violations, 0 inline styles

## 6. Documentation

- [x] 6.1 Update `README.md` and `README.ru.md` together — resizable columns, the reset, and
  `cols=` in the documented hash grammar. Keep them in sync in the same pass — one pass over both:
  a new **Column widths** / **Ширина колонок** paragraph (drag a header boundary, the message
  column absorbs the remainder so there is never a horizontal scrollbar, per-column minimum,
  double-click to reset one, the **Reset columns** button that appears only once a width differs),
  and the **URL sharing** / **Шаринг через URL** paragraph extended with column widths in its list,
  `cols=<#>,<timestamp>,<level>` in `rem`, "the message column is derived and so has no place in
  the grammar", "all-default widths are left out of the link entirely", and the fall-back rules
  (not a number → that column's default; below the minimum → clamped; never a reason for a log not
  to load). Both files carry the same facts in the same two places
- [x] 6.2 Note in both READMEs' Security sections that inline styles are no longer permitted, if
  the current text mentions the concession — done with 5.2/5.10, in the same pass over both files.
  Each Security section now says `script-src`'s `'wasm-unsafe-eval'` is the only addition, and that
  inline styles are **not** permitted (`style-src` is `'self'`, no `'unsafe-inline'`) because the
  virtual-scrolled table positions rows through rules in the page's own stylesheet rather than
  through `style` attributes. The old sentence describing the concession is gone from both
- [x] 6.3 Append a `docs/devlog.md` entry with the before/after per-frame numbers and the command
  that produced them, and close the parked "rework row positioning off inline styles" item in
  `## Ideas for later` — **entry appended, item deliberately left open.** `docs/devlog.md` has a
  new `## 2026-08-19 — resizable-table-columns: the perf gate passed, the CSP gate did not` entry
  with both metrics, the commands, the point that the frame-time metric alone would have hidden
  the render-path regression, the fact that the user was shown both and accepted it, and the full
  5.4 failure with the three probed fixes. **Updated after 5.7–5.10:** the same entry (retitled
  `resizable-table-columns: the CSP gate, on the second attempt`) gained the resolution — the
  mechanism that replaced the `<style>` element, the fail-loud guard, the served-header numbers, and
  the point that only a served policy exercises element insertion. The parked "rework the
  virtual-scrolled table's row positioning off inline `element.style`…" item is now **closed and
  removed** from `## Ideas for later`, because the `style-src` payoff it existed for has landed. The
  neighbouring collapsible-rail/detail-pane item was not touched — it is a live requirement for a
  future change
  — entry `## 2026-08-19 — `resizable-table-columns`: the perf gate passed, the CSP gate took
  two attempts`, with both perf metrics and their method, the falsified `<style>`-element premise
  and its empty-string hash, the three probed alternatives, and a `### The second attempt` section
  covering the linked-stylesheet mechanism, the fail-loud guard and the served-header re-verification.
  The parked inline-`element.style` item is closed and removed from `## Ideas for later`; the
  neighbouring collapsible rail/detail item is untouched.
- [x] 6.4 Run `openspec validate resizable-table-columns --strict` — `Change
  'resizable-table-columns' is valid`
