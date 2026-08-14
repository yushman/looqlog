// Presentational: surfaces parser diagnostics on screen (app-shell spec, "Parser
// diagnostics reach the user" / design.md D5 — this project's silent-failure list
// stops being a parser concern and becomes a product one here). Never console-only.

import type { DiagnosticsSummaryDto } from "../wasm-types";

export class LooqDiagnostics extends HTMLElement {
  private summary: DiagnosticsSummaryDto | null = null;
  private entriesEmitted = 0;
  private cumulative = false;

  /** `cumulative`: true for a live stream (design.md D7) — the one long-lived
   * parser instance never resets, so these counts describe everything ever seen,
   * not what is currently retained after client-side eviction (`live-tail-ui`
   * spec, "Field inventory reported as cumulative"). File mode leaves this false. */
  setDiagnostics(
    summary: DiagnosticsSummaryDto | null,
    entriesEmitted: number,
    cumulative = false,
  ): void {
    this.summary = summary;
    this.entriesEmitted = entriesEmitted;
    this.cumulative = cumulative;
    this.render();
  }

  connectedCallback(): void {
    this.render();
  }

  private render(): void {
    const summary = this.summary;
    const cumulativeNote = this.cumulative
      ? `<p class="cumulative-note">Counts are cumulative for the life of this stream —
         they describe every line seen so far, not only what is currently retained
         after older entries were evicted.</p>`
      : "";
    if (summary === null || summary.total === 0) {
      this.innerHTML = `<p class="diagnostics ok">No skipped lines.</p>${cumulativeNote}`;
      this.classList.remove("warning");
      return;
    }

    const skipRatio = summary.total / Math.max(1, summary.total + this.entriesEmitted);
    const isSevere = skipRatio > 0.2;
    this.classList.toggle("warning", isSevere);

    const countRows = Object.entries(summary.counts)
      .map(([reason, count]) => `<li><code>${escapeHtml(reason)}</code>: ${count}</li>`)
      .join("");

    const exampleRows = summary.examples
      .slice(0, 20)
      .map(
        (example) =>
          `<li>line ${example.line}: ${escapeHtml(example.reasonLabel)} — ${escapeHtml(example.detail)}</li>`,
      )
      .join("");

    const cappedNote =
      summary.total > summary.examples.length
        ? `<p>Showing ${summary.examples.length} of ${summary.total} examples (capped at ${summary.cap}); exact counts above are not capped.</p>`
        : "";

    this.innerHTML = `
      <p class="diagnostics${isSevere ? " warning" : ""}">
        <strong>${summary.total}</strong> line(s) skipped or flagged, next to
        <strong>${this.entriesEmitted}</strong> entries produced.
      </p>
      <ul class="diagnostics-counts">${countRows}</ul>
      <details>
        <summary>Examples</summary>
        <ul class="diagnostics-examples">${exampleRows}</ul>
        ${cappedNote}
      </details>
      ${cumulativeNote}
    `;
  }
}

function escapeHtml(input: string): string {
  return input
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;");
}

customElements.define("looq-diagnostics", LooqDiagnostics);
