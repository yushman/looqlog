// The entry table's column-width model (`entry-table` spec, "Columns can be
// resized"; design.md D1/D5/D6).
//
// It lives in its own module rather than inside `looq-entry-table.ts` because two
// unrelated places need the same numbers: the component that renders and drags
// them, and `url-hash.ts`, which has to clamp a width that arrived from a shared
// link without importing a Web Component (and without a DOM at all — that is what
// makes the clamping and fall-back rules testable in `vitest`).
//
// Widths are in `rem`, not pixels: the table's type scales with the root font
// size, so a pixel width copied into a link would mean a different fraction of a
// column on a machine with a different browser font setting.

/** The columns the user can drag directly, in the order they appear in the grid
 * and in the `cols=` hash value. The message column is deliberately absent: it is
 * `minmax(0, 1fr)` and absorbs whatever these three leave (design D1), so it has
 * no width of its own to encode. */
export const RESIZABLE_COLUMNS = ["ordinal", "timestamp", "level"] as const;

export type ResizableColumn = (typeof RESIZABLE_COLUMNS)[number];

/** Width in `rem` per resizable column. */
export type ColumnWidths = Readonly<Record<ResizableColumn, number>>;

/** The widths the table has always shipped with — the fixed template these
 * variables replace was `3rem 12.5rem 2.5rem minmax(0, 1fr)`, so an untouched
 * table still renders exactly as it did before this change. */
export const DEFAULT_COLUMN_WIDTHS: ColumnWidths = {
  ordinal: 3,
  timestamp: 12.5,
  level: 2.5,
};

/** Per-column floors, not one global floor (design D5): the ordinal and level
 * columns are narrow by nature, so a floor wide enough to keep a timestamp
 * readable would make them undraggable, and a floor narrow enough for them would
 * let the timestamp column vanish. `level` keeps room for the 1.6em level badge. */
export const MIN_COLUMN_WIDTHS: ColumnWidths = {
  ordinal: 2,
  timestamp: 3,
  level: 2,
};

/** A single ceiling: past this a column has stopped being a column and is just a
 * way to push the message off screen. Dragging clamps here the same way it clamps
 * at the floor, so the handle never appears stuck (design D5). */
export const MAX_COLUMN_WIDTH_REM = 60;

/** Human-readable column names for the notices a bad hash produces — `#` is what
 * the header actually says for `ordinal`. */
const COLUMN_LABELS: Record<ResizableColumn, string> = {
  ordinal: "#",
  timestamp: "timestamp",
  level: "level",
};

export function clampColumnWidth(column: ResizableColumn, rem: number): number {
  return Math.min(MAX_COLUMN_WIDTH_REM, Math.max(MIN_COLUMN_WIDTHS[column], rem));
}

export function columnWidthsEqual(a: ColumnWidths, b: ColumnWidths): boolean {
  return RESIZABLE_COLUMNS.every((column) => a[column] === b[column]);
}

/** Serialises to the `cols=` value, or `null` when every width is its default —
 * absent means defaults (`url-state` spec, "Absent widths mean defaults"), so a
 * table nobody has resized adds nothing to the link. Two decimals: a drag lands on
 * fractional `rem` values, and the full float would put 16 digits of noise in a URL
 * the user is expected to read. */
export function encodeColumnWidths(widths: ColumnWidths): string | null {
  if (columnWidthsEqual(widths, DEFAULT_COLUMN_WIDTHS)) {
    return null;
  }
  return RESIZABLE_COLUMNS.map((column) => String(Math.round(widths[column] * 100) / 100)).join(",");
}

export interface ParsedColumnWidths {
  widths: ColumnWidths;
  /** What could not be applied verbatim, phrased for the shared-URL notice. Empty
   * when the value was fully usable. A width is a presentation preference, so
   * these are always *reports*, never a reason to stop (`url-state` spec, "A bad
   * column width never blocks the log"). */
  errors: string[];
}

/** Parses a `cols=` value into usable widths plus everything that had to be
 * changed to make them usable (design D6). Never throws and never returns a width
 * outside `[MIN_COLUMN_WIDTHS, MAX_COLUMN_WIDTH_REM]`. */
export function parseColumnWidths(raw: string | null): ParsedColumnWidths {
  if (raw === null) {
    return { widths: DEFAULT_COLUMN_WIDTHS, errors: [] };
  }
  const parts = raw.split(",");
  if (parts.length !== RESIZABLE_COLUMNS.length) {
    // A wrong arity can't be assigned to columns positionally at all — there is no
    // way to tell which width was meant for which column — so this is the one case
    // that falls back wholesale rather than per column.
    return {
      widths: DEFAULT_COLUMN_WIDTHS,
      errors: [`column widths "${raw}" are not ${RESIZABLE_COLUMNS.length} comma-separated values`],
    };
  }

  const errors: string[] = [];
  const widths: Record<ResizableColumn, number> = { ...DEFAULT_COLUMN_WIDTHS };
  RESIZABLE_COLUMNS.forEach((column, i) => {
    const text = (parts[i] ?? "").trim();
    const value = Number(text);
    if (text.length === 0 || !Number.isFinite(value)) {
      errors.push(`column width "${text}" for the ${COLUMN_LABELS[column]} column is not a number`);
      return;
    }
    const clamped = clampColumnWidth(column, value);
    if (clamped !== value) {
      // Clamped, not rejected (`url-state` spec, "Out-of-range width is clamped,
      // not rejected") — a too-narrow column that came back as its default would
      // be a bigger surprise than one that came back at its floor.
      errors.push(
        `column width ${value}rem for the ${COLUMN_LABELS[column]} column is out of range, ` +
          `applied ${clamped}rem`,
      );
    }
    widths[column] = clamped;
  });

  return { widths, errors };
}
