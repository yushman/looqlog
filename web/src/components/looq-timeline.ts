// Count-per-bucket histogram (`timeline` spec, design.md D3/D4/D5/D8). Owns its own
// `uPlot` instance; takes an `EntryIndex` reference and re-derives everything else
// (span, bucket width, counts) from it on `render()`. Emits `range-change` when the
// user drags a selection or clears it; never holds the active range itself past
// reflecting what the shell sets via `setActiveRange` (D7 — range is shell state).

import uPlot from "uplot";
import "uplot/dist/uPlot.min.css";

import type { EntryIndex, RobustSpan } from "../entry-index";
import { bucketCountForSpan, pickBucketWidthMs } from "../timeline-bucket";
import type { TimeRange } from "../time-range";

/** uPlot sizes itself from this, so the CSS `min-height` alone cannot shrink the
 * chart — the two are kept in step (task 9.4). */
const CHART_HEIGHT = 110;
const MIN_CHART_WIDTH = 480;

function pad2(n: number): string {
  return String(n).padStart(2, "0");
}

/** Ticks are formatted from the bucket width, not a fixed calendar unit, so a
 * five-second bucket shows seconds and a thirty-day bucket shows dates — always in
 * UTC, matching the table's stated timezone (log-parsing-core only supports UTC or
 * a fixed offset, both normalised to UTC by the time entries reach the browser). */
function formatTick(ms: number, bucketWidthMs: number): string {
  const d = new Date(ms);
  if (bucketWidthMs < 60_000) {
    return `${pad2(d.getUTCHours())}:${pad2(d.getUTCMinutes())}:${pad2(d.getUTCSeconds())}`;
  }
  if (bucketWidthMs < 24 * 60 * 60 * 1000) {
    return `${pad2(d.getUTCHours())}:${pad2(d.getUTCMinutes())}`;
  }
  return `${d.getUTCFullYear()}-${pad2(d.getUTCMonth() + 1)}-${pad2(d.getUTCDate())}`;
}

function formatSpanLabel(ms: number): string {
  return `${new Date(ms).toISOString().replace("T", " ").replace("Z", " UTC")}`;
}

/** `--accent` is a plain `#rrggbb` hex token; the filtered-series fill/stroke need
 * an alpha channel uPlot's canvas paths use directly, so convert rather than
 * hardcoding a second, alpha-bearing copy of the color in this file. */
function hexToRgba(hex: string, alpha: number): string {
  if (!/^#[0-9a-f]{6}$/i.test(hex)) {
    return hex; // malformed token: fall back to the raw value rather than throwing
  }
  const r = parseInt(hex.slice(1, 3), 16);
  const g = parseInt(hex.slice(3, 5), 16);
  const b = parseInt(hex.slice(5, 7), 16);
  return `rgba(${r}, ${g}, ${b}, ${alpha})`;
}

export class LooqTimeline extends HTMLElement {
  private index: EntryIndex | null = null;
  private plot: uPlot | null = null;
  private activeRange: TimeRange | null = null;
  private showingFullSpan = false;
  /** Width the current canvas was built at, and the pending resize frame — see
   * `handleResize`. */
  private lastChartWidth = 0;
  private resizeFrame: number | null = null;

  private chartEl!: HTMLDivElement;
  private summaryEl!: HTMLParagraphElement;
  private outlierEl!: HTMLParagraphElement;
  private clearBtn!: HTMLButtonElement;
  private zoomOutBtn!: HTMLButtonElement;

  connectedCallback(): void {
    this.innerHTML = `
      <div class="timeline-wrap">
        <!-- Summary, outlier note and range controls share one line: three stacked
             rows cost vertical space the entry table needs (task 9.4). -->
        <div class="timeline-head">
          <p class="timeline-summary" id="timeline-summary"></p>
          <p class="timeline-outlier-note provisional-note" id="timeline-outlier" hidden>
            <span id="outlier-text"></span>
            <button type="button" id="zoom-out-btn">Show full range</button>
          </p>
          <div class="timeline-controls">
            <button type="button" id="clear-range" hidden>Clear range</button>
          </div>
        </div>
        <div class="timeline-chart" id="chart"></div>
      </div>
    `;
    this.chartEl = this.querySelector("#chart") as HTMLDivElement;
    this.summaryEl = this.querySelector("#timeline-summary") as HTMLParagraphElement;
    this.outlierEl = this.querySelector("#timeline-outlier") as HTMLParagraphElement;
    this.clearBtn = this.querySelector("#clear-range") as HTMLButtonElement;
    this.zoomOutBtn = this.querySelector("#zoom-out-btn") as HTMLButtonElement;

    window.addEventListener("resize", this.handleResize);

    this.clearBtn.addEventListener("click", () => {
      this.dispatchEvent(new CustomEvent<TimeRange | null>("range-change", { detail: null }));
    });
    this.zoomOutBtn.addEventListener("click", () => {
      this.showingFullSpan = true;
      this.render();
    });
  }

