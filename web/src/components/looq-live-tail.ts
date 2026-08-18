// Live tail UI (`live-tail-ui` spec, `entry-index`/`timeline`/`entry-table` specs,
// design.md D6/D8): connection state, a throttled lines/sec counter, autoscroll
// that pauses on scroll-away, a cumulative gap-loss indicator, a client-side
// eviction indicator, and the stdin-mode privacy indicator (TDR §12 — worded
// differently from file mode's, never the same).
//
// This is its own shell (design.md D7's "the shell" is whichever top-level owns
// the active time range — file mode's is `looq-app.ts`, stream mode's is this
// component): it owns `activeRange` and passes it to the timeline and table
// exactly like `looq-app.ts` does, rather than wiring them to each other.
//
// Virtual-scrolled via `<looq-entry-table>` (`entry-table` spec) — this used to
// render a capped tail window of plain DOM rows (500 in file mode's provisional
// table, 1000 here); both are gone now that the real virtual scroller handles
// front-eviction (`timeline-and-table` proposal.md "Impact": that change had to
// handle front-eviction, not invent it here — this is that change).

import "./looq-detection";
import "./looq-diagnostics";
import "./looq-timeline";
import "./looq-entry-table";
import "./looq-entry-detail";
import "./looq-filter-bar";
import "./looq-workspace";

import { LiveTailSession, type ConnectionState } from "../live-tail";
import { matchesFieldFilters, matchesQuery, type CompiledQuery, type FieldFilters } from "../predicate";
import type { TimeRange } from "../time-range";
import { readAuthToken } from "../token";
import { HashWriter } from "../url-hash";
import type { LooqDetection } from "./looq-detection";
import type { LooqDiagnostics } from "./looq-diagnostics";
import type { LooqEntryDetail } from "./looq-entry-detail";
import type { LooqEntryTable } from "./looq-entry-table";
import type { FiltersChangeDetail, LooqFilterBar } from "./looq-filter-bar";
import type { LooqTimeline } from "./looq-timeline";
import type { LooqWorkspace } from "./looq-workspace";

// D6 batches rendering so a fast producer doesn't turn every line into a layout
// pass, but TDR §11's end-to-end live-tail latency target is <100ms — chosen well
// under that budget (not e.g. 400ms) so batching coalesces bursts without itself
// becoming the dominant term in what a user perceives as "latency" (measured
// end-to-end in docs/devlog.md: task 5.5 of `live-tail`).
const TABLE_RENDER_INTERVAL_MS = 80;
// The timeline's recompute is a full bucket scan (D8: "on an interval, not per
// entry"); it's cheap, but re-creating a `uPlot` instance is heavier per call than
// the table's row-window recompute, so it gets its own, coarser interval rather
// than sharing the 80ms one.
const TIMELINE_RENDER_INTERVAL_MS = 500;
const RATE_INTERVAL_MS = 1000;
/** Within this many pixels of the bottom counts as "at the bottom" for autoscroll
 * purposes — exact-zero would make a sub-pixel layout jitter falsely pause it. */
const AUTOSCROLL_THRESHOLD_PX = 24;

const STATE_LABEL: Record<ConnectionState, string> = {
  connecting: "Connecting…",
  live: "LIVE",
  ended: "Stream ended",
  disconnected: "Disconnected — retrying…",
};

export class LooqLiveTail extends HTMLElement {
  private session: LiveTailSession | null = null;
  private tableRenderTimer: ReturnType<typeof setInterval> | null = null;
  private timelineRenderTimer: ReturnType<typeof setInterval> | null = null;
  private rateTimer: ReturnType<typeof setInterval> | null = null;
  private following = true;
  private indexAttached = false;

  private activeRange: TimeRange | null = null;

  // filtering-and-search state (`filtering` spec, "Filters apply to live
  // entries"): same predicate model as `looq-app.ts`'s file-mode shell, this
  // component being stream mode's own shell (D7 — "the shell" is whichever
  // top-level owns the active range, see the file header). No hash-applied-on-
  // load handling here: a stdin session has no file-selection step for a fresh
  // tab to reproduce (`url-state` spec's scenarios are all framed around
  // reopening a file), so this shell only *writes* the hash, for the copy-link
  // affordance, not read one on connect.
  private fieldFilters: FieldFilters = new Map();
  private compiledQuery: CompiledQuery = { kind: "none" };
  private readonly hashWriter = new HashWriter();

