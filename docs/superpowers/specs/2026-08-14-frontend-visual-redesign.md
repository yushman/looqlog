# Frontend visual redesign — palette + density

**Status:** Draft, pending user review
**Date:** 2026-08-14

## Context

looq's frontend has never had a deliberate visual pass beyond what each OpenSpec change needed
to satisfy its own spec. That said, `web/src/style.css` is not a placeholder: it already
implements a complete light+dark token system per the `theming` capability
(`openspec/specs/theming/spec.md`) — `--bg`/`--fg`/`--muted`/`--border`/`--code-bg`,
semantic `--accent`/`--error`/`--warn` (each with `-fg`/`-bg` variants), `--success`, `--hl-bg`
for search highlighting — defined once on `:root` and re-pointed under both
`@media (prefers-color-scheme: dark)` and `:root[data-theme="dark"]`, with `system-ui` for UI
chrome and `ui-monospace` for data-dense surfaces already correctly separated. Every other
component (filter bar, timeline, live-tail indicators, detail panel, error/confirm banners,
diagnostics) is already built from these tokens rather than hardcoded colors.

The user supplied `docs/ui.png`, a screenshot of an unrelated tool ("LogVue"), as a reference
for palette and density only — not as a layout or structure template. Explicit instruction:
adopt the "vibe" (darker, denser, more saturated), not the reference's specific element
positions (its left sidebar for filters, persistent right-side detail panel, browser-chrome
top bar). looq keeps its own structural decisions from earlier changes: the detail panel opens
inline below the table (`timeline-and-table` design.md D6), the filter bar is a horizontal strip
above the table (`filtering-and-search`), not a sidebar.

This is a design document only. No application code changes here.

## Goals / Non-Goals

**Goals:**
- A darker, more saturated dark-mode palette that reads closer to the reference's density and
  contrast, applied purely at the CSS custom-property level.
- Six visually distinct level-badge colors — today INFO borrows `--accent` and both TRACE and
  DEBUG borrow `--muted`, so two of six levels are indistinguishable at a glance. Fix this with
  dedicated tokens, not a side effect of the palette change.
- A tighter spacing scale (~15–20% less padding/margin throughout) so the UI reads as dense as
  the reference without touching anything that has a hard technical reason to stay fixed.
- Light mode stays fully specified (the `theming` spec still requires it, unconditionally) but
  gets proportionally less design attention than dark, since the reference and the user's stated
  priority are both dark-first.

**Non-Goals:**
- No component markup/DOM changes anywhere. This is a token-and-value change, not a
  restructuring.
- No layout changes: detail panel stays inline below the table, filter bar stays a horizontal
  top strip. The reference's sidebar and persistent right panel are explicitly not adopted.
- No change to the 24px table row height — it's hardcoded for the virtual-scroll math in
  `web/src/components/looq-entry-table.ts` and touching it is a separate, riskier change than a
  palette pass.
- No renaming of looq's level vocabulary. The canonical levels are TRACE/DEBUG/INFO/WARN/ERROR/
  FATAL (`crates/looq-core/src/level.rs`), not the reference's VERBOSE/ASSERT naming — the
  redesign maps colors onto looq's real levels, it does not import the reference's terms.
- No changes to `theming`'s behavioral requirements (system-preference default, explicit toggle,
  persistence, legibility in both modes, no external resources) — this stays a value-only change
  within that existing contract.

## Decisions

### D1 — Darker, higher-contrast dark palette

Current dark `--bg` (`#121212`) is a mid-charcoal; the reference reads much closer to
near-black. New values:

| Token | Current | New |
|---|---|---|
| `--bg` | `#121212` | `#0b0c0f` |
| `--border` opacity | `0.22` | `0.13` |
| `--code-bg` opacity (alt row stripe) | `0.1` | `0.06` |
| `--accent` | `#7aa8ff` | `#6e8bff` |
| `--error` | `#ff8a80` | `#f0525a` |
| `--warn` | `#ffca7a` | `#f5a742` |

`--border` and `--code-bg` move to lower opacity because the reference's dividers and row
stripes are barely-there — the current 0.22/0.1 values read as noticeably harder edges next to
it. `--accent` shifts a few degrees toward indigo/violet, a nod to the reference's purple accent
without literally copying it (the reference's purple is spent entirely on the timeline bars and
the Assert badge, not on a general UI accent). `--error` and `--warn` move to more saturated
values matching the reference's vivid badge colors — the current coral-toned `#ff8a80` reads as
soft next to it.

