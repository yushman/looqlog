## 0. Prerequisite

- [x] 0.1 Confirm `resizable-table-columns` is **archived** before starting. This change's
  `url-state` delta is written against a "Hash grammar" requirement that already lists `cols=`;
  archiving in the other order would silently drop `cols=` from the main spec while still
  validating. If it is not archived, stop and say so — **archived**, at
  `openspec/changes/archive/2026-08-19-resizable-table-columns/`, and
  `openspec/specs/url-state/spec.md`'s "Hash grammar" requirement already names "the entry
  table's column widths" and carries the two `cols=` scenarios, which is the text this change's
  delta is written against

## 1. Baseline

- [x] 1.1 Confirm the suite is green (`cargo test --workspace`, and `npm run typecheck` /
  `npm run test` in `web/`) so later failures are attributable — `cargo test --workspace`
  19 + 27 + 114 + 33 = **193 passed, 0 failed**; `npm run typecheck` clean; `npm run test`
  **7 files / 80 tests passed**
- [x] 1.2 Note the current `.workspace` grid template and the three places the 1100px breakpoint
  is currently expressed, so the D5 consolidation is a diff and not a rewrite from memory —
  template was `grid-template-columns: 18rem minmax(0, 1fr) 22rem` with rows
  `auto auto minmax(0, 1fr)` (`style.css:182`). The breakpoint had **two** literal definitions,
  not three: `@media (max-width: 1100px)` in `style.css:293` and
  `NARROW_LAYOUT_QUERY = "(max-width: 1100px)"` in `looq-workspace.ts:33` (consumed by
  `isNarrowLayout()`, called twice from `looq-filter-bar.ts` to decide whether the rail's sections
  start open); the `style.css` comment above the media query warned that the two must stay in
  sync. This change's stacked-collapse rule would have been the third consumer

## 2. Collapse mechanics

- [x] 2.1 Define the stacking breakpoint once and consume it from both the stylesheet and
  `looq-workspace.ts`, replacing the duplicated `1100px` / `NARROW_LAYOUT_QUERY` pair, and update
  the `style.css` comment that currently warns about two copies (design D5) — the media query is
  now the single definition and publishes it: `:root { --narrow-layout: 0 }` with
  `@media (max-width: 1100px) { :root { --narrow-layout: 1 } }`, and `isNarrowLayout()` reads
  `getComputedStyle(document.documentElement).getPropertyValue("--narrow-layout")` instead of
  calling `matchMedia` with a repeated string. `NARROW_LAYOUT_QUERY` is gone;
  `looq-filter-bar.ts` keeps importing `isNarrowLayout` unchanged. The comment now says the width
  in the media query is the only definition in the codebase and names what reads it.
  `workspace-layout.test.ts` asserts `1100px` occurs exactly once in `style.css` and not at all in
  `looq-workspace.ts`. Browser check at 900px: `--narrow-layout` reads `1`, single-column
  template, and **0 of 4** rail filter sections open — the behavior `isNarrowLayout()` drives
  still fires at the same width
- [x] 2.2 Drive `.workspace`'s grid template from two custom properties so a collapsed pane's
  track shrinks and the table absorbs it; do NOT use `display: none`, which would discard the
  pane's scroll position on every toggle (design D2) — `grid-template-columns: var(--ws-rail-width)
  minmax(0, 1fr) var(--ws-detail-width)`, set to `var(--ws-strip-width)` by
  `.workspace[data-collapsed~="rail"]` / `[~="detail"]`. **Re-measured after group 8** (the
  collapsed track is the strip's `2rem`, not `0`), Chrome at 1440x860: expanded
  `288px 800px 352px`; rail collapsed `32px 1056px 352px`; both collapsed `32px 1376px 32px`.
  `document.scrollHeight - clientHeight` stayed **0** in all three states.
  `computedStyle(.ws-rail).display` is `flex` when collapsed — the box is never destroyed
- [x] 2.3 Give each pane its own collapse control with `aria-expanded` and an accessible name
  (design D1, as reworked in group 8 — the topbar pair this task originally described is gone) —
  `#ws-toggle-rail` / `#ws-toggle-detail` live in a `.ws-pane-strip` inside `.ws-rail` /
  `.ws-detail`, `aria-label` "Filters" / "Details", `aria-controls` pointing at the pane's
  `.ws-pane-content` (`#ws-pane-rail-content` / `#ws-pane-detail-content`), `aria-expanded`
  maintained on every state change. Verified in the accessibility snapshot
  (`button "Filters" [expanded]`, `button "Details" [expanded]`) and after toggling
  (`aria-expanded` `false`/`true`); with both panes collapsed both buttons are still 22x17px on
  screen. `workspace-layout.test.ts` asserts the strip and its toggle are inside each `<aside>`,
  and that no copy of the control is left in the `<header>`
