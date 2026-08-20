// Presentational: surfaces parser diagnostics on screen (app-shell spec, "Parser
// diagnostics reach the user" — this project's silent-failure list stops being a
// parser concern and becomes a product one here). Never console-only.
//
// A collapsible rail surface (design.md D7 of `frontend-three-pane-layout`), which
// is only allowed because the collapsed summary itself carries the skip count and
// its severity, and because a severe skip ratio opens the section on its own. A
// warning a user has to expand to discover would not satisfy "diagnostics reach the
// user"; a summary line that always states the count does.

import type { DiagnosticsSummaryDto } from "../wasm-types";
import { dispatchRailAttention } from "./looqlog-workspace";

/** Same threshold the expanded panel uses for its warning styling. */
const SEVERE_SKIP_RATIO = 0.2;

export class LooqlogDiagnostics extends HTMLElement {
  private summary: DiagnosticsSummaryDto | null = null;
  private entriesEmitted = 0;
  private cumulative = false;

  private detailsEl!: HTMLDetailsElement;
  private stateEl!: HTMLElement;
  private headlineEl!: HTMLElement;
  private bodyEl!: HTMLElement;
  private autoOpened = false;
  /** Live mode calls `setDiagnostics` on every render tick; the body is only
   * rewritten when its content actually changed, so the nested "Examples" section
   * keeps its open state (and nothing inside gets detached under a pointer). */
  private lastBodyKey: string | null = null;
  /** What the workspace was last told about this surface (design D3). */
  private lastAttention: boolean | null = null;

  connectedCallback(): void {
    if (this.detailsEl) {
      return;
    }
    this.innerHTML = `
      <details class="rail-section" id="diagnostics-section">
        <summary>
          <span class="rail-section-title">Diagnostics</span>
          <span class="rail-section-state" id="diagnostics-state">0 skipped</span>
        </summary>
        <div class="rail-section-body">
          <p class="diagnostics" id="diagnostics-headline"></p>
          <div id="diagnostics-body"></div>
        </div>
      </details>
    `;
    this.detailsEl = this.querySelector("#diagnostics-section") as HTMLDetailsElement;
    this.stateEl = this.querySelector("#diagnostics-state") as HTMLElement;
    this.headlineEl = this.querySelector("#diagnostics-headline") as HTMLElement;
    this.bodyEl = this.querySelector("#diagnostics-body") as HTMLElement;
    this.render();
  }

  /** `cumulative`: true for a live stream — the one long-lived parser instance
   * never resets, so these counts describe everything ever seen, not what is
   * currently retained after client-side eviction (`live-tail-ui` spec, "Field
   * inventory reported as cumulative"). File mode leaves this false. */
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

  private render(): void {
    if (!this.detailsEl) {
      return; // not connected yet; connectedCallback renders once it is
    }
    const summary = this.summary;
    const cumulativeNote = this.cumulative
      ? `<p class="cumulative-note">Counts are cumulative for the life of this stream —
         they describe every line seen so far, not only what is currently retained
         after older entries were evicted.</p>`
      : "";

    if (summary === null || summary.total === 0) {
      // The collapsed state still states the outcome: nothing was skipped.
      this.stateEl.textContent = "0 skipped";
      this.stateEl.className = "rail-section-state";
      this.headlineEl.textContent = "No skipped lines.";
      this.headlineEl.className = "diagnostics ok";
      this.classList.remove("warning");
      this.writeBody("clean", cumulativeNote);
      // Nothing to see, so the pane's own control must stop saying there is
      // (collapsible-workspace-panes design D3) — live mode can go from skipped
      // lines back to none as counts are recomputed.
      this.reportAttention(false);
      return;
    }

    const skipRatio = summary.total / Math.max(1, summary.total + this.entriesEmitted);
    const isSevere = skipRatio > SEVERE_SKIP_RATIO;
    this.classList.toggle("warning", isSevere);

    // The count and its severity are readable without expanding anything.
    this.stateEl.textContent = `${summary.total} skipped`;
    this.stateEl.className = `rail-section-state${isSevere ? " warning" : ""}`;
    this.headlineEl.className = `diagnostics${isSevere ? " warning" : ""}`;
    this.headlineEl.textContent =
      `${summary.total} line(s) skipped or flagged, next to ${this.entriesEmitted} entries produced.`;

    // Skipped lines are something the user must not miss, so the rail's toggle
    // says so too — otherwise collapsing the rail would hide the very summary
    // that makes this section allowed to be collapsible (design D3). A severe
    // ratio opens the section AND, through `autoExpand`, the pane containing it:
    // an auto-opened section inside a zero-width pane is visible to nobody.
    const autoExpand = isSevere && !this.autoOpened;
    if (autoExpand) {
      this.detailsEl.open = true;
      this.autoOpened = true;
    }
    this.reportAttention(true, autoExpand);

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

    this.writeBody(
      `${JSON.stringify(summary.counts)}|${summary.examples.length}|${summary.total}|${this.cumulative}`,
      `<ul class="diagnostics-counts">${countRows}</ul>
       <details class="diagnostics-examples-section">
         <summary>Examples</summary>
         <ul class="diagnostics-examples">${exampleRows}</ul>
         ${cappedNote}
       </details>
       ${cumulativeNote}`,
    );
  }

  /** Live mode re-renders on every tick; only a *change* is worth telling the
   * workspace about (an `autoExpand` always is — it is an action, not a state). */
  private reportAttention(needsAttention: boolean, autoExpand = false): void {
    if (needsAttention === this.lastAttention && !autoExpand) {
      return;
    }
    this.lastAttention = needsAttention;
    dispatchRailAttention(this, { needsAttention, autoExpand });
  }

  private writeBody(key: string, html: string): void {
    if (key === this.lastBodyKey) {
      return;
    }
    this.lastBodyKey = key;
    this.bodyEl.innerHTML = html;
  }
}

function escapeHtml(input: string): string {
  return input
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;");
}

customElements.define("looqlog-diagnostics", LooqlogDiagnostics);
