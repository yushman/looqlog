// Compiles the search box's raw text into a `CompiledQuery` (`search` spec:
// substring by default, `re:` for regex, invalid regex fails loudly) and pulls
// `field=value` tokens for known fields out into chips (D4, `search` spec
// "`field=value` in the search box becomes a filter"). Pure and DOM-free so the
// combination rule's edge cases (task 1.4-adjacent: both search behaviours) are
// unit-testable without a browser.

import type { CompiledQuery } from "./predicate";

const REGEX_PREFIX = "re:";
/** Documented escape (`search` spec, "Literal text that looks like a prefix"): a
 * leading backslash immediately before `re:` produces a literal substring search
 * for text starting with `re:`, rather than being read as the regex prefix. */
const LITERAL_ESCAPE_PREFIX = "\\re:";

export interface QueryCompileResult {
  /** `null` only when `error` is set — D5: an invalid regex must not silently
   * become "a query that matches nothing", so there is no compiled query to fall
   * back to here. The caller is responsible for keeping whatever compiled query
   * was active before this call (`filtering`/`search` spec: previous results are
   * preserved, not this module's job — it has no notion of "previous"). */
  compiled: CompiledQuery | null;
  error: string | null;
}

export function compileQuery(raw: string): QueryCompileResult {
  if (raw.length === 0) {
    return { compiled: { kind: "none" }, error: null };
  }
  if (raw.startsWith(LITERAL_ESCAPE_PREFIX)) {
    const literal = raw.slice(1); // drop one leading backslash, keep the literal "re:..."
    return { compiled: { kind: "substring", needle: literal.toLowerCase() }, error: null };
  }
  if (raw.startsWith(REGEX_PREFIX)) {
    const pattern = raw.slice(REGEX_PREFIX.length);
    try {
      // No implicit flags (`search` spec: "applied as written, with no implicit
      // case-insensitivity") — exactly what the user typed, compiled as-is.
      const re = new RegExp(pattern);
      return { compiled: { kind: "regex", re, source: pattern }, error: null };
    } catch (err) {
      return { compiled: null, error: `invalid regex: ${err instanceof Error ? err.message : String(err)}` };
    }
  }
  return { compiled: { kind: "substring", needle: raw.toLowerCase() }, error: null };
}

export interface ExtractedToken {
  field: string;
  value: string;
}

export interface ExtractResult {
  tokens: ExtractedToken[];
  remainingText: string;
}

const FIELD_TOKEN_RE = /^([A-Za-z_][\w.-]*)=(.+)$/;

/** Splits `text` on whitespace and converts any whitespace-terminated token of the
 * form `field=value` into a chip when `field` is known — the token currently being
 * typed (the tail, when `text` doesn't end in whitespace) is left alone so a chip
 * doesn't materialise mid-keystroke, before the user has finished typing the
 * value. An unknown field name is left as ordinary text (`search` spec, "Unknown
 * field stays text") — silently producing an empty result is the failure this
 * project keeps designing out. */
export function extractFieldTokens(text: string, knownFields: ReadonlySet<string>): ExtractResult {
  const trailingSpace = /\s$/.test(text);
  const words = text.split(/\s+/).filter((w) => w.length > 0);
  const complete = trailingSpace ? words : words.slice(0, -1);
  const inProgress = trailingSpace ? [] : words.slice(-1);

  const tokens: ExtractedToken[] = [];
  const remaining: string[] = [];
  for (const word of complete) {
    const m = FIELD_TOKEN_RE.exec(word);
    if (m && knownFields.has(m[1]!)) {
      tokens.push({ field: m[1]!, value: m[2]! });
    } else {
      remaining.push(word);
    }
  }
  return { tokens, remainingText: [...remaining, ...inProgress].join(" ") };
}