- [x] 2.4 Remove a collapsed pane from the tab order, and assert it — a narrow pane still
  contains tabbable controls, and tabbing into an invisible filter list is the accessibility
  form of the bug this change exists to prevent (design D2) — done with `visibility: hidden`,
  **scoped to `.ws-pane-content`** rather than the whole pane (group 8: the strip is now the only
  way back and must stay reachable); not `display: none`, which would take the box with it.
  **Asserted in a real browser**: with both panes collapsed, 40 consecutive `Tab` presses cycled
  the whole page (`theme-toggle → ws-timeline-summary → zoom-out-btn → ws-toggle-rail → viewport
  → ws-toggle-detail → …`) and landed inside `.ws-rail` **6 times, all of them on
  `#ws-toggle-rail`**, and inside `.ws-detail` **6 times, all of them on `#ws-toggle-detail`** —
  the strip's own button and nothing else, while the rail still contained **26** elements matching
  the focusable selector (25 of them unreachable). `workspace-layout.test.ts` pins the mechanism
  (`visibility: hidden` present, `display: none` absent, and the selector scoped to
  `> .ws-pane-content`)
- [x] 2.5 In the stacked layout, collapse hides the stacked block instead of a grid column, with
  the controls keeping their meaning (design D5) — inside the media query,
  `.workspace[data-collapsed~="rail"] .ws-rail > .ws-pane-content` (and the detail equivalent)
  get `display: none` while the strip stays as a full-width horizontal bar; there is no side
  column to shrink below the breakpoint. **Re-verified after group 8** at 900x800: collapsing the
  rail leaves a `900x27px` bar with the label in `horizontal-tb` and the content `display: none`,
  the button stays 22x17px and reopens the block (`900x650px` again), and the hash still reads
  `#panes=rail`

## 3. The no-hidden-warnings rule at pane level

This is the group that makes this more than a CSS change — do it before the polish.

- [x] 3.1 Surface an indicator on the rail's toggle when a surface inside it is in a state
  `app-shell` already calls out: skipped lines, or fallback / low-confidence detection (design D3)
  — `looq-detection` and `looq-diagnostics` raise a bubbling `rail-attention` event carrying
  `{ needsAttention, autoExpand }`; `looq-workspace` listens for it **on itself** (both shells
  mount the same surfaces into the rail, so the rule belongs to the layout rather than being
  duplicated in `looq-app` and `looq-live-tail`), tracks the warning sources in a set so two
  surfaces can warn at once, and shows a `!` badge plus an `attention` class and a title on the
  rail's own button — which, after group 8, is the button on the rail's strip, so the indicator
  sits on the collapsed pane itself. Diagnostics reports on skipped lines, detection on fallback
  or below-threshold confidence — the same two conditions the existing requirement names
- [x] 3.2 Make a condition that auto-expands a section also expand the pane containing it, so an
  auto-expanded surface is actually visible — the same event's `autoExpand` flag is set on exactly
  the transition that flips each component's existing `autoOpened` guard, and the workspace
  answers it with `expandPane("rail")` (a no-op when the rail is already open, so it never emits
  a spurious change). Expanding this way goes through the normal `panes-change` path, so the hash
  follows
- [x] 3.3 Test both: a collapsed rail with skipped lines shows the indicator; a severe skip ratio
  expands the rail, not just the diagnostics section — **driven in Chrome against real files**,
  release binary served over a pty. `target/ui-fixtures/mild-skips.jsonl` (900 valid JSON lines,
  59 unparsable → 6.2%, under the 20% severe threshold) opened from `#panes=rail`:
  `collapsed="rail"`, rail width **32px (the strip)**, badge visible **on the strip's button**
  (`badge.closest(".ws-pane-strip")` true, `closest("header")` false, box `[19, 267, 7, 12]`),
  button class `ws-pane-toggle attention` and title "A surface in the filter rail needs
  attention", diagnostics summary `59 skipped`, `#diagnostics-section.open === false`, rail
  **stays** collapsed. `severe-skips.jsonl` (300 valid, 200 unparsable → 40%) opened from
  `#format=json&panes=rail+detail`: after the parse `collapsed="detail"` — the rail expanded to
  **288px, content visibility visible**, `#diagnostics-section.open === true` with state
  `200 skipped` (class `rail-section-state warning`), while the detail pane stayed collapsed at
  32px. The detection path was exercised too: `prose.txt` opened from `#panes=rail` produced
  `fell back to plain text (0%)` (class `rail-section-state warning`), auto-opened the detection
  section and expanded the rail on its own (`collapsed` back to `null`, width 288). **All three
  re-run against the group-8 build** — earlier numbers said rail width `0`