  private stateEl!: HTMLSpanElement;
  private rateEl!: HTMLSpanElement;
  private evictionEl!: HTMLParagraphElement;
  private gapEl!: HTMLParagraphElement;
  private resumeBtn!: HTMLButtonElement;
  private detectionEl!: LooqDetection;
  private diagnosticsEl!: LooqDiagnostics;
  private timelineEl!: LooqTimeline;
  private tableEl!: LooqEntryTable;
  private detailEl!: LooqEntryDetail;
  private filterBarEl!: LooqFilterBar;
  private copyLinkBtn!: HTMLButtonElement;
  private copyStatusEl!: HTMLParagraphElement;

  connectedCallback(): void {
    const maxLines = Number(this.getAttribute("max-lines")) || 100_000;

    // The same `looq-workspace` file mode mounts (design.md D1): only the contents
    // of the panes differ, never the arrangement.
    this.innerHTML = `<looq-workspace></looq-workspace>`;
    const ws = this.querySelector("looq-workspace") as LooqWorkspace;

    ws.pane("topbar").innerHTML = `
      <span class="ws-mode">stdin stream</span>
      <span class="conn-indicator" id="conn-indicator"></span>
      <span class="lines-per-sec" id="rate">0 lines/sec</span>
    `;
    // Gap and eviction notices are not collapsible: a silently shortened live tail
    // looks like "nothing happened" (CLAUDE.md's silent-failure list, ADR-0004).
    ws.pane("messages").innerHTML = `
      <p class="provisional-note eviction-note" id="eviction-note" hidden></p>
      <p class="provisional-note gap-note" id="gap-note" hidden></p>
    `;
    ws.pane("timeline").innerHTML = `<looq-timeline></looq-timeline>`;
    ws.pane("rail").innerHTML = `<looq-filter-bar></looq-filter-bar>`;
    ws.pane("rail-secondary").innerHTML = `
      <looq-detection></looq-detection>
      <looq-diagnostics></looq-diagnostics>
      <details class="rail-section">
        <summary>
          <span class="rail-section-title">Privacy</span>
          <span class="rail-section-state">crosses a local socket</span>
        </summary>
        <div class="rail-section-body">
          <p class="privacy-note">
            Streaming stdin: log lines cross a local WebSocket between the
            <code>looq</code> process and this browser — a different guarantee from
            file mode's "never leaves the browser" (no file is involved here; this
            data crosses a process boundary but not the machine, TDR §12).
          </p>
        </div>
      </details>
      <details class="rail-section">
        <summary><span class="rail-section-title">Share</span></summary>
        <div class="rail-section-body">
          <button type="button" id="copy-link">Copy shareable link</button>
          <p class="share-caveat" id="copy-status" hidden></p>
        </div>
      </details>
    `;
    ws.pane("table-toolbar").innerHTML = `
      <div class="autoscroll-controls" id="autoscroll-controls" hidden>
        <button type="button" id="resume-btn"></button>
      </div>
    `;
    ws.pane("table").innerHTML = `<looq-entry-table></looq-entry-table>`;
    ws.pane("detail").innerHTML = `<looq-entry-detail></looq-entry-detail>`;

    this.stateEl = this.querySelector("#conn-indicator") as HTMLSpanElement;
    this.rateEl = this.querySelector("#rate") as HTMLSpanElement;
    this.evictionEl = this.querySelector("#eviction-note") as HTMLParagraphElement;
    this.gapEl = this.querySelector("#gap-note") as HTMLParagraphElement;
    this.resumeBtn = this.querySelector("#resume-btn") as HTMLButtonElement;
    this.detectionEl = this.querySelector("looq-detection") as LooqDetection;
    this.diagnosticsEl = this.querySelector("looq-diagnostics") as LooqDiagnostics;
    this.timelineEl = this.querySelector("looq-timeline") as LooqTimeline;
    this.tableEl = this.querySelector("looq-entry-table") as LooqEntryTable;
    this.detailEl = this.querySelector("looq-entry-detail") as LooqEntryDetail;
    this.filterBarEl = this.querySelector("looq-filter-bar") as LooqFilterBar;
    this.copyLinkBtn = this.querySelector("#copy-link") as HTMLButtonElement;
    this.copyStatusEl = this.querySelector("#copy-status") as HTMLParagraphElement;

    this.resumeBtn.addEventListener("click", () => this.resumeFollowing());
    this.tableEl.addEventListener("viewport-scroll", (event) => {
      this.handleTableScroll((event as CustomEvent<number>).detail);
    });
    this.tableEl.addEventListener("selection-change", (event) => {
      this.detailEl.setSelectedOrdinal((event as CustomEvent<number | null>).detail);
    });
    this.timelineEl.addEventListener("range-change", (event) => {
      this.setActiveRange((event as CustomEvent<TimeRange | null>).detail);
    });
    this.filterBarEl.addEventListener("range-change", (event) => {
      this.setActiveRange((event as CustomEvent<TimeRange | null>).detail);
    });
    this.filterBarEl.addEventListener("filters-change", (event) => {
      const detail = (event as CustomEvent<FiltersChangeDetail>).detail;
      this.fieldFilters = detail.fieldFilters;
      this.compiledQuery = detail.compiledQuery;
      this.applyPredicate();
      this.scheduleHashWrite();
    });
    this.copyLinkBtn.addEventListener("click", () => this.copyShareableLink());

    const wsUrl = `${location.protocol === "https:" ? "wss:" : "ws:"}//${location.host}/ws`;
    this.session = new LiveTailSession(wsUrl, maxLines, readAuthToken(), (state) =>
      this.renderConnState(state),
    );
    this.renderConnState(this.session.getConnectionState());
    this.session.connect();

    this.tableRenderTimer = setInterval(() => this.tickTableRender(), TABLE_RENDER_INTERVAL_MS);
    this.timelineRenderTimer = setInterval(() => this.tickTimelineRender(), TIMELINE_RENDER_INTERVAL_MS);
    this.rateTimer = setInterval(() => this.tickRate(), RATE_INTERVAL_MS);
  }

