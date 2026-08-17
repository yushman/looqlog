## Context

Full rationale lives in the brainstorm design doc,
`docs/superpowers/specs/2026-08-14-frontend-visual-redesign.md` — this file summarizes it into the
OpenSpec shape and is the one `tasks.md` is written against. Read the brainstorm doc for the
per-decision trade-off discussion; this is the condensed version.

The user supplied `docs/ui.png` (a screenshot of an unrelated tool, "LogVue") as a palette/density
reference only — explicitly not a layout template. looq keeps its existing structural decisions:
detail panel inline below the table (`timeline-and-table` D6), filter bar a horizontal strip above
the table (`filtering-and-search`), not the reference's sidebar/persistent-right-panel layout.

## Goals / Non-Goals

**Goals:**
- Darker, higher-contrast dark-mode palette closer to the reference's density and saturation.
- Six visually distinct level-badge colors (today INFO borrows `--accent`, TRACE and DEBUG both
  borrow `--muted` — a real legibility defect, not a hypothetical one).
- ~15–20% tighter spacing throughout, without touching anything with a hard technical reason to
  stay fixed.
- Light mode stays fully specified (still required, unconditionally, by `theming`) with
  proportionally less design scrutiny than dark.

**Non-Goals:**
- No component markup/DOM changes anywhere — token-and-value change only.
- No layout changes — detail panel and filter-bar placement are unchanged.
- No change to the 24px table row height (hardcoded for virtual-scroll math in
  `looq-entry-table.ts`).
- No renaming of looq's level vocabulary (TRACE/DEBUG/INFO/WARN/ERROR/FATAL stays as-is).
- No change to `theming`'s behavioral requirements — value-only, inside the existing contract.

## Decisions

### D1 — Darker, higher-contrast dark palette

`--bg` `#121212`→`#0b0c0f`; `--border` opacity `0.22`→`0.13`; `--code-bg` opacity `0.1`→`0.06`;
`--accent` `#7aa8ff`→`#6e8bff`; `--error` `#ff8a80`→`#f0525a`; `--warn` `#ffca7a`→`#f5a742`. `--fg`
and `--muted` unchanged — no contrast complaint found against either background.

### D2 — New `--bg-elevated` token

`#131417` dark / `#f7f7f8` light, applied to `.topbar` only, giving app chrome a background
distinct from body content — the one new token this change introduces.

### D3 — Six dedicated `--level-*` tokens

Replaces the current borrowed scheme (`error`/`fatal`/`critical`→`--error`, `warn`/`warning`→
`--warn`, `info`→`--accent-fg`, `debug`/`trace`→`--muted`) with dedicated tokens per level, each
with a `-bg` variant:

| Level | Dark | Light |
|---|---|---|
| `--level-trace` | `#7d818a` | `#5b5f66` |
| `--level-debug` | `#5b8cff` | `#2f6fed` |
| `--level-info` | `#4ade80` | `#15803d` |
| `--level-warn` | `#f5a742` | `#a35a00` |
| `--level-error` | `#f0525a` | `#c22a31` |
| `--level-fatal` | `#c084fc` | `#7c3aed` |

Gray→blue→green→amber→red→purple, tracking severity monotonically by hue. `--level-info` (light)
intentionally matches the existing `--success` value — both mean "nominal," deliberately shared.

### D4 — Timeline: muted background, accent foreground

Background (unfiltered) series fill opacity `0.45`→`~0.25`, same hue. Foreground (filtered) series
switches from its hardcoded blue to `--accent`. Chosen over level-colored bars and a pure two-tone
texture because the filtered/unfiltered distinction is real information, not decoration, and should
stay legible while reading calmer than the current fully-saturated blue-on-gray. **Exact foreground
opacity is an implementation-time visual check against the new `#0b0c0f` background, not a number
fixed here** — see `tasks.md`.

### D5 — Density: ~15–20% tighter spacing

Representative deltas (applied uniformly, not just at these examples): filter-bar container
`0.5rem 0.75rem`→`0.4rem 0.6rem`; filter chip `0.15em 0.6em`→`0.12em 0.5em`; error/confirm banner
`0.6em 0.9em`→`0.5em 0.75em`; detail panel `0.75rem`→`0.6rem`. Container `border-radius` `6px`→
`5px`; badge (`3px`) and pill (`999px`) radii unchanged. 24px row height untouched.

### D6 — Live-tail indicator colors follow the new palette

Four-state structure (live/connecting/ended/disconnected) unchanged; LIVE green and DISCONNECTED
red move to sit alongside `--level-info`/`--level-error` instead of their own one-off hex values —
one green and one red meaning "good"/"bad" consistently across badges and connection state.

### D7 — Typography: unchanged

Existing split kept: `ui-monospace` for data (table, timestamps, badges, detail panel,
diagnostics), `system-ui` for UI chrome. **This is a best-judgment default, not a confirmed user
decision** — the clarifying question timed out twice during brainstorming. If the actual preference
turns out to be "monospace everywhere," that's a follow-up change, not a blocker for this one.

### D8 — Everything else inherits the palette for free

Error/confirm banners, detail panel, diagnostics/detection warnings, drop-zone drag-active state
are already built from semantic tokens — D1–D3 propagate to them with no direct edits needed. This
is why "palette + density, no markup changes" is viable as a single change at all.

## Risks / Trade-offs

- Six new tokens double the level-color surface to keep in sync (12 values instead of borrowing 4
  existing ones) — accepted because TRACE/DEBUG sharing a color is a real defect.
- Tightened spacing risks feeling cramped combined with the already-dense 24px rows and increased
  background contrast — not verified visually as part of the design; `tasks.md` requires a real
  side-by-side check before treating the percentages as final.
- D4's timeline opacity and D7's typography scope are both open (see below) — implementation must
  resolve D4 by looking at it, and should not silently resolve D7 by assumption a second time if
  the user is reachable.
- Light-mode `--level-*` values (D3) are picked by the same eyeball-consistency method as the rest
  of the existing light palette, not checked against a rendered page yet.

## Migration Plan

Direct edit to `web/src/style.css` token values and the two component files listed in the
proposal's Impact section. No data migration, no API change, no feature flag — a CSS custom
property value change ships the moment the frontend bundle rebuilds. Rollback is a revert of the
same files if the new values read worse in practice than expected.

## Open Questions

- **Typography scope** (D7): data-only monospace (current default) vs. monospace everywhere. Ask
  the user again before finalizing if there's a chance to.
- **Exact timeline foreground opacity** (D4): pick by looking at it against `#0b0c0f`, not by
  further discussion.
- **Whether `--bg-elevated` should extend beyond the top bar** (filter bar, detail panel) — out of
  scope to decide without seeing the top bar change first.