- [x] 3.4 Confirm the existing scenarios still hold with the pane expanded — this requirement was
  modified, not replaced, and its original behavior must not regress — clean parse with both panes
  open (`clean.jsonl`, 2000 entries, re-run on the group-8 build): diagnostics `0 skipped` and
  `open === false`, detection `json (100%)` and `open === false`, badge `hidden`, button class
  plain `ws-pane-toggle` with an empty title — "A clean parse stays out of the way" intact. "Skipped lines are visible while collapsed" (the *section* sense) intact: the
  `59 skipped` / `200 skipped` summaries above are readable without expanding anything, and the
  severe one still auto-opens its own section. "Fallback detection is not buried" intact:
  `fell back to plain text (0%)` on the collapsed summary with the `warning` class

## 4. Detail pane and selection

- [x] 4.1 Selecting a row while the detail pane is collapsed expands it (design D4) — both shells
  handle it in their existing `selection-change` listener: a non-null ordinal calls
  `workspace.expandPane("detail")`. **Re-verified on the group-8 build** in file mode (both panes
  collapsed to strips, clicked row 4 → detail expanded from 32px to 352px, content visibility
  `visible`, showing `ordinal4 … DEBUG`, `aria-expanded` back to `true`, and the hash went from
  `#panes=rail%2Bdetail` to `#panes=rail`) and again in stdin mode (collapse detail →
  `#panes=detail`, 32px strip with the vertical `Details` label, click a row → pane back at 352px
  showing ordinal 433, hash back to empty)
- [x] 4.2 Collapsing the detail pane does not clear the selection; re-expanding shows the same
  entry — **re-verified on the group-8 build** (stream mode, entry 433 selected): collapsing left
  `.entry-row.selected` on the same row (`433 2023-11-14T22:13:20 I line 432`) and the detail
  content unchanged behind the strip, and re-expanding showed the same content
  (`ordinal433 timestamp2023-11-14T22:13:20+00:00 (UTC) levelINFO`). Nothing in the collapse path
  touches `setSelectedOrdinal`
- [x] 4.3 Test both directions — done in the same browser run as 4.1/4.2 above (collapsed →
  select → expands; selected → collapse → expand → same entry), in both file and stream mode

## 5. URL hash

- [x] 5.1 Add `panes=` to the grammar in `web/src/url-hash.ts`, naming the panes that are
  *collapsed* so the default state is absent from the hash (design D6) — the vocabulary lives in
  a new `web/src/panes.ts` (`COLLAPSIBLE_PANES`, `encodeCollapsedPanes`, `parseCollapsedPanes`),
  mirroring `column-widths.ts` for the same reason: `url-hash.ts` must read a pane name without
  importing a Web Component or needing a DOM. `HashState`/`DecodedHash` gain
  `collapsedPanes` (+ `collapsedPaneErrors`), and `panes` joins `KNOWN_KEYS` so it is never
  reported as unrecognised. `encodeCollapsedPanes` returns `null` when nothing is collapsed, so
  the key is omitted entirely
- [x] 5.2 Round-trip tests: collapse, serialise, parse, get the same state back — in
  `url-hash.test.ts`: one pane (`panes=detail`), both panes in a stable `COLLAPSIBLE_PANES` order
  (`panes=rail%2Bdetail` — `+` is the documented separator and travels as `%2B` because a literal
  `+` means a space in a form-urlencoded fragment), "nothing collapsed means no key at all"
  (`encodeHash` of a query-only state is exactly `q=boom`), and "no key means nothing collapsed,
  reported as nothing". End-to-end in the browser: collapse rail → `#panes=rail`, collapse both →
  `#panes=rail%2Bdetail`, reopen everything → hash back to empty
