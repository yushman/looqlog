## 1. Typography token

- [x] 1.1 Add `--font-mono` to `web/src/style.css` (one declaration, not duplicated per theme block
  — it's a font stack, not a color): `ui-monospace, "JetBrains Mono", "Cascadia Code", "SF Mono",
  Menlo, Consolas, "Liberation Mono", monospace`.
- [x] 1.2 `body`'s `font-family` switches from `system-ui, -apple-system, sans-serif` to
  `var(--font-mono)`.
- [x] 1.3 Remove every other `font-family: ui-monospace, SFMono-Regular, Menlo, monospace;`
  declaration in `style.css` (currently on `.status`, `.conn-indicator`, entry-row styles, detail
  panel, diagnostics — grep for the literal string to find all of them; there were six-plus before
  this task). Confirm via computed style (Playwright `getComputedStyle`, or visually) that every
  element that previously had an explicit declaration renders identically after inheriting from
  `body` instead.

## 2. Label typography

- [x] 2.1 `.filter-field-name` gains `text-transform: uppercase; letter-spacing: 0.04em;`. Do not
  apply this treatment to any other element — it's a deliberate label convention, not a general
  typographic rule.

## 3. Level badges → single-letter circles (entry table only)

- [x] 3.1 `web/src/components/looq-entry-table.ts`: change the level-badge render from
  `<span class="level-badge level-${level}">${fullName}</span>` to a version whose visible text is
  the first letter (TRACE→T, DEBUG→D, INFO→I, WARN→W, ERROR→E, FATAL→F — confirm all six stay
  unique, they should) and which carries the full name as `aria-label` and as `title`. Keep the
  existing `level-${level}` class untouched — only the text content and the two new attributes
  change.
- [x] 3.2 `web/src/style.css`: rework `.level-badge` from a padded text pill to a fixed-size circle
  (`display: inline-flex; align-items: center; justify-content: center; width: 1.6em; height: 1.6em;
  border-radius: 50%;`, drop `padding: 0 0.4em`). The six `.level-badge.level-*` color rules
  (background/color from the `--level-*` tokens) stay as-is — do not touch them.
- [x] 3.3 Do **not** apply this treatment to filter-bar level chips (`looq-filter-bar.ts`) — they
  keep full level-name text. Verify they're untouched, don't assume.
- [x] 3.4 Verify with a real fixture containing all six levels, in both light and dark: each circle
  is legible, the letter is readable against its background color, hovering shows the full name as a
  tooltip, and an accessibility tree snapshot (Playwright `browser_snapshot`, which reads
  accessible names) shows the full level word, not the single letter, for each badge.

## 4. Top bar accent

- [x] 4.1 `web/src/style.css`: add real padding to `.topbar` (start from `0.5rem 0.75rem`, matching
  the density scale from `frontend-visual-redesign`; adjust if it reads wrong against the new
  border in 4.2 — this is an implementation-time judgment call, not a number to treat as exact).
- [x] 4.2 Add `border-bottom: 2px solid var(--accent);` to `.topbar`.
- [x] 4.3 Add a small accent-colored mark before the `<h1>looq</h1>` text in `web/index.html` (a
  `::before` pseudo-element on `h1` or a small `<span class="brand-mark">` — implementer's choice;
  roughly a `0.6em` filled square or dot, colored `var(--accent)`). Keep it purely decorative — no
  new application logic, no new component.
- [x] 4.4 Verify in both light and dark: `.topbar` now reads as a visible strip (not a tight patch
  around the heading and button), the accent border and mark are visible against both `--bg` and
  `--bg-elevated`, and nothing about the page's overall width/centering changed (no full-bleed — that
  would be a layout change, out of scope).

## 5. Spec housekeeping (opportunistic, not scope expansion)

- [x] 5.1 `openspec/specs/entry-table/spec.md`'s `## Purpose` section currently reads
  "TBD - created by archiving change timeline-and-table. Update Purpose after archive." — since this
  change already edits `entry-table`'s delta, write a real one-paragraph Purpose when archiving (follow
  the style of `openspec/specs/theming/spec.md`'s Purpose section as a model). Do **not** fix the
  same stale-Purpose issue on `entry-index`, `filtering`, `search`, `timeline`, or `url-state` — those
  are untouched by this change; note them in `docs/devlog.md`'s `## Ideas for later` instead, don't
  silently expand scope to fix them here.

## 6. Regression check

- [x] 6.1 `cargo test --workspace` (should be entirely unaffected — no Rust touched), the TypeScript
  test suite, `npm run typecheck`, `cargo fmt --check`, `cargo clippy --workspace --all-targets --
  -D warnings` all clean.
- [x] 6.2 Re-run the golden-path checkpoint informally: open a file, filter, search, live-tail — in
  both themes — confirm nothing about the visual changes broke interaction (e.g. the smaller circular
  badge is still clickable/selectable the same way the old pill was, if entry selection depends on
  clicking the badge area specifically — check this rather than assuming click targets are unaffected
  by the size/shape change).

## 7. Wrap-up

- [x] 7.1 `docs/devlog.md` entry: what changed, the D7 resolution (now: monospace everywhere, per
  direct user instruction), any implementation-time judgment calls made (topbar padding value, accent
  mark exact size/shape), and the five stale-Purpose specs noted for later (task 5.1).
- [x] 7.2 Confirm no README changes needed (visual-only change, same as `frontend-visual-redesign` —
  check, don't assume, per that change's own precedent).
- [x] 7.3 `openspec validate frontend-terminal-overhaul --strict` passes.
- [x] 7.4 Archive the change.
