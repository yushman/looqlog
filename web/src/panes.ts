// The workspace's collapsible side panes (`app-shell` spec, "Three-pane workspace
// layout"; design.md D2/D6).
//
// Same split as `column-widths.ts`, and for the same reason: two unrelated places
// need the same vocabulary — the component that renders the toggles, and
// `url-hash.ts`, which has to read a pane name out of a shared link without
// importing a Web Component (and without a DOM at all, which is what makes the
// fall-back rules testable in `vitest`).

/** The panes that can be put away. The centre column is not one of them: it holds
 * the log itself, which is the thing the other two are collapsed to make room for. */
export const COLLAPSIBLE_PANES = ["rail", "detail"] as const;

export type CollapsiblePane = (typeof COLLAPSIBLE_PANES)[number];

export type CollapsedPanes = ReadonlySet<CollapsiblePane>;

export function isCollapsiblePane(name: string): name is CollapsiblePane {
  return (COLLAPSIBLE_PANES as readonly string[]).includes(name);
}

/** Serialises to the `panes=` value, or `null` when nothing is collapsed (design
 * D6): the key names what is *collapsed*, so the default state — both panes
 * expanded — is absent from the link entirely, exactly like all-default column
 * widths. Order comes from `COLLAPSIBLE_PANES`, not from insertion, so the same
 * view always produces the same URL. */
export function encodeCollapsedPanes(panes: CollapsedPanes): string | null {
  const names = COLLAPSIBLE_PANES.filter((pane) => panes.has(pane));
  return names.length === 0 ? null : names.join("+");
}

export interface ParsedCollapsedPanes {
  panes: Set<CollapsiblePane>;
  /** What could not be applied, phrased for the shared-URL notice. Always a
   * *report*: a collapsed pane is a presentation preference, never a reason for a
   * log not to load (`url-state` spec, "An unknown pane name never blocks the
   * log"). */
  errors: string[];
}

/** Parses a `panes=` value. Never throws.
 *
 * `+` is the documented separator (design D6), but in an
 * `application/x-www-form-urlencoded` fragment a literal `+` means a space, so
 * `encodeCollapsedPanes`'s output travels as `%2B` and a hand-typed
 * `#panes=rail+detail` arrives here as `rail detail`. Splitting on `+`, `,` and
 * whitespace alike accepts all three spellings rather than reporting a URL the
 * user typed exactly as documented. */
export function parseCollapsedPanes(raw: string | null): ParsedCollapsedPanes {
  if (raw === null) {
    return { panes: new Set(), errors: [] };
  }
  const names = raw.split(/[+,\s]+/).filter((name) => name.length > 0);
  const panes = new Set<CollapsiblePane>();
  const errors: string[] = [];
  for (const name of names) {
    if (isCollapsiblePane(name)) {
      panes.add(name);
    } else {
      // Dropped for this entry only — the rest of the value still applies.
      errors.push(`collapsed pane "${name}" is not a pane name`);
    }
  }
  if (names.length === 0) {
    errors.push(`collapsed panes value "${raw}" names no pane`);
  }
  return { panes, errors };
}
