## Why

`frontend-visual-redesign` (archived) retuned colors and spacing only, explicitly leaving typography
and component markup untouched — it flagged D7 (monospace scope) as an open question that timed out
twice, and named "full markup overhaul" as a follow-up option rather than something it attempted.

The user has now resolved D7 directly: the whole UI should read as more distinctly technical/IT —
monospace everywhere, not just data surfaces. Combined with that, this change adopts the two other
markup-level touches that were scoped as "approach C" during the original brainstorm but deferred:
level badges in the entry table become single-letter circular indicators (closer to the reference's
feel) instead of full-word text pills, and the top bar gets an accent treatment. This is still
bounded — no sidebar, no persistent right-panel, no change to where the detail panel or filter bar
live on the page; those structural decisions from earlier changes stand.

Two things surfaced while grounding this in the current code (not assumed): `.topbar` has a
`background: var(--bg-elevated)` rule from `frontend-visual-redesign` but no padding, so it currently
renders as a tight patch behind the heading and theme-toggle button rather than a real strip — this
change fixes that as part of giving the top bar its accent. And the monospace font stack
(`ui-monospace, SFMono-Regular, Menlo, monospace`) is duplicated as a literal string across six-plus
CSS rules — extending it to `body` is also the moment to promote it to a `--font-mono` token instead
of adding a seventh copy.

## What Changes

- **Typography, project-wide monospace.** `body`'s `font-family` switches from
  `system-ui, -apple-system, sans-serif` to a shared `--font-mono` token, broadened to prefer more
  distinctly developer-associated system fonts before falling back (e.g. `"JetBrains Mono"`,
  `"Cascadia Code"`, `"SF Mono"`, `Menlo`, `Consolas`, `"Liberation Mono"`, `monospace`) — no
  `@font-face`/network font loading, purely a preference order over whatever's already installed,
  consistent with `theming`'s "No external resources" requirement. Every existing per-rule
  `font-family: ui-monospace...` declaration is removed in favor of inheriting from `body`.
- **Label typography.** `.filter-field-name` (the "level"/"service" field labels in the filter bar)
  gets uppercase + letter-spacing, a small technical-label cue, consistent with the new monospace
  baseline.
- **Level badges become single-letter circular indicators**, in the entry table only (not the filter
  chips, which keep full level names — they're interactive controls a user reads before clicking,
  and "service"/tag values aren't reducible to one letter at all). TRACE/DEBUG/INFO/WARN/ERROR/FATAL
  map to T/D/I/W/E/F — all six letters are already unique, no collision. The full level name stays
  available as accessible text (`aria-label` or equivalent) and a `title` tooltip, so this is a
  visual compression, not an information loss — deaf/blind-to-color and screen-reader users still get
  the real word.
- **Top bar accent.** `.topbar` gets real padding (fixing the current no-padding gap so it reads as
  a strip, not a patch), a `--accent`-colored bottom border, and a small accent-colored mark before
  the "looq" heading, echoing the reference's colored logo corner without adopting its browser-chrome
  treatment.
- No layout changes: detail panel stays inline below the table, filter bar stays a horizontal top
  strip, no sidebar, no persistent right panel. No change to the 24px table row height. No renaming
  of looq's level vocabulary.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `entry-table`: the "Columns" requirement (`openspec/specs/entry-table/spec.md`) already says
  "Levels SHALL be visually distinguishable." This change makes the level column's representation
  concrete — a single-letter, color-coded indicator — and adds the accessibility guarantee that the
  abbreviation never drops the full level name for assistive technology or on-hover inspection. See
  `design.md` and `specs/entry-table/spec.md` for the exact delta.

## Impact

- `web/src/style.css`: new `--font-mono` token (light and dark blocks, though the font stack itself
  doesn't differ by theme — one declaration is enough, see `design.md`), `body` font-family switch,
  removal of ~6 duplicate `font-family: ui-monospace...` declarations, `.filter-field-name`
  uppercase/letter-spacing, `.level-badge` reworked from a text pill to a fixed-size circle,
  `.topbar` padding + bottom border + accent mark styling.
- `web/src/components/looq-entry-table.ts`: the level-badge render call changes from full-name text
  content to a single letter, with an added accessible-name attribute.
- `web/index.html`: the accent mark before "looq" in `.topbar` (a small decorative element, not new
  application logic).
- No new dependencies. No behavior changes to filtering, search, live-tail, or parsing — this is a
  presentation-layer change plus the one accessibility guarantee named above.
