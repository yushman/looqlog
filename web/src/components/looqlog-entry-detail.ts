// The right pane (`entry-table` spec, "Row selection and detail view", design.md
// D6). Persistent: the pane exists whether or not a row is selected, so inspecting
// an entry never reflows the table. Selection is held as an `ordinal` — a stable
// entry identity, not a screen position — so live growth cannot silently swap what
// is being described, and an evicted selection is reported as evicted rather than
// replaced by whatever now sits at that index.

import type { EntryReplyDto, FieldValueDto, RowDto } from "../query-types";
import type { ViewSession } from "../view-session";
import { highlightHtml } from "./looqlog-entry-table";

export class LooqlogEntryDetail extends HTMLElement {
  private session: ViewSession | null = null;
  private ordinal: number | null = null;
  /** What the pane is currently describing, so a refresh that changes nothing about
   * the selection doesn't rewrite the pane. */
  private renderedKey: string | null = null;
  private fetchSeq = 0;

  connectedCallback(): void {
    this.show(null);
  }

  setSource(session: ViewSession | null): void {
    this.session = session;
    this.ordinal = null;
    this.show(null);
  }

  setSelectedOrdinal(ordinal: number | null): void {
    this.ordinal = ordinal;
    this.refresh();
  }

  /** Re-fetches the selection: after live eviction the entry may be gone, and after a
   * query change its highlighting differs. */
  refresh(): void {
    const ordinal = this.ordinal;
    if (ordinal === null || !this.session) {
      this.show(null);
      return;
    }
    const seq = ++this.fetchSeq;
    this.session
      .entry(ordinal)
      .then((reply) => {
        if (seq === this.fetchSeq) {
          this.show(reply);
        }
      })
      .catch(() => undefined);
  }

  private show(reply: EntryReplyDto | null): void {
    const row = reply?.row ?? null;
    const key = `${this.ordinal ?? "none"}|${row ? `${row.matchRanges.join(";")}|${reply?.raw ?? ""}` : "absent"}`;
    if (key === this.renderedKey) {
      return;
    }
    this.renderedKey = key;

    if (this.ordinal === null) {
      this.innerHTML = `<p class="detail-empty">Select an entry to inspect</p>`;
      return;
    }
    if (reply === null) {
      return; // still loading; keep what is shown
    }
    if (!row) {
      this.innerHTML = `
        <p class="detail-evicted">Entry #${this.ordinal} is no longer retained (evicted at
        <code>--max-lines</code>) — its contents are gone, not hidden.</p>`;
      return;
    }
    this.innerHTML = renderEntryHtml(row, reply.raw);
  }
}

function renderEntryHtml(entry: RowDto, raw: string | null): string {
  const fieldRows = Object.entries(entry.fields)
    .map(([key, value]) => `<tr><th>${escapeHtml(key)}</th><td>${renderFieldValue(value)}</td></tr>`)
    .join("");
  return `
    <dl class="detail-core">
      <dt>ordinal</dt><dd>${entry.ordinal}</dd>
      <dt>timestamp</dt><dd>${renderTimestamp(entry)}</dd>
      <dt>level</dt><dd>${entry.level ? escapeHtml(entry.level) : "no level extracted"}</dd>
    </dl>
    <p class="detail-message-label">message</p>
    <pre class="detail-message">${highlightHtml(entry.message, entry.matchRanges)}</pre>
    ${fieldRows ? `<p class="detail-fields-label">fields</p><table class="detail-fields">${fieldRows}</table>` : ""}
    ${raw !== null ? `<p class="detail-fields-label">raw line</p><pre class="detail-message">${escapeHtml(raw)}</pre>` : ""}
  `;
}

/** The timestamp plus every assumption that went into it. Both flags are here for the
 * same reason: an entry dated by inference sits at a point on the timeline the log
 * never stated, and a user who cannot see that has no way to distinguish it from a
 * fact. `timestampYearInferred` fires on the year-less shapes (syslog RFC 3164, klog),
 * where the year comes from the reference instant the backend supplies. */
function renderTimestamp(entry: RowDto): string {
  if (!entry.timestamp) {
    return "no timestamp extracted";
  }
  const notes: string[] = ["UTC"];
  if (entry.timestampUsedDefaultTz) {
    notes.push("zone assumed — the value carried no offset");
  }
  if (entry.timestampYearInferred) {
    notes.push("year inferred — this timestamp shape carries none");
  }
  const assumed = entry.timestampUsedDefaultTz || entry.timestampYearInferred;
  return `${escapeHtml(entry.timestamp)} <span class="${assumed ? "detail-assumed" : ""}">(${escapeHtml(notes.join("; "))})</span>`;
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
      // crates/looqlog-core/src/entry.rs) rather than flattened — shown verbatim in a
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

customElements.define("looqlog-entry-detail", LooqlogEntryDetail);