- [x] 5.3 An unknown pane name is dropped and reported while the rest of the value still applies;
  a value that cannot be parsed at all leaves both panes expanded, reports, and never blocks the
  log from loading — `parseCollapsedPanes` reports per unknown name and, when the value names no
  pane at all, reports that too. Unit tests cover `panes=rail%2Bsidebar` (rail applied, one
  error, `q=` still applied, `unknownKeys` empty), `panes=` (nothing collapsed, one error) and
  `panes=???`. **Verified in the browser**: `#panes=rail+sidebar` → rail collapsed, 2000 entries
  loaded, 38 rows rendered, notice reads *"From the shared URL, could not be applied: collapsed
  pane "sidebar" is not a pane name."*; `#panes=???` → both panes expanded, log still loads, same
  notice shape. No console errors in either
- [x] 5.4 Route the write through the existing debounced hash writer, like every other key — the
  workspace emits `panes-change`; each shell mirrors the set into its own field and calls
  `scheduleHashWrite()`, exactly as `columns-change` does. `HashWriter` is untouched, so
  `flush()` before "Copy shareable link" still picks up a collapse that happened moments ago

## 6. Verification

- [x] 6.1 Full suite: `cargo test --workspace`, `cargo fmt --all --check`,
  `cargo clippy --all-targets --all-features -- -D warnings`, `npm run typecheck`, `npm run test`.
  Note the CLI test binary has port-binding tests that flake if a server was just killed nearby —
  re-run before reporting a failure, and say which it was — **all green, first run each, no
  flakes and no re-runs needed**, re-run after group 8: `cargo test --workspace`
  19 + 27 + 114 + 33 = **193 passed, 0 failed**; `cargo fmt --all --check` clean; `cargo clippy
  --all-targets --all-features -- -D warnings` clean; `npm run typecheck` clean; `npm run test`
  **8 files / 98 tests passed** (was 7 / 80 before this change — the new
  `workspace-layout.test.ts` plus the `panes=` cases in `url-hash.test.ts`)
- [x] 6.2 Drive it in a browser at a wide window: collapse each pane, confirm the table takes the
  width, reopen both, and confirm the document still does not scroll — Chrome at 1440x860,
  `script -q /dev/null ./target/release/looq --port 7822 --no-browser …` after
  `bash scripts/build-frontend.sh && cargo build --release -p looq`. Numbers in 2.2 (re-measured
  for the strip); the document never scrolled (`scrollHeight - clientHeight === 0` expanded, one
  pane collapsed, and both collapsed), and with both panes collapsed both strips and their buttons
  were still on screen (rail strip at x=0 w=32, detail strip at x=1408 w=32)
- [x] 6.3 Drive it below the breakpoint: collapse in the stacked layout, and open a link produced
  in the wide layout to confirm it applies — at 900x800 the layout stacks
  (`--narrow-layout: 1`, template `900px`) and collapsing leaves the rail's strip as a `900x27px`
  bar with the content `display: none`; the wide-layout-produced `#panes=rail%2Bdetail` opened at
  900px gives `collapsed="rail detail"` with both blocks reduced to their `900x27px` bars and both
  buttons 22x17px, and the same link back at 1440px gives the same collapsed set with template
  `32px 1376px 32px` and both panes 32px wide — one serialisation, both layouts, a reachable
  control in each
- [x] 6.4 Drive the warning path with a real file that produces skipped lines: collapse the rail,
  confirm the indicator, and confirm a severe ratio expands the pane — see 3.3 for the fixtures,
  the numbers and both outcomes. Re-run against the rebuilt bundle after the group-8 markup
  change, with zero console errors on the page (only pre-existing favicon 404s and the stream
  mode's WebSocket retries from earlier sessions appear in the all-session log)
- [x] 6.5 Tab through the page with a pane collapsed and confirm focus never enters it, apart from
  the strip — see 2.4: 40 `Tab` presses, the only landings inside either collapsed pane were on
  that pane's own reopen button

## 7. Documentation

- [x] 7.1 Update `README.md` and `README.ru.md` together — collapsing the panes and `panes=` in
  the documented hash grammar. Keep them in sync in the same pass — one pass over both: a new
  **Collapsing the side panes** / **Сворачивание боковых панелей** paragraph (rewritten in group
  8: the per-pane button, the labelled strip, what happens in the stacked layout, that collapsing
  never hides a warning and that a severe condition reopens the pane, and that selecting a row reopens
  the detail pane), and the **URL sharing** / **Шаринг через URL** paragraph extended with the
  collapsed panes in its list, `panes=rail|detail|rail+detail` with "the key names what is
  *collapsed*, so both-open is absent from the link", and the fall-back rule (an unknown pane name
  is dropped and reported, the rest of the value still applies, never a reason for a log not to
  load). Both files carry the same facts in the same two places
