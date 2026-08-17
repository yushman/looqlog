// Presentational: shows the detected format and match fraction (app-shell spec,
// "The detected format is visible and overridable in principle"). A fallback or
// low-confidence result gets a visually distinct treatment so a misclassification is
// noticed, not silently accepted.
//
// A collapsible rail surface (`app-shell` spec, "Secondary surfaces are collapsible
// without hiding warnings", design.md D7): the state lives on the summary line that
// stays visible when collapsed, and a fallback or low-confidence detection opens the
// section by itself the first time it happens — collapsing saves space, it never
// hides the warning.

import type { DetectionResultDto } from "../wasm-types";

/** Below this fraction a *threshold* win is still flagged as low-confidence — the
 * detector's own 0.8 threshold (crates/looq-core/src/detect.rs) is a "did it win"
 * cutoff, not a "how comfortable should the user be" one; this is a separate,
 * UI-only judgement call for extra visual caution near the boundary. */
const LOW_CONFIDENCE_BELOW = 0.9;

export class LooqDetection extends HTMLElement {
  private detailsEl!: HTMLDetailsElement;
  private stateEl!: HTMLElement;
  private bodyEl!: HTMLElement;
  /** Live mode pushes a detection result on every render tick; re-rendering
   * identical content would reset the section's open state (and, for anything
   * interactive inside it, the same detach-under-the-pointer failure the filter
   * rail had). */
  private lastKey: string | null = null;
  private autoOpened = false;

  connectedCallback(): void {
    if (this.detailsEl) {
      return;
    }
    this.innerHTML = `
      <details class="rail-section" id="detection-section">
        <summary>
          <span class="rail-section-title">Format</span>
          <span class="rail-section-state" id="detection-state">detecting…</span>
        </summary>
        <div class="rail-section-body" id="detection-body"></div>
      </details>
    `;
    this.detailsEl = this.querySelector("#detection-section") as HTMLDetailsElement;
    this.stateEl = this.querySelector("#detection-state") as HTMLElement;
    this.bodyEl = this.querySelector("#detection-body") as HTMLElement;
    this.render(null);
  }

  setDetection(detection: DetectionResultDto | null): void {
    this.render(detection);
  }

  private render(detection: DetectionResultDto | null): void {
    const key = detection === null ? "null" : `${detection.format}|${detection.matchFraction}|${detection.outcome}`;
    if (key === this.lastKey) {
      return;
    }
    this.lastKey = key;

    if (detection === null) {
      this.stateEl.textContent = "detecting…";
      this.stateEl.className = "rail-section-state";
      this.bodyEl.innerHTML = `<p class="detection">Detecting format...</p>`;
      this.classList.remove("fallback", "low-confidence");
      return;
    }

    const pct = (detection.matchFraction * 100).toFixed(0);
    const isFallback = detection.outcome === "fallback";
    const isLowConfidence = isFallback || detection.matchFraction < LOW_CONFIDENCE_BELOW;
    this.classList.toggle("fallback", isFallback);
    this.classList.toggle("low-confidence", isLowConfidence);

    // The collapsed summary carries the outcome, not just the format name.
    this.stateEl.textContent = isFallback
      ? `fell back to plain text (${pct}%)`
      : isLowConfidence
        ? `${detection.format} — low confidence (${pct}%)`
        : `${detection.format} (${pct}%)`;
    this.stateEl.className = `rail-section-state${isLowConfidence ? " warning" : ""}`;

    if (isLowConfidence && !this.autoOpened) {
      this.detailsEl.open = true;
      this.autoOpened = true;
    }

    this.bodyEl.innerHTML = isFallback
      ? `<p class="detection warning">
           <strong>Format fell back to plain text</strong> — no format matched at least
           80% of the sampled lines (best match: ${pct}%). Sampled lines may not be
           representative, or this really is unstructured text.
         </p>`
      : `<p class="detection${isLowConfidence ? " warning" : ""}">
           Detected format: <strong>${escapeHtml(detection.format)}</strong>
           (${pct}% of sampled lines matched)
         </p>`;
  }
}

function escapeHtml(input: string): string {
  return input
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;");
}

customElements.define("looq-detection", LooqDetection);