  disconnectedCallback(): void {
    if (this.tableRenderTimer !== null) {
      clearInterval(this.tableRenderTimer);
    }
    if (this.timelineRenderTimer !== null) {
      clearInterval(this.timelineRenderTimer);
    }
    if (this.rateTimer !== null) {
      clearInterval(this.rateTimer);
    }
    this.session?.dispose();
    this.session = null;
  }

  /** The one place `activeRange` is written (D7), same pattern as `looq-app.ts`. */
  private setActiveRange(range: TimeRange | null): void {
    this.activeRange = range;
    this.timelineEl.setActiveRange(range);
    this.tableEl.setActiveRange(range);
    this.filterBarEl.setActiveRange(range);
    this.updateFilterCounts();
    this.scheduleHashWrite();
  }

  /** D9: sets the stream's `EntryIndex` predicate so every entry already
   * retained is re-evaluated once (the expensive path), and — because the
   * predicate now lives on the index itself — every arriving entry afterward is
   * tested once on arrival by `EntryIndex.append`, not by re-running this whole
   * method per line (`filtering` spec, "Filter changed mid-stream"). */
  private applyPredicate(): void {
    if (!this.session || !this.indexAttached) {
      return; // filters can be toggled before the first entry lands; nothing to apply to yet
    }
    const index = this.session.getIndex();
    const hasFilters = this.fieldFilters.size > 0 || this.compiledQuery.kind !== "none";
    index.setPredicate(
      hasFilters ? (e) => matchesFieldFilters(e, this.fieldFilters) && matchesQuery(e, this.compiledQuery) : null,
    );
    this.tableEl.setQuery(this.compiledQuery);
    this.detailEl.setQuery(this.compiledQuery);
    this.tableEl.refreshFilters();
    this.timelineEl.refresh();
    this.updateFilterCounts();
  }

  /** Same reasoning as `looq-app.ts`'s method of the same name (D2): the filter
   * bar's count must match what the table shows, range included. */
  private updateFilterCounts(): void {
    if (!this.session || !this.indexAttached) {
      return;
    }
    const index = this.session.getIndex();
    const matching = this.activeRange ? index.countInRange(this.activeRange.startMs, this.activeRange.endMs) : index.matchingCount;
    this.filterBarEl.setCounts(matching, index.totalCount);
  }

  private scheduleHashWrite(): void {
    this.hashWriter.schedule({
      range: this.activeRange,
      fieldFilters: this.fieldFilters,
      query: this.filterBarEl?.getState().queryText ?? "",
      formatOverride: null,
      tzOffsetMinutes: null,
    });
  }

