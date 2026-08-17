## 1. Tokens

- [x] 1.1 Update dark-mode tokens in `web/src/style.css`: `--bg` → `#0b0c0f`, `--border` opacity →
  `0.13`, `--code-bg` opacity → `0.06`, `--accent` → `#6e8bff`, `--error` → `#f0525a`, `--warn` →
  `#f5a742`. Leave `--fg`/`--muted` unchanged.
- [x] 1.2 Add `--bg-elevated` to both the light `:root` block (`#f7f7f8`) and both dark blocks
  (`#131417`, under `@media (prefers-color-scheme: dark)` and `[data-theme="dark"]`, per the
  existing pattern every other token follows so the explicit toggle keeps winning both directions).
- [x] 1.3 Add six new token pairs (`--level-trace`/`-bg` through `--level-fatal`/`-bg`) to light and
  both dark blocks, values from `design.md` D3. `-bg` variants: derive at a similar alpha to the
  existing `--error-bg`/`--warn-bg`/`--accent-bg` pattern (check their opacity in the current file
  and match it) rather than inventing a new alpha scheme.
- [x] 1.4 Apply the D5 spacing/radius deltas across `style.css`: filter-bar container, filter chip,
  error/confirm banner, detail panel paddings; container border-radius `6px`→`5px` (theme toggle,
  drop zone, filter bar, error/confirm banners, detail panel). Do not touch badge (`3px`)/pill
  (`999px`) radii or the 24px table row height.

## 2. Component wiring

- [x] 2.1 Repoint `.level-badge.level-*` rules in `style.css` (currently borrowing `--accent-fg`/
  `--accent-bg`, `--warn`/`--warn-bg`, `--error`/`--error-bg`, `--muted`/`--code-bg`) to the six new
  `--level-*`/`--level-*-bg` tokens, one rule per level (error, fatal/critical, warn/warning, info,
  debug, trace) — do not leave any two levels sharing a token.
- [x] 2.2 Add a `.topbar` background rule using `--bg-elevated`; confirm `body`'s existing
  `background: var(--bg)` rule is untouched so the two surfaces read as distinct.
- [x] 2.3 `web/src/components/looq-timeline.ts`: unfiltered (background) series fill opacity
  `0.45`→`~0.25` (same gray hue, e.g. `rgba(148, 163, 184, 0.25)`); filtered (foreground) series
  fill/stroke switch from the hardcoded blue to `--accent`'s new value. Read the CSS custom
  property at render time (however the component currently sources colors — check the existing
  pattern) rather than hardcoding the new hex directly, so a future token change doesn't require
  touching this file again.
- [x] 2.4 `web/src/components/looq-live-tail.ts`: LIVE and DISCONNECTED indicator colors switch
  from their current one-off hex values to `--level-info`/`--level-error`. Keep the existing
  4-state structure (live/connecting/ended/disconnected) — no new states, no markup changes.
  (Done in `style.css`'s `.conn-indicator.conn-live`/`.conn-disconnected` rules — that's where the
  colors actually live; `looq-live-tail.ts` only supplies state labels, no color constants. See
  devlog "Deviation" note.)

## 3. Visual verification (real browser, both appearances)

- [x] 3.1 Build the frontend and run the app against a real fixture (Playwright, per the pattern
  used in every prior UI change in this project) in dark mode: confirm all six level badges are
  visually distinct at a glance, confirm the top bar reads as separated from the body, confirm the
  timeline's two series are both legible against the new `#0b0c0f` background. This is the check
  for the new `theming` scenario "Every level has its own color" — a fixture with all six levels
  present, badge colors compared pairwise, not just eyeballed.
- [x] 3.2 Resolve D4's open number: try the filtered-series opacity at a few values against the
  real dark background and pick one that reads clearly without overpowering the muted background
  series — record the chosen value and a one-line reason in `docs/devlog.md`. (fill 0.6 / stroke
  0.9 — see devlog.)
- [x] 3.3 Repeat 3.1 in light mode: confirm the six level badges, the top bar/body separation, and
  the timeline series are all legible against the light palette too — light mode got less design
  scrutiny going in and this is where a real problem would surface.
- [x] 3.4 Confirm the explicit theme toggle still overrides system preference correctly in both
  directions (the existing `theming` behavioral requirements are unchanged by this work, but verify
  the new tokens didn't accidentally break the override mechanics).
- [x] 3.5 Sanity check the tightened spacing (D5) doesn't read as cramped next to the already-dense
  24px table rows — if it does, ease back the percentage rather than shipping a worse reading
  experience than before; this was flagged as unverified in `design.md`'s Risks section. (Held up
  as-is, no easing needed — see devlog.)
- [x] 3.6 If reachable, ask the user to resolve D7 (typography scope — data-only monospace vs.
  monospace everywhere) rather than assuming the default a second time. If unreachable, proceed
  with the existing split (already the current behavior) and flag D7 as still-open in the final
  report, same as `design.md` does. (No live human reachable in this run — proceeded with the
  existing split, D7 still open.)

## 4. Regression check

- [x] 4.1 Run the existing test suites unmodified (`cargo test --workspace`, the TypeScript test
  suite) — this change should not need new automated assertions or break existing ones, since no
  requirement changed, only values. If something breaks, that's a sign a "non-behavioral" edit
  accidentally touched behavior — stop and investigate rather than adjusting the test to match.
  (One flaky timing failure on first run, unrelated to this change — see devlog; clean on re-run.)
- [x] 4.2 `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `npm run typecheck` all clean.

## 5. Wrap-up

- [x] 5.1 `docs/devlog.md` entry: what changed, the resolved D4 opacity value with reasoning, the
  D7 outcome (resolved or still-open), and any deviation from `design.md`'s exact numbers found
  necessary during the visual check (3.5).
- [x] 5.2 No README changes expected — this is a visual-only change with no new install steps,
  commands, flags, or documented limitations. Confirm this holds rather than skipping the check.
  (Confirmed — both READMEs' Theme sections describe mechanics only, no specific colors/spacing.)
- [x] 5.3 `openspec validate frontend-visual-redesign --strict` passes.
- [x] 5.4 Archive the change.
