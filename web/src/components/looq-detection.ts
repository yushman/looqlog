// Presentational: shows the detected format and match fraction (app-shell spec,
// "The detected format is visible and overridable in principle"). A fallback or
// low-confidence result gets a visually distinct treatment so a misclassification is
// noticed, not silently accepted.

import type { DetectionResultDto } from "../wasm-types";

/** Below this fraction a *threshold* win is still flagged as low-confidence — the
 * detector's own 0.8 threshold (crates/looq-core/src/detect.rs) is a "did it win"
 * cutoff, not a "how comfortable should the user be" one; this is a separate,
 * UI-only judgement call for extra visual caution near the boundary. */
const LOW_CONFIDENCE_BELOW = 0.9;

export class LooqDetection extends HTMLElement {
  setDetection(detection: DetectionResultDto | null): void {
    this.render(detection);
  }

  connectedCallback(): void {
    this.render(null);
  }

  private render(detection: DetectionResultDto | null): void {
    if (detection === null) {
      this.innerHTML = `<p class="detection">Detecting format...</p>`;
      this.classList.remove("fallback", "low-confidence");
      return;
    }

    const pct = (detection.matchFraction * 100).toFixed(0);
    const isFallback = detection.outcome === "fallback";
    const isLowConfidence = isFallback || detection.matchFraction < LOW_CONFIDENCE_BELOW;
    this.classList.toggle("fallback", isFallback);
    this.classList.toggle("low-confidence", isLowConfidence);

    if (isFallback) {
      this.innerHTML = `
        <p class="detection warning">
          <strong>Format fell back to plain text</strong> — no format matched at least
          80% of the sampled lines (best match: ${pct}%). Sampled lines may not be
          representative, or this really is unstructured text.
        </p>
      `;
      return;
    }

    this.innerHTML = `
      <p class="detection${isLowConfidence ? " warning" : ""}">
        Detected format: <strong>${escapeHtml(detection.format)}</strong>
        (${pct}% of sampled lines matched)
      </p>
    `;
  }
}

function escapeHtml(input: string): string {
  return input
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;");
}

customElements.define("looq-detection", LooqDetection);