  /** Same caveat, same "flush before copy" reasoning as `looq-app.ts`'s
   * `copyShareableLink` (D8) — duplicated rather than shared because the two
   * shells (file mode vs. stream mode, D7) don't otherwise share a base class. */
  private copyShareableLink(): void {
    this.hashWriter.flush();
    const caveat =
      "Copied — this link encodes your search text and filter values, which are fragments of the log itself.";
    navigator.clipboard
      .writeText(location.href)
      .then(() => {
        this.copyStatusEl.hidden = false;
        this.copyStatusEl.textContent = caveat;
      })
      .catch(() => {
        this.copyStatusEl.hidden = false;
        this.copyStatusEl.textContent = `Couldn't access the clipboard — copy the address bar manually. ${caveat}`;
      });
  }

  private renderConnState(state: ConnectionState): void {
    this.stateEl.textContent = STATE_LABEL[state];
    this.stateEl.className = `conn-indicator conn-${state}`;
  }

  private tickRate(): void {
    if (!this.session) {
      return;
    }
    const n = this.session.takeLineCount();
    this.rateEl.textContent = `${n} lines/sec`;
  }

  private tickTableRender(): void {
    if (!this.session) {
      return;
    }
    this.session.refreshSummary(); // also forces D5's detection-hold timeout
    const summary = this.session.getSummary();
    this.detectionEl.setDetection(summary.detection, summary.format ?? null);
    this.diagnosticsEl.setDiagnostics(summary.diagnostics, summary.entriesEmitted, true);
    // Chips need the cumulative field inventory (`filtering` spec, "Filter chips
    // come from the field inventory") — refreshed on the same cadence as
    // diagnostics, since both come from the same `refreshSummary()` call above.
    this.filterBarEl.setFieldInventory(summary.fieldInventory, this.session.getIndex().levelStats);

    const evictedTotal = this.session.getEvictedTotal();
    if (evictedTotal > 0) {
      this.evictionEl.hidden = false;
      this.evictionEl.textContent =
        `Retaining the most recent --max-lines entries; ${evictedTotal} earlier ` +
        `entr${evictedTotal === 1 ? "y has" : "ies have"} been evicted and are no longer shown (design.md D4).`;
    }
    const gapTotal = this.session.getGapTotal();
    if (gapTotal > 0) {
      this.gapEl.hidden = false;
      this.gapEl.textContent =
        `${gapTotal} line${gapTotal === 1 ? "" : "s"} lost to backpressure so far this session — ` +
        `dropped before parsing, not just hidden from view.`;
    }

    if (!this.indexAttached) {
      // `setIndex` (not `refresh`) exactly once: it resets timeline state like
      // "zoomed to full span" that a `refresh` call must leave alone.
      const index = this.session.getIndex();
      this.tableEl.setIndex(index);
      this.timelineEl.setIndex(index);
      this.detailEl.setIndex(index);
      this.indexAttached = true;
      this.applyPredicate(); // in case a filter was toggled before the first tick attached the index
    } else {
      // `EntryIndex.append` already tested each new entry against the active
      // predicate on arrival (D9) — this just re-derives `displayed` from
      // whatever the index now reports, same as the non-live path.
      this.tableEl.refresh();
      // The selected entry may have just been evicted — the detail pane has to say
      // so rather than keep describing an entry that is gone (D6).
      this.detailEl.refresh();
      this.updateFilterCounts();
    }
    if (this.following) {
      this.tableEl.scrollToBottom();
    }
  }

  private tickTimelineRender(): void {
    if (!this.session || !this.indexAttached) {
      // The table's tick runs more often and is what actually attaches the index
      // (see `tickTableRender`) — this guards against a timeline tick landing
      // before the first table tick has called `setIndex`.
      return;
    }
    this.timelineEl.refresh();
  }

  private handleTableScroll(distanceFromBottom: number): void {
    const atBottom = distanceFromBottom <= AUTOSCROLL_THRESHOLD_PX;
    if (atBottom && !this.following) {
      this.resumeFollowing();
    } else if (!atBottom && this.following) {
      this.following = false;
      this.updateAutoscrollControls();
    }
  }

  private resumeFollowing(): void {
    this.following = true;
    this.updateAutoscrollControls();
    this.tableEl.scrollToBottom();
  }

  private updateAutoscrollControls(): void {
    const controls = this.querySelector("#autoscroll-controls") as HTMLDivElement;
    if (this.following) {
      controls.hidden = true;
      return;
    }
    controls.hidden = false;
    this.resumeBtn.textContent = "Resume following";
  }
}

customElements.define("looq-live-tail", LooqLiveTail);