`--fg` (`#e9e9e9`) and `--muted` (`#a8abb3`) are left as-is: they already read as neutral/legible
against either the old or new `--bg`, and the audit found no complaint about foreground text
contrast, only about background/border harshness.

### D2 — New `--bg-elevated` token

Add `--bg-elevated` (`#131417` dark, `#f7f7f8` light), a small step up from `--bg`, for the top
bar only. Today the top bar shares `--bg` with the body, so there is no visual separation between
"app chrome" and "content" — the reference's browser-tab-plus-app-header treatment reads as more
structured because that boundary exists. This is the one genuinely new token in this design; the
existing `body { background: var(--bg) }` rule is unaffected — `.topbar` gets its own background
declaration.

### D3 — Six dedicated level-badge tokens, not four borrowed ones

Today `.level-badge` styling (`web/src/style.css:532-560`) maps:

- error/fatal/critical → `--error` / `--error-bg`
- warn/warning → `--warn` / `--warn-bg`
- info → `--accent-fg` / `--accent-bg`
- debug/trace → `--muted` / `--code-bg`

Two problems: INFO's color is really "the UI's interactive-element color," which is a coincidence
of reuse, not a design choice — if `--accent` ever changes for unrelated UI reasons, INFO's
badge changes with it. And TRACE and DEBUG are visually identical, which defeats the purpose of a
six-level severity scale.

New tokens, each with a `-bg` variant, dark values first (light values in the table below):

| Level | Dark | Light |
|---|---|---|
| `--level-trace` | `#7d818a` | `#5b5f66` |
| `--level-debug` | `#5b8cff` | `#2f6fed` |
| `--level-info` | `#4ade80` | `#15803d` |
| `--level-warn` | `#f5a742` | `#a35a00` |
| `--level-error` | `#f0525a` | `#c22a31` |
| `--level-fatal` | `#c084fc` | `#7c3aed` |

This gives a gray→blue→green→amber→red→purple ramp that tracks severity monotonically by hue as
well as by position, and echoes the reference's badge coloring (which uses the same rough
gray/blue/green/orange/red/purple spread across its six levels) without adopting its level
*names*. `.level-badge.level-*` rules in `style.css` point at these instead of `--accent`,
`--warn`, `--error`, `--muted`. `--level-info`'s light value (`#15803d`) intentionally matches the
existing `--success` token — both mean "nominal/good" in this palette, and giving them the same
value is a deliberate small consistency, not a coincidence to fix later.

### D4 — Timeline series: muted background, accent foreground

`web/src/components/looq-timeline.ts` (`filtering-and-search`/`timeline-and-table`) already
renders two series — the unfiltered background histogram and the filtered foreground histogram —
added when a predicate is active. Current values:

- background (unfiltered): fill `rgba(148, 163, 184, 0.45)`, stroke `rgba(100, 116, 139, 0.6)`
- foreground (filtered): fill `rgba(37, 99, 235, 0.55)`, stroke `rgba(37, 99, 235, 0.9)`

