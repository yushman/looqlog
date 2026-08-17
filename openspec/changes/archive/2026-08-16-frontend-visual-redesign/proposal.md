## Why

looq's frontend has never had a deliberate visual pass beyond what each OpenSpec change needed
to satisfy its own spec. `web/src/style.css` is already a complete, disciplined light+dark token
system (per the `theming` capability) with every component built from semantic tokens rather than
hardcoded colors — but the values themselves read as a mid-charcoal dark mode with two of six log
levels (TRACE, DEBUG) visually indistinguishable, and a spacing scale looser than the density this
kind of dense, data-heavy viewer wants.

This change retunes the token values and tightens spacing to a darker, denser, more legible
palette, worked out in `docs/superpowers/specs/2026-08-14-frontend-visual-redesign.md` (brainstorm
design doc — read that first, this proposal and the accompanying `design.md` summarize it into the
OpenSpec artifact shape). Almost all of it is a value-only change inside the existing `theming`
contract (system preference default, explicit toggle, persistence, no external resources — all
unaffected). One piece is not cosmetic: `entry-table`'s "Columns" requirement already says "Levels
SHALL be visually distinguishable," and `theming` already requires "entry levels... remain
distinguishable" in both appearances — but TRACE and DEBUG currently share a color, which is a real
gap against both of those already-accepted requirements, not a style preference. This change closes
it and tightens `theming`'s requirement to say so concretely (see Capabilities below).

## What Changes

- New darker dark-mode `--bg` (`#0b0c0f`), lower-opacity `--border`/`--code-bg`, retuned `--accent`/
  `--error`/`--warn` toward the reference's higher saturation.
- New `--bg-elevated` token (dark `#131417`, light `#f7f7f8`) giving the top bar its own background,
  distinct from the body — the one genuinely new token in this change.
- Six new dedicated `--level-*` tokens (trace/debug/info/warn/error/fatal, each with a `-bg`
  variant, dark and light values) replacing the current scheme where `.level-badge` styling borrows
  `--accent`/`--warn`/`--error`/`--muted` — fixes TRACE and DEBUG currently sharing a color.
- Timeline series recolor: unfiltered background series fill opacity drops (~0.45 → ~0.25),
  filtered foreground series switches from its hardcoded blue to `--accent`. Exact foreground
  opacity is a visual-check-at-implementation-time value, not fixed here.
- ~15–20% tighter padding/margin across containers, chips, banners, and the detail panel;
  container `border-radius` 6px → 5px. The 24px table row height is explicitly untouched.
- Live-tail connection-state colors (live/connecting/ended/disconnected) retuned to align with the
  new `--level-info`/`--level-error` values instead of their own one-off hex codes.
- No component markup/DOM changes. No layout changes (detail panel stays inline below the table,
  filter bar stays a horizontal top strip — not the reference's sidebar/persistent-panel layout).
  No renaming of looq's TRACE/DEBUG/INFO/WARN/ERROR/FATAL level vocabulary.

## Capabilities

### New Capabilities

None.

- `theming`: the "Light and dark appearance" requirement gains a concrete guarantee — each of the
  six log levels renders with its own distinct color in both appearances, no two sharing one. This
  isn't a new behavior invented for this change; it's tightening an existing vague scenario
  ("entry levels... remain distinguishable") into something checkable, because the current
  implementation violates it (TRACE and DEBUG currently share a color) and `entry-table`'s
  "Columns" requirement already separately says "Levels SHALL be visually distinguishable." D3 in
  `design.md` is what closes this gap. Every other `theming` requirement (system-preference
  default, explicit persisted toggle, no external resources) is untouched.

No other capability's requirements are touched: no markup, no layout, no new interactive behavior.

## Impact

- `web/src/style.css`: token value changes (`:root` and both dark blocks), new `--bg-elevated` and
  six `--level-*`/`--level-*-bg` token pairs, `.level-badge.level-*` rules repointed, spacing/radius
  values tightened throughout, `.topbar` background rule added.
- `web/src/components/looq-timeline.ts`: series color/opacity constants updated.
- `web/src/components/looq-live-tail.ts`: connection-state color constants updated to reference the
  new tokens instead of their current one-off hex values.
- No other files change. No new dependencies. No test behavior changes — existing Playwright/vitest
  coverage should pass unmodified; this change adds a manual/Playwright visual check in both
  appearances as its own verification step (see `tasks.md`), not new automated assertions, since
  there's no new requirement to assert against.
