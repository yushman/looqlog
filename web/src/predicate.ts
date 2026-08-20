// The one predicate this change hangs on (design.md D1/D2, `filtering` spec
// "Combination rule"). Field filters and search each have their own module
// (`search-query.ts` compiles a query, this module evaluates it), but the rule for
// combining them lives in exactly one place so the table, the timeline and every
// count agree by construction rather than by three implementations staying in sync.
//
// The active time range is deliberately NOT part of `matchesFieldFilters`/
// `matchesQuery` — `EntryIndex` already answers range queries in O(log n + k) via
// its sorted structure (`entry-index.ts`), and re-testing a range per entry here
// would throw that away. `entryMatchesRange` exists only for the few callers (live
// entries arriving one at a time, task 6.1) that need the full three-way test —
// range ∧ fields ∧ query — on a single entry outside the index's binary search.

import type { EntryDto, FieldInventoryDto, FieldValueDto } from "./wasm-types";
import type { TimeRange } from "./time-range";

export type FieldFilters = ReadonlyMap<string, ReadonlySet<string>>;

/** `level` is a first-class `EntryDto` property, not a member of `fields` (the
 * parser only inventories arbitrary fields — `crates/looq-core/src/parser.rs`
 * calls `inventory.record` for everything except timestamp/level/message). Chips
 * treat it as a field anyway (`filtering` spec: "chips for `level` and for the
 * fields the parser reported"), so every place that resolves "the value of field X
 * for this entry" has to special-case it once, here. */
export function fieldDisplayValue(entry: EntryDto, field: string): string | null {
  if (field === "level") {
    return entry.level;
  }
  const value = entry.fields[field];
  return value === undefined ? null : displayFieldValue(value);
}

/** Mirrors `FieldValue::display()` (`crates/looq-core/src/entry.rs`) — the same
 * canonical string a value was counted under in the field inventory, so a chip's
 * value always matches what an entry actually carries. */
export function displayFieldValue(value: FieldValueDto): string {
  switch (value.kind) {
    case "string":
      return value.value;
    case "number":
      return value.value;
    case "bool":
      return String(value.value);
    case "null":
      return "null";
    case "json":
      return value.value;
  }
}

/** D1: OR within a field (any selected value of that field matches), AND across
 * fields (every field with an active selection must match). An empty value set for
 * a field is the same as no filter on that field — chip removal clears the set
 * rather than leaving a filter that can never match. */
export function matchesFieldFilters(entry: EntryDto, filters: FieldFilters): boolean {
  for (const [field, values] of filters) {
    if (values.size === 0) {
      continue;
    }
    const actual = fieldDisplayValue(entry, field);
    if (actual === null || !values.has(actual)) {
      return false;
    }
  }
  return true;
}

export type CompiledQuery =
  | { kind: "none" }
  | { kind: "substring"; needle: string }
  | { kind: "regex"; re: RegExp; source: string };

/** D3: message text plus every field value (including `level`), never field
 * names — searching "service" must not match every entry that merely has a
 * `service` field. */
export function buildHaystack(entry: EntryDto): string[] {
  const haystack = [entry.message];
  if (entry.level !== null) {
    haystack.push(entry.level);
  }
  for (const key in entry.fields) {
    haystack.push(displayFieldValue(entry.fields[key]!));
  }
  return haystack;
}

/** Deliberately does NOT call `buildHaystack` (task 7.3/7.4 — the 10k filter
 * latency budget): allocating one array per entry just to `.some()` over it added
 * measured milliseconds at 10k entries with a regex query (`docs/devlog.md`),
 * because every non-matching entry still walks every field. Testing message,
 * level and each field value directly, short-circuiting on the first hit, does
 * the same work with none of the per-entry array allocation. */
export function matchesQuery(entry: EntryDto, compiled: CompiledQuery): boolean {
  if (compiled.kind === "none") {
    return true;
  }
  if (matchesText(entry.message, compiled)) {
    return true;
  }
  if (entry.level !== null && matchesText(entry.level, compiled)) {
    return true;
  }
  for (const key in entry.fields) {
    if (matchesText(displayFieldValue(entry.fields[key]!), compiled)) {
      return true;
    }
  }
  return false;
}