  setIndex(index: EntryIndex): void {
    this.index = index;
    this.showingFullSpan = false;
    this.render();
  }

  /** Called after live growth/eviction. The caller is responsible for throttling
   * (design.md D8 — "on an interval, not per entry"); this method itself always
   * does a full recompute so it stays correct whenever it's called. */
  refresh(): void {
    this.render();
  }

  setActiveRange(range: TimeRange | null): void {
    this.activeRange = range;
    this.clearBtn.hidden = range === null;
    this.applySelectionOverlay();
  }

  private destroyPlot(): void {
    this.plot?.destroy();
    this.plot = null;
  }

  /** uPlot takes its width as a number at construction time, so a canvas drawn
   * into a 1256px pane keeps that width in an 800px one — the chart simply
   * overflows or leaves a gap. Nothing else re-renders the timeline in file mode
   * (no live tick), so the window is what has to say when the pane changed. A
   * redraw is a full bucket recompute, so it only happens when the available
   * width actually changed, and at most once per frame. */
  private readonly handleResize = (): void => {
    if (this.resizeFrame !== null) {
      return;
    }
    this.resizeFrame = requestAnimationFrame(() => {
      this.resizeFrame = null;
      if (this.availableChartWidth() !== this.lastChartWidth) {
        this.render();
      }
    });
  };

  private availableChartWidth(): number {
    return Math.max(this.chartEl.clientWidth || MIN_CHART_WIDTH, MIN_CHART_WIDTH);
  }

  disconnectedCallback(): void {
    window.removeEventListener("resize", this.handleResize);
    if (this.resizeFrame !== null) {
      cancelAnimationFrame(this.resizeFrame);
      this.resizeFrame = null;
    }
    this.destroyPlot();
  }

  private render(): void {
    if (!this.index) {
      return;
    }
    const timestamplessCount = this.index.timestamplessCount;
    const timestampedCount = this.index.timestampedCount;

    if (timestampedCount === 0) {
      this.destroyPlot();
      this.chartEl.replaceChildren();
      this.outlierEl.hidden = true;
      this.summaryEl.textContent =
        timestamplessCount > 0
          ? `No entries have a usable timestamp (${timestamplessCount} total) — ` +
            `nothing to place on a time axis.`
          : "No entries yet.";
      return;
    }

    const full = this.index.fullSpan()!;
    const robust: RobustSpan = this.showingFullSpan
      ? { minMs: full.minMs, maxMs: full.maxMs, outlierCount: 0 }
      : (this.index.robustSpan() ?? { minMs: full.minMs, maxMs: full.maxMs, outlierCount: 0 });

    let { minMs, maxMs } = robust;
    if (maxMs === minMs) {
      // Degenerate single-instant span: pad so a bucket has non-zero width.
      minMs -= 500;
      maxMs += 500;
    }
    const spanMs = maxMs - minMs;

    const bucketWidthMs = pickBucketWidthMs(spanMs);
    const bucketCount = bucketCountForSpan(spanMs, bucketWidthMs);
    // Align the bucket grid to a round multiple of its own width, so boundaries
    // land on clean marks (e.g. a 5s bucket starts on :00/:05) instead of at the
    // dataset's arbitrary first timestamp.
    const startMs = Math.floor(minMs / bucketWidthMs) * bucketWidthMs;
    // D6 (`timeline` spec, MODIFIED "Count-per-bucket histogram"): two series —
    // `total` ignores the active predicate entirely (background, "the dataset's
    // shape"), `filtered` respects it (foreground). Identical arrays when no
    // predicate is active, which is exactly the old single-series look.
    const totalCounts = this.index.bucketCountsUnfiltered(startMs, bucketWidthMs, bucketCount);
    const filteredCounts = this.index.bucketCounts(startMs, bucketWidthMs, bucketCount);

    const xs = new Array<number>(bucketCount);
    for (let i = 0; i < bucketCount; i++) {
      xs[i] = (startMs + i * bucketWidthMs) / 1000; // uPlot x values, seconds
    }
    const data: uPlot.AlignedData = [xs, totalCounts, filteredCounts];

    this.drawPlot(data, bucketWidthMs);

    const total = timestampedCount + timestamplessCount;
    this.summaryEl.textContent =
      `${formatSpanLabel(startMs)} → ${formatSpanLabel(startMs + bucketCount * bucketWidthMs)} ` +
      `(bucket ${describeBucket(bucketWidthMs)}, UTC)` +
      (timestamplessCount > 0 ? ` — ${timestamplessCount} of ${total} entries have no timestamp` : "");

    if (robust.outlierCount > 0) {
      this.outlierEl.hidden = false;
      (this.querySelector("#outlier-text") as HTMLSpanElement).textContent =
        `${robust.outlierCount} entr${robust.outlierCount === 1 ? "y" : "ies"} ` +
        `fall outside the shown range and would compress it if included.`;
    } else {
      this.outlierEl.hidden = true;
    }
  }