New: background fill opacity drops to `~0.25` (same gray hue, less prominent — it should read as
context, not a competing series), foreground switches from its hardcoded blue to the new
`--accent` value (`#6e8bff`) at a comparable or slightly higher opacity than today. This was a
direct choice among three options presented to the user — level-colored bars, a pure two-tone
texture with no data meaning (closest to the reference's literal look), or "neutral accent
foreground, muted background" — and the third was picked explicitly, because it keeps the
filtered/unfiltered distinction (which is real information, not decoration) while still reading
as calmer/more monochrome than the current fully-saturated blue-on-gray.

Exact numeric opacity for the foreground series is left to implementation-time visual check
against the new darker `--bg` — a fixed value picked at design time without testing it against
`#0b0c0f` risks being either washed out or overpowering; the direction (accent-colored,
comparable-or-higher prominence than the current blue) is the actual decision.

### D5 — Density: ~15–20% tighter spacing, radius pulled in one notch

Representative before/after (full list applied uniformly across `style.css`, not just these
examples):

| Element | Current padding | New |
|---|---|---|
| Filter-bar container | `0.5rem 0.75rem` | `0.4rem 0.6rem` |
| Filter chip | `0.15em 0.6em` | `0.12em 0.5em` |
| Error/confirm banner | `0.6em 0.9em` | `0.5em 0.75em` |
| Detail panel | `0.75rem` | `0.6rem` |

Container `border-radius` goes from `6px` to `5px` (theme toggle, drop zone, filter bar, error/
confirm banners, detail panel); badge/pill radii (`3px` badges, `999px` pills) are unchanged —
they're already small/fully-rounded and the reference doesn't read as sharper at that scale.

The 24px table row height is explicitly **not** touched (see Non-Goals) — it's already the
densest element on the page and it's load-bearing for virtual-scroll arithmetic elsewhere in
`looq-entry-table.ts`.

### D6 — Live-tail indicator colors follow the new palette, structure unchanged

The four connection-state colors (live/connecting/ended/disconnected) keep their existing
four-state design (`live-tail` capability) but get retuned: LIVE's green and DISCONNECTED's red
move to sit alongside the new `--level-info`/`--level-error` values rather than their own
one-off hardcoded hex codes, so the palette has one green and one red meaning "good" and "bad"
consistently across level badges and connection state, instead of two unrelated pairs that
happen to both be green/red.

### D7 — Typography: no change, assumption flagged

The existing split (`ui-monospace` for the table, timestamps, level badges, detail panel,
diagnostics; `system-ui` for buttons, chips, labels, headings) is kept as-is. **This is a
best-judgment assumption, not a confirmed decision** — the user was asked explicitly whether to
extend monospace to all UI chrome (matching the reference's uniform terminal typography) or keep
the current data/chrome split, and the question went unanswered twice (no response after two
10-minute windows). The current split matches what was already implemented before this design
started, so it was kept by default rather than changed speculatively. See Open Questions.

### D8 — Everything else inherits the palette for free

Error/confirm banners, the detail panel, diagnostics/detection warnings, and the drop-zone
drag-active state are all already built from the semantic tokens (`--error`, `--warn`,
`--accent`, `--border`, `--code-bg`) rather than hardcoded values. None of them need direct
edits — D1–D3's token changes propagate automatically. This is the main reason "Approach A"
(palette + density only, no markup changes) was viable at all: the existing CSS was disciplined
enough about tokens that a redesign this size doesn't need to touch component structure.

## Risks / Trade-offs

- **Six new tokens double the level-color surface to keep in sync.** Every future palette
  adjustment now has to consider 6 dark + 6 light level values in addition to the existing
  semantic tokens. Accepted because the alternative (TRACE/DEBUG sharing a color) is a real
  legibility defect, not a hypothetical one.
- **Tightened spacing risks feeling cramped**, especially combined with the already-dense 24px
  rows and the darker background increasing perceived contrast. Not verified visually as part of
  this design — implementation should do a real side-by-side check before committing to the
  specific percentages above, and treat them as a starting point, not a spec to hit exactly.
- **The timeline foreground series opacity is unresolved** (D4) — picked a direction, not a
  number, deliberately, because the right number depends on how it looks against the new,
  darker `--bg`, which doesn't exist as a rendered surface yet.
- **The typography decision (D7) rests on an unanswered question.** If the user's actual
  preference was "monospace everywhere," this design under-delivers on the reference's terminal
  aesthetic in a way that's easy to miss during review since nothing looks broken, just less
  literal to the reference than it could be.
- **Light mode gets a same-shape-different-values treatment with less scrutiny than dark.** The
  light-mode `--level-*` values in D3 are picked by the same eyeball-consistency method as the
  rest of the existing light palette (darker/more saturated than the dark equivalents, for
  contrast against white), but they haven't been checked against a rendered page any more than
  the timeline opacity has.

## Open Questions

- **Typography scope** (D7): monospace for data only (current, kept by default) vs. monospace
  everywhere including UI chrome, matching the reference's uniform terminal look. Needs an actual
  answer before or during implementation, not another assumption.
- **Exact timeline foreground opacity** (D4): a number to be picked by looking at it against the
  new `--bg`, not by further discussion.
- **Whether `--bg-elevated` should extend to any surface beyond the top bar** (e.g. the filter
  bar or detail panel) once it exists and can be seen — out of scope to speculate about now.