function matchesText(text: string, compiled: Exclude<CompiledQuery, { kind: "none" }>): boolean {
  return compiled.kind === "substring" ? text.toLowerCase().includes(compiled.needle) : compiled.re.test(text);
}

/** The predicate `EntryIndex` evaluates, split in two because a continuation chain
 * is filtered and searched by different halves of it
 * (`multiline-entry-continuations` design D9):
 *
 * - **`root`** — the field filters. Evaluated against the chain root only, so
 *   `level=ERROR` shows a whole trace whose frames extracted no level of their own,
 *   and a chain whose root does not match is hidden entirely (`filtering` spec).
 * - **`member`** — the search query. A match on *any* member surfaces the chain, with
 *   the root intact, so a matching frame is never shown detached from the exception it
 *   belongs to (`search` spec).
 *
 * An entry that is not part of a chain — including a member orphaned by eviction — is
 * simply tested against both halves. */
export interface ChainAwarePredicate {
  root(entry: EntryDto): boolean;
  member(entry: EntryDto): boolean;
}

/** The active chips and query as the two halves above. */
export function chainAwarePredicate(
  fieldFilters: FieldFilters,
  query: CompiledQuery,
): ChainAwarePredicate {
  return {
    root: (entry) => matchesFieldFilters(entry, fieldFilters),
    member: (entry) => matchesQuery(entry, query),
  };
}

/** The full three-way test — range ∧ fields ∧ query — on one entry. `EntryIndex`
 * does not use this for its bulk range/bucket queries (it has faster range
 * machinery of its own); this is for the callers that only ever see one entry at a
 * time, chiefly live entries evaluated on arrival (D9, `filtering` spec "Filters
 * apply to live entries"). */
export function matchesPredicate(
  entry: EntryDto,
  range: TimeRange | null,
  fieldFilters: FieldFilters,
  query: CompiledQuery,
): boolean {
  if (range !== null) {
    if (entry.timestamp === null) {
      return false;
    }
    const ms = Date.parse(entry.timestamp);
    if (Number.isNaN(ms) || ms < range.startMs || ms >= range.endMs) {
      return false;
    }
  }
  return matchesFieldFilters(entry, fieldFilters) && matchesQuery(entry, query);
}

/** The known field names chips/`field=value` parsing may recognise: `level` always
 * (even with zero distinct values so far — a stream that hasn't emitted an ERROR
 * yet still knows `level` is a field), plus every field the inventory reports. */
export function knownFieldNames(inventory: FieldInventoryDto | null): Set<string> {
  const names = new Set<string>(["level"]);
  if (inventory) {
    for (const name of Object.keys(inventory.fields)) {
      names.add(name);
    }
  }
  return names;
}

/** Character ranges in `text` that `compiled` matches, for highlighting (`search`
 * spec, "Match highlighting"). Callers pass the already-truncated visible text so
 * ranges line up with what is actually rendered. */
export function findMatchRanges(text: string, compiled: CompiledQuery): Array<[number, number]> {
  if (compiled.kind === "none") {
    return [];
  }
  const ranges: Array<[number, number]> = [];
  if (compiled.kind === "substring") {
    if (compiled.needle.length === 0) {
      return ranges;
    }
    const lower = text.toLowerCase();
    let from = 0;
    for (;;) {
      const at = lower.indexOf(compiled.needle, from);
      if (at === -1) {
        break;
      }
      ranges.push([at, at + compiled.needle.length]);
      from = at + compiled.needle.length;
    }
    return ranges;
  }
  // Regex: force a `g` flag on a fresh RegExp so `exec` walks every match rather
  // than the single boolean `matchesQuery` needed — the source object's own flags
  // (whatever the user's `re:` pattern implied) are preserved otherwise.
  const flags = compiled.re.flags.includes("g") ? compiled.re.flags : `${compiled.re.flags}g`;
  const re = new RegExp(compiled.re.source, flags);
  let match: RegExpExecArray | null;
  // A zero-length match (e.g. `re:a*`) would spin `lastIndex` forever without this.
  while ((match = re.exec(text)) !== null) {
    if (match[0].length === 0) {
      re.lastIndex += 1;
      continue;
    }
    ranges.push([match.index, match.index + match[0].length]);
  }
  return ranges;
}
