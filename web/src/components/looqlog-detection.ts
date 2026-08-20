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

import type { DetectionResultDto, TimestampShapeKey } from "../wasm-types";
import { dispatchRailAttention } from "./looqlog-workspace";

/** Human-readable names for the timestamp shapes the plain-text prefix scanner
 * recognises (crates/looqlog-core/src/timestamp.rs `TimestampShape::as_str`). */
const SHAPE_LABELS: Record<TimestampShapeKey, string> = {
  iso8601: "ISO 8601",
  "slash-date": "slash-date",
  klog: "klog",
  syslog3164: "syslog RFC 3164",
  clf: "Apache/CLF",
  epoch: "epoch",
};

/** Below this fraction a *threshold* win is still flagged as low-confidence — the
 * detector's own 0.8 threshold (crates/looqlog-core/src/detect.rs) is a "did it win"
 * cutoff, not a "how comfortable should the user be" one; this is a separate,
 * UI-only judgement call for extra visual caution near the boundary. */
const LOW_CONFIDENCE_BELOW = 0.9;

export class LooqlogDetection extends HTMLElement {
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
    this.render(null, null);
  }

  /** `activeFormat`: the format the parser is actually using, which is the only
   * thing there is to report when `detection` is `null` because a `#format=`
   * override skipped detection entirely. */
  setDetection(detection: DetectionResultDto | null, activeFormat: string | null = null): void {
    this.render(detection, activeFormat);
  }

  private render(detection: DetectionResultDto | null, activeFormat: string | null): void {
    const key =
      detection === null
        ? `null|${activeFormat ?? ""}`
        : `${detection.format}|${detection.matchFraction}|${detection.outcome}|${detection.timestampShape ?? ""}`;
    if (key === this.lastKey) {
      return;
    }
    this.lastKey = key;

    if (detection === null) {
      this.classList.remove("fallback", "low-confidence");
      dispatchRailAttention(this, { needsAttention: false, autoExpand: false });
      // An explicit override means nothing was ever sampled, so there is no match
      // fraction and never will be — saying "detecting…" forever describes work
      // that is not happening. Without an override, detection genuinely is still
      // pending.
      if (activeFormat !== null) {
        this.stateEl.textContent = `${activeFormat} (set explicitly)`;
        this.stateEl.className = "rail-section-state";
        this.bodyEl.innerHTML = `<p class="detection">
             Format was set explicitly to <strong>${escapeHtml(activeFormat)}</strong>, so auto-detection
             did not run and there is no match fraction to report. Remove the
             <code>#format=</code> override to have the format detected from the file.
           </p>`;
        return;
      }
      this.stateEl.textContent = "detecting…";
      this.stateEl.className = "rail-section-state";
      this.bodyEl.innerHTML = `<p class="detection">Detecting format...</p>`;
      return;
    }

    const pct = (detection.matchFraction * 100).toFixed(0);
    // Plain text is no longer automatically a fallback: when the prefix scanner
    // recognised a timestamp shape in enough of the sample, the core reports a
    // threshold match like any other format, and the "fell back to plain text"
    // warning must not fire on input that parsed perfectly well (design D9 of the
    // prefix-and-payload-parsing change). Saying *which* shape matched is what keeps
    // that from reading as a bare "plain (95%)" the user cannot act on.
    const shape = detection.timestampShape;
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

    // Same rule as the diagnostics surface (collapsible-workspace-panes design
    // D3): a fallback or low-confidence detection is stated on the rail's own
    // toggle, so collapsing the rail cannot bury it, and the condition that opens
    // this section by itself expands the pane around it too.
    const autoExpand = isLowConfidence && !this.autoOpened;
    if (autoExpand) {
      this.detailsEl.open = true;
      this.autoOpened = true;
    }
    dispatchRailAttention(this, { needsAttention: isLowConfidence, autoExpand });

    this.bodyEl.innerHTML = isFallback
      ? `<p class="detection warning">
           <strong>Format fell back to plain text</strong> — no format matched at least
           80% of the sampled lines (best match: ${pct}%). Sampled lines may not be
           representative, or this really is unstructured text.
         </p>`
      : `<p class="detection${isLowConfidence ? " warning" : ""}">
           Detected format: <strong>${escapeHtml(detection.format)}</strong>
           (${pct}% of sampled lines matched)${
             shape === null
               ? ""
               : ` — matched by a recognised <strong>${escapeHtml(SHAPE_LABELS[shape] ?? shape)}</strong> timestamp prefix`
           }
         </p>`;
  }
}

function escapeHtml(input: string): string {
  return input
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;");
}

customElements.define("looqlog-detection", LooqlogDetection);
