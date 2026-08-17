// The right pane (`entry-table` spec, "Row selection and detail view", design.md
// D6). Persistent: the pane exists whether or not a row is selected, so inspecting
// an entry never reflows the table. Selection is held as an `ordinal` — a stable
// entry identity, not a screen position — so live growth cannot silently swap what
// is being described, and an evicted selection is reported as evicted rather than
// replaced by whatever now sits at that index.

import type { EntryIndex } from "../entry-index";
import { findMatchRanges, type CompiledQuery } from "../predicate";
import type { EntryDto, FieldValueDto } from "../wasm-types";

export class LooqEntryDetail extends HTMLElement {
  private index: EntryIndex | null = null;
  private ordinal: number | null = null;
  private compiledQuery: CompiledQuery = { kind: "none" };
  /** What the pane is currently describing, so a live tick that changes nothing
   * about the selection doesn't rewrite the pane eighty times a second. */
  private renderedKey: string | null = null;

  connectedCallback(): void {
    this.render();
  }

  setIndex(index: EntryIndex | null): void {
    this.index = index;
    this.ordinal = null;
    this.render();
  }

  setSelectedOrdinal(ordinal: number | null): void {
    this.ordinal = ordinal;
    this.render();
  }

  /** Highlighting only, mirroring the table's own `setQuery` (`search` spec). */
  setQuery(compiled: CompiledQuery): void {
    this.compiledQuery = compiled;
    this.renderedKey = null; // highlighting changed even if the selection didn't
    this.render();
  }

  /** Called after live append/eviction: the selected ordinal may have just been
   * evicted, which changes what this pane says without changing the selection. */
  refresh(): void {
    this.render();
  }

  private render(): void {
    const entry = this.ordinal === null ? undefined : this.index?.getByOrdinal(this.ordinal);
    const key = `${this.ordinal ?? "none"}|${entry ? "present" : "absent"}`;
    if (key === this.renderedKey) {
      return;
    }
    this.renderedKey = key;

    if (this.ordinal === null) {
      this.innerHTML = `<p class="detail-empty">Select an entry to inspect</p>`;
      return;
    }
    if (!entry) {
      this.innerHTML = `
        <p class="detail-evicted">Entry #${this.ordinal} is no longer retained (evicted at
        <code>--max-lines</code>) — its contents are gone, not hidden.</p>`;
      return;
    }
    this.innerHTML = renderEntryHtml(entry, this.compiledQuery);
  }
}

function renderEntryHtml(entry: EntryDto, compiled: CompiledQuery): string {
  const fieldRows = Object.entries(entry.fields)
    .map(([key, value]) => `<tr><th>${escapeHtml(key)}</th><td>${renderFieldValue(value)}</td></tr>`)
    .join("");
  return `
    <dl class="detail-core">
      <dt>ordinal</dt><dd>${entry.ordinal}</dd>
      <dt>timestamp</dt><dd>${entry.timestamp ? escapeHtml(entry.timestamp) + " (UTC)" : "no timestamp extracted"}</dd>
      <dt>level</dt><dd>${entry.level ? escapeHtml(entry.level) : "no level extracted"}</dd>
    </dl>
    <p class="detail-message-label">message</p>
    <pre class="detail-message">${highlightHtml(entry.message, compiled)}</pre>
    ${fieldRows ? `<p class="detail-fields-label">fields</p><table class="detail-fields">${fieldRows}</table>` : ""}
  `;
}

function renderFieldValue(value: FieldValueDto): string {
  switch (value.kind) {
    case "string":
      return escapeHtml(value.value);
    case "number":
      return escapeHtml(value.value);
    case "bool":
      return String(value.value);
    case "null":
      return `<span class="absent">null</span>`;
    case "json":
      // Nested JSON is kept as raw text (Entry::FieldValue::Json,
      // crates/looq-core/src/entry.rs) rather than flattened — shown verbatim in a
      // <pre> so it stays readable (`entry-table` spec, "Nested JSON is readable").
      return `<pre class="detail-json">${escapeHtml(value.value)}</pre>`;
  }
}

function escapeHtml(input: string): string {
  return input
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;");
}

/** Same `<span class="hl">`-not-`<mark>` reasoning as `looq-entry-table.ts`'s copy
 * of this helper (a measured cost, see that file's comment). */
function highlightHtml(text: string, compiled: CompiledQuery): string {
  const ranges = findMatchRanges(text, compiled);
  if (ranges.length === 0) {
    return escapeHtml(text);
  }
  const parts: string[] = [];
  let cursor = 0;
  for (const [start, end] of ranges) {
    parts.push(escapeHtml(text.slice(cursor, start)));
    parts.push(`<span class="hl">${escapeHtml(text.slice(start, end))}</span>`);
    cursor = end;
  }
  parts.push(escapeHtml(text.slice(cursor)));
  return parts.join("");
}

customElements.define("looq-entry-detail", LooqEntryDetail);