  private drawPlot(data: uPlot.AlignedData, bucketWidthMs: number): void {
    this.destroyPlot();
    const width = this.availableChartWidth();
    this.lastChartWidth = width;
    // Read the accent color from the token at render time rather than hardcoding
    // its hex, so a future palette change (`theming` spec) doesn't require an edit
    // here too (design.md D4).
    const accent = getComputedStyle(document.documentElement).getPropertyValue("--accent").trim() || "#6e8bff";
    const opts: uPlot.Options = {
      width,
      height: CHART_HEIGHT,
      scales: { x: { time: false } },
      cursor: {
        drag: { x: true, y: false, setScale: false },
      },
      legend: { show: false },
      series: [
        {},
        {
          // Background series (D6): the unfiltered/total counts, drawn first so
          // the foreground series below renders over it — where a filter excludes
          // entries, this is what stays visible above the shorter blue bar,
          // making "filtered to nothing" read as "a filter emptied this" rather
          // than "no data exists here".
          label: "all entries",
          paths: uPlot.paths.bars!({ size: [0.9, 100] }),
          // D4: dropped from 0.45 so the unfiltered background reads as context,
          // not a competing series, against the new darker #0b0c0f background.
          fill: "rgba(148, 163, 184, 0.25)",
          stroke: "rgba(100, 116, 139, 0.6)",
          width: 1,
        },
        {
          label: "matching filters",
          paths: uPlot.paths.bars!({ size: [0.9, 100] }),
          // D4: switched from a hardcoded blue to the `--accent` token so palette
          // changes propagate here automatically; 0.6/0.9 chosen by visual check
          // against #0b0c0f (docs/devlog.md) — comparable-or-higher prominence
          // than the old hardcoded blue while reading calmer/more monochrome.
          fill: hexToRgba(accent, 0.6),
          stroke: hexToRgba(accent, 0.9),
          width: 1,
        },
      ],
      axes: [
        { values: (_u, vals) => vals.map((v) => formatTick(v * 1000, bucketWidthMs)) },
        { size: 40 },
      ],
      hooks: {
        setSelect: [
          (u) => {
            if (u.select.width <= 0) {
              return;
            }
            const startSec = u.posToVal(u.select.left, "x");
            const endSec = u.posToVal(u.select.left + u.select.width, "x");
            const range: TimeRange = {
              startMs: Math.round(startSec * 1000),
              endMs: Math.round(endSec * 1000),
            };
            this.dispatchEvent(new CustomEvent<TimeRange>("range-change", { detail: range }));
          },
        ],
      },
    };
    this.plot = new uPlot(opts, data, this.chartEl);
    this.applySelectionOverlay();
  }

  private applySelectionOverlay(): void {
    if (!this.plot) {
      return;
    }
    if (!this.activeRange) {
      this.plot.setSelect({ left: 0, top: 0, width: 0, height: 0 }, false);
      return;
    }
    const left = this.plot.valToPos(this.activeRange.startMs / 1000, "x");
    const right = this.plot.valToPos(this.activeRange.endMs / 1000, "x");
    this.plot.setSelect(
      { left, top: 0, width: Math.max(0, right - left), height: CHART_HEIGHT },
      false,
    );
  }
}

function describeBucket(bucketWidthMs: number): string {
  if (bucketWidthMs < 1000) {
    return `${bucketWidthMs}ms`;
  }
  if (bucketWidthMs < 60_000) {
    return `${bucketWidthMs / 1000}s`;
  }
  if (bucketWidthMs < 3_600_000) {
    return `${bucketWidthMs / 60_000}m`;
  }
  if (bucketWidthMs < 86_400_000) {
    return `${bucketWidthMs / 3_600_000}h`;
  }
  return `${bucketWidthMs / 86_400_000}d`;
}

customElements.define("looq-timeline", LooqTimeline);
