// URL hash grammar (`url-state` spec) and a debounced writer (design.md D7).
//
// Grammar: the fragment (after `#`) is an `application/x-www-form-urlencoded`
// query string, parsed/built with `URLSearchParams` rather than hand-rolled
// splitting — percent-encoding, and round-tripping a value that itself contains
// `,`/`=`/`&`, come free from a structure the platform already gets right instead
// of a bespoke escaper this project would have to prove correct. Keys:
//
//   range=<startMs>,<endMs>      -- epoch milliseconds, half-open [start, end)
//   filter=<field>=<value>       -- repeated, once per active chip value; two
//                                    values of the same field are two `filter=`
//                                    entries (D1's OR, not a comma-joined list,
//                                    so a value containing a comma can't be
//                                    misread as two values)
//   q=<text>                     -- raw search box text (may start with `re:`)
//   format=<json|logfmt|plain>   -- format override, `log-parsing-core` spec
//   tz=<minutes>                 -- fixed UTC offset override, minutes east
//
// All keys are optional; an absent key means "no constraint on that axis", not
// "empty". Unknown keys and unparsable values are reported by `decodeHash` rather
// than silently dropped (`url-state` spec, "A malformed hash is reported").

import type { TimeRange } from "./time-range";

export interface HashState {
  range: TimeRange | null;
  /** Field name -> selected values (D1: OR within, so a set). */
  fieldFilters: ReadonlyMap<string, ReadonlySet<string>>;
  query: string;
  formatOverride: string | null;
  tzOffsetMinutes: number | null;
}

export const EMPTY_HASH_STATE: HashState = {
  range: null,
  fieldFilters: new Map(),
  query: "",
  formatOverride: null,
  tzOffsetMinutes: null,
};

const KNOWN_KEYS = new Set(["range", "filter", "q", "format", "tz"]);

export function encodeHash(state: HashState): string {
  const params = new URLSearchParams();
  if (state.range) {
    params.set("range", `${state.range.startMs},${state.range.endMs}`);
  }
  for (const [field, values] of state.fieldFilters) {
    for (const value of values) {
      params.append("filter", `${field}=${value}`);
    }
  }
  if (state.query.length > 0) {
    params.set("q", state.query);
  }
  if (state.formatOverride) {
    params.set("format", state.formatOverride);
  }
  if (state.tzOffsetMinutes !== null) {
    params.set("tz", String(state.tzOffsetMinutes));
  }
  return params.toString();
}

export interface DecodedHash {
  range: TimeRange | null;
  fieldFilters: Map<string, Set<string>>;
  query: string;
  formatOverride: string | null;
  tzOffsetMinutes: number | null;
  /** Hash keys this grammar doesn't recognise at all (`url-state` spec, "Unknown
   * key") — reported, never silently ignored. */
  unknownKeys: string[];
  /** Set when `range=` was present but unparsable (`url-state` spec, "Invalid
   * range value") — the rest of the hash still applies. */
  rangeError: string | null;
  /** Set when one or more `filter=` entries had no `=`, so no field name could be
   * read from them at all — distinct from a field name the *dataset* doesn't
   * recognise, which is a semantic check the caller makes once the field
   * inventory exists (this module knows nothing about a dataset). */
  malformedFilterCount: number;
}

export function decodeHash(hash: string): DecodedHash {
  const raw = hash.startsWith("#") ? hash.slice(1) : hash;
  const params = new URLSearchParams(raw);

  const unknownKeys = [...new Set([...params.keys()].filter((k) => !KNOWN_KEYS.has(k)))];

  let range: TimeRange | null = null;
  let rangeError: string | null = null;
  const rangeRaw = params.get("range");
  if (rangeRaw !== null) {
    const parts = rangeRaw.split(",");
    const startMs = Number(parts[0]);
    const endMs = Number(parts[1]);
    if (parts.length === 2 && Number.isFinite(startMs) && Number.isFinite(endMs)) {
      range = { startMs, endMs };
    } else {
      rangeError = `range "${rangeRaw}" is not two comma-separated timestamps`;
    }
  }

  const fieldFilters = new Map<string, Set<string>>();
  let malformedFilterCount = 0;
  for (const entry of params.getAll("filter")) {
    const eq = entry.indexOf("=");
    if (eq <= 0) {
      malformedFilterCount += 1;
      continue;
    }
    const field = entry.slice(0, eq);
    const value = entry.slice(eq + 1);
    let values = fieldFilters.get(field);
    if (!values) {
      values = new Set();
      fieldFilters.set(field, values);
    }
    values.add(value);
  }

  const query = params.get("q") ?? "";
  const formatOverride = params.get("format");
  const tzRaw = params.get("tz");
  const tzOffsetMinutes = tzRaw !== null && Number.isFinite(Number(tzRaw)) ? Number(tzRaw) : null;

  return { range, fieldFilters, query, formatOverride, tzOffsetMinutes, unknownKeys, rangeError, malformedFilterCount };
}

/**
 * Debounced `replaceState` writer (D7): filters/search change on every keystroke
 * or click, and pushing a history entry per change would make the back button
 * useless within seconds. `flush()` writes immediately — used by the copy-link
 * affordance so a copied URL reflects the state on screen right now rather than
 * whatever was last written before the debounce fired (design.md risk: "the copy
 * affordance must read current state rather than the last written hash").
 */
export class HashWriter {
  private timer: ReturnType<typeof setTimeout> | null = null;
  private current: HashState = EMPTY_HASH_STATE;

  constructor(private readonly debounceMs: number = 300) {}

  schedule(state: HashState): void {
    this.current = state;
    if (this.timer !== null) {
      clearTimeout(this.timer);
    }
    this.timer = setTimeout(() => this.flush(), this.debounceMs);
  }

  /** Writes `current` now, bypassing the debounce. Idempotent. */
  flush(): void {
    if (this.timer !== null) {
      clearTimeout(this.timer);
      this.timer = null;
    }
    const encoded = encodeHash(this.current);
    const url = encoded.length > 0 ? `#${encoded}` : `${location.pathname}${location.search}`;
    history.replaceState(null, "", url);
  }

  dispose(): void {
    if (this.timer !== null) {
      clearTimeout(this.timer);
      this.timer = null;
    }
  }
}