- [x] 7.2 Append a `docs/devlog.md` entry, and remove the collapsible-panes item from
  `## Ideas for later` now that it has shipped. Leave the neighbouring *resizable* panes item
  alone — it is a different, still-parked idea — entry `## 2026-08-19 —
  collapsible-workspace-panes: the CSS was the easy half` appended, with the measured pane widths
  before/after, the `rail-attention` mechanism and why it lives on the workspace, both fixtures
  and their skip ratios with what each produced, the breakpoint consolidation, the 40-`Tab`
  result, and the suite numbers. The collapsible item is removed from `## Ideas for later`; the
  resizable rail/detail item directly below it is untouched. **Rewritten in group 8**: the stale
  numbers are corrected and three paragraphs added — the D1 reversal and why the pixel argument
  lost, the tab-order consequence with the re-measured sweep, and the stacked-layout treatment
- [x] 7.3 Run `openspec validate collapsible-workspace-panes --strict` — `Change
  'collapsible-workspace-panes' is valid`

## 8. Rework: the control belongs to the pane (design D1, reversed)

The topbar-toggle version was built, verified and rejected on sight. D1 now specifies an icon
button on each pane, and a collapsed pane leaving a labelled vertical strip instead of a zero
track. This group replaces that part of groups 2 and 3; everything else (the hash, the stacked
layout, selection-expands-detail, the auto-expand rule) stays as built.

- [x] 8.1 Move the control onto each pane as an icon button, and delete `.ws-pane-toggles` from
  the topbar along with its markup, styles and tests. Do not leave a second way to do the same
  thing — a `paneStrip(pane)` helper in `looq-workspace.ts` renders
  `.ws-pane-strip > button.ws-pane-toggle + span.ws-pane-strip-label` inside each `<aside>`; the
  button keeps its old id (`#ws-toggle-rail` / `#ws-toggle-detail`), gains `aria-label` and now
  points `aria-controls` at the pane's content region. The icon is an **inline SVG chevron**
  (`currentColor`, mirrored by CSS per pane and per state so it always points where the click
  sends the pane) — no new dependency and no external asset, which the `default-src 'self'` CSP
  would block anyway. `.ws-pane-toggles`, `.ws-topbar-row` and their CSS are gone; the header is
  back to `.ws-topbar-line` + `.ws-messages`. `workspace-layout.test.ts`'s topbar assertions were
  replaced by "the control belongs to the pane it collapses", which asserts the strip is inside
  each aside **and** that neither the markup nor the stylesheet still mentions `ws-pane-toggles`
- [x] 8.2 Collapsed state leaves a strip instead of a zero track: the grid template carries a
  strip width, so the table gains `pane − strip` rather than the whole pane — `--ws-strip-width:
  2rem` on `.workspace`, and `[data-collapsed~="rail"]` sets `--ws-rail-width` to it instead of
  `0`. Measured at 1440x860: `288px 800px 352px` → rail collapsed `32px 1056px 352px` (table
  +256 = 288 − 32) → both collapsed `32px 1376px 32px` (+576 = 640 − 64). A unit test pins that
  neither width is ever set to `0`
- [x] 8.3 The strip shows the pane's name as vertical text (`writing-mode: vertical-rl`),
  "Filters" and "Details". It must stay selectable text — not an image, not a rotated
  background — so it survives zoom and assistive technology — a plain `<span>` with real text
  content; only `writing-mode`/`text-orientation` change when collapsed, and the rule is asserted
  to contain no `rotate` and no `background-image`. Read back from the live page:
  `label.textContent` `"Filters"` / `"Details"` with `getComputedStyle(...).writingMode ===
  "vertical-rl"`, and both appear in the accessibility snapshot as text inside their pane
- [x] 8.4 Replace the wholesale `visibility: hidden` on a collapsed pane: the strip stays visible
  and focusable because it is now the only way back, while every other control in the pane stays
  out of the tab order. Re-run the real-browser Tab sweep from task 2.4 — the expected result has
  changed from "0 hits inside the pane" to "exactly the strip's button, and nothing else" — the
  rule is now `.workspace[data-collapsed~="rail"] .ws-rail > .ws-pane-content` (and the detail
  equivalent). **Sweep re-run in Chrome at 1440x860 with both panes collapsed**: 40 `Tab` presses
  cycled `theme-toggle → ws-timeline-summary → zoom-out-btn → ws-toggle-rail → viewport →
  ws-toggle-detail → …`; 6 landings inside `.ws-rail`, **every one `#ws-toggle-rail`**, and 6
  inside `.ws-detail`, **every one `#ws-toggle-detail`** — no other control in either pane was
  ever reached, out of 26 focusable elements in the rail's markup
- [x] 8.5 Move the attention indicator from the topbar toggle onto the strip's button (design
  D3/D1). Re-verify with the same two fixtures task 3.3 used — `mild-skips.jsonl` shows the
  indicator without expanding, `severe-skips.jsonl` expands the rail — the badge markup moved
  into `paneStrip("rail")`; `renderRailAttention()` is unchanged apart from its comment, because
  it always looked the button up by `data-attention-for` / the toggles map. Both fixtures re-run,
  numbers in 3.3: mild → rail stays 32px, badge visible **inside `.ws-pane-strip`** and not in
  the header, button `ws-pane-toggle attention` with the title; severe → rail expands to 288px
  with the diagnostics section open and the detail pane still collapsed
- [x] 8.6 Re-verify the stacked layout below the breakpoint: the strip has to make sense there
  too, or the stacked case needs its own treatment — decide and record which, do not leave it
  looking accidental — **decided: the stacked case gets its own treatment.** A vertical label in
  a full-width row would be nonsense, and the previous stacked rule (`display: none` on the whole
  block) would now hide the only reopen control, so below the breakpoint the strip becomes the
  horizontal bar the block collapses to and only `.ws-pane-content` gets `display: none`. The
  label reverts to `writing-mode: horizontal-tb` there. `display: none` is safe in the stacked
  case specifically — the document scrolls and the panes are `overflow: visible`, so there is no
  pane scroll offset for it to discard, which is the only reason `visibility` is used above the
  breakpoint. Measured at 900x800: collapsed rail = `900x27px` bar, content `display: none`,
  button 22x17px, click reopens to `900x650px`; `#panes=rail%2Bdetail` opened at 900px collapses
  both to their bars with both buttons on screen
- [x] 8.7 Re-run the browser checks that groups 2 and 3 recorded, since their measured numbers
  (`0px 1088px 352px` and so on) no longer describe the built behavior, and update that evidence
  in place rather than leaving numbers that no longer hold — 2.2, 2.3, 2.4, 2.5, 3.1, 3.3, 3.4,
  4.1, 4.2, 6.1, 6.2, 6.3, 6.4, 6.5 and 7.1/7.2 were edited in place against a fresh
  `bash scripts/build-frontend.sh && cargo build --release -p looq` build served over a pty
  (`script -q /dev/null ./target/release/looq --port 7822|7823|7824 --no-browser …`), in file mode
  and stream mode
- [x] 8.8 Update both READMEs and the `docs/devlog.md` entry for this change: the reversal and
  why. Keep the two READMEs in sync in the same pass — the **Collapsing the side panes** /
  **Сворачивание боковых панелей** paragraphs were rewritten together (per-pane button, the
  labelled strip, the stacked bar, the indicator now on the strip); the `panes=` grammar
  paragraphs were already correct and are untouched. The devlog entry's stale numbers were
  corrected and it gained the reversal, the tab-order consequence and the stacked decision
- [x] 8.9 Full suite: `cargo test --workspace`, `cargo fmt --all --check`,
  `cargo clippy --all-targets --all-features -- -D warnings`, `npm run typecheck`, `npm run test`,
  then `openspec validate collapsible-workspace-panes --strict` — all green, first run each, no
  port-binding flake: `cargo test --workspace` 19 + 27 + 114 + 33 = **193 passed, 0 failed**;
  `cargo fmt --all --check` clean; `cargo clippy --all-targets --all-features -- -D warnings`
  clean; `npm run typecheck` clean; `npm run test` **8 files / 98 tests passed** (was 8 / 95 —
  the three new `workspace-layout.test.ts` cases for the strip); `openspec validate
  collapsible-workspace-panes --strict` → `Change 'collapsible-workspace-panes' is valid`.
  `crates/looq/assets/` rebuilt from `web/` at the end, so the embedded bundle is not stale
