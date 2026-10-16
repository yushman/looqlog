// Live tail UI (`live-tail-ui` spec, design.md D6/D8): connection state, a throttled
// lines/sec counter, autoscroll that pauses on scroll-away, the eviction marker, and
// the privacy indicator. Retention, parsing and filtering are the backend's
// (ADR-0010): this shell turns notifications into redraws and leaves the view to a
// `ViewController`. It owns the active range like `looqlog-app.ts` does, through the
// controller, rather than wiring the timeline and table to each other.

import { newSincePause, resumeLabel } from "../resume-label";
import "./looqlog-detection";
import "./looqlog-diagnostics";
import "./looqlog-timeline";
import "./looqlog-entry-table";
import "./looqlog-entry-detail";
import "./looqlog-filter-bar";
import "./looqlog-workspace";
import "./looqlog-uploader";

import { DEFAULT_COLUMN_WIDTHS, type ColumnWidths } from "../column-widths";
import type { CollapsiblePane } from "../panes";
import { QueryClient, socketUrl, type ConnectionState } from "../query-client";
import type { Notification } from "../query-types";
import { goneMessage } from "../routing";
import { readAuthToken } from "../token";
import { formatOverrideFromHash, HashWriter, SHARE_CAVEAT } from "../url-hash";
import type { LooqlogDetection } from "./looqlog-detection";
import type { LooqlogDiagnostics } from "./looqlog-diagnostics";
import type { LooqlogEntryDetail } from "./looqlog-entry-detail";
import type { LooqlogEntryTable } from "./looqlog-entry-table";
import type { LooqlogFilterBar } from "./looqlog-filter-bar";
import type { LooqlogTimeline } from "./looqlog-timeline";
import { PANES_CHANGE_EVENT, type LooqlogWorkspace } from "./looqlog-workspace";
import { ViewController } from "./view-controller";

// Batched rendering (D6): well under TDR §11's <100ms live-tail latency budget, so
// batching coalesces bursts without itself becoming the dominant latency term.
const TABLE_RENDER_INTERVAL_MS = 80;
// A timeline redraw re-creates a `uPlot` instance, so it gets a coarser interval.
const TIMELINE_RENDER_INTERVAL_MS = 500;
const RATE_INTERVAL_MS = 1000;
/** Detection, diagnostics and the field inventory are refreshed on this cadence. */
const STATS_INTERVAL_MS = 1000;
/** Within this many pixels of the bottom counts as "at the bottom" for autoscroll. */
const AUTOSCROLL_THRESHOLD_PX = 24;

const STATE_LABEL: Record<ConnectionState, string> = {
  connecting: "Connecting…",
  live: "LIVE",
  ended: "Stream ended",
  disconnected: "Connection to looqlog lost — retrying…",
};

export class LooqlogLiveTail extends HTMLElement {
  private client: QueryClient | null = null;
  private controller!: ViewController;
  private tableRenderTimer: ReturnType<typeof setInterval> | null = null;
  private timelineRenderTimer: ReturnType<typeof setInterval> | null = null;
  private rateTimer: ReturnType<typeof setInterval> | null = null;
  private statsTimer: ReturnType<typeof setInterval> | null = null;
  private following = true;
  /** `total + evicted` when following stopped; null while following. */
  private seenAtPause: number | null = null;
  private connected = false;
  /** Set by a `grown`/`evicted`, cleared by the next table/timeline tick. */
  private tableDirty = false;
  private timelineDirty = false;
  /** Lines seen at the last rate sample: retained entries plus evicted lines. */
  private lastRateCount: number | null = null;
  private evicted = 0;
  /** Set once the backend says the stream's source is gone (a link from an earlier run). */
  private gone = false;

  /** The table owns the drag, the workspace owns the toggles; this shell owns what
   * the copy-link affordance puts in the URL. A stdin session has no file for a
   * fresh tab to reproduce, so only the format and timezone override are read back
   * from the hash; everything else is written, never read. */
  private formatOverride: string | null = null;
  private tzOffsetMinutes: number | null = null;
  private tzName: string | null = null;
  private overrideApplied = false;
  private columnWidths: ColumnWidths = DEFAULT_COLUMN_WIDTHS;
  private collapsedPanes: Set<CollapsiblePane> = new Set();
  private readonly hashWriter = new HashWriter();

  private workspace!: LooqlogWorkspace;
  private stateEl!: HTMLSpanElement;
  private rateEl!: HTMLSpanElement;
  private resumeBtn!: HTMLButtonElement;
  private timelineEl!: LooqlogTimeline;
  private tableEl!: LooqlogEntryTable;
  private filterBarEl!: LooqlogFilterBar;
  private copyStatusEl!: HTMLParagraphElement;
  private errorEl!: HTMLParagraphElement;

  connectedCallback(): void {
    // The same `looqlog-workspace` file mode mounts (design.md D1): only the contents
    // of the panes differ, never the arrangement.
    const override = formatOverrideFromHash(location.hash);
    this.formatOverride = override?.formatOverride ?? null;
    this.tzOffsetMinutes = override?.tzOffsetMinutes ?? null;
    this.tzName = override?.tzName ?? null;
    this.innerHTML = `<looqlog-workspace></looqlog-workspace>`;
    const ws = this.querySelector("looqlog-workspace") as LooqlogWorkspace;
    this.workspace = ws;

    ws.pane("topbar").innerHTML = `
      <span class="ws-mode">stdin stream</span>
      <span class="conn-indicator" id="conn-indicator"></span>
      <span class="lines-per-sec" id="rate">0 lines/sec</span>
    `;
    ws.pane("messages").innerHTML = `
      <p class="provisional-note error-note" id="error-note" hidden></p>
    `;
    ws.pane("timeline").innerHTML = `<looqlog-timeline></looqlog-timeline>`;
    ws.pane("rail").innerHTML = `<looqlog-filter-bar></looqlog-filter-bar>`;
    ws.pane("rail-secondary").innerHTML = `
      <looqlog-detection></looqlog-detection>
      <looqlog-diagnostics></looqlog-diagnostics>
      <details class="rail-section">
        <summary>
          <span class="rail-section-title">Privacy</span>
          <span class="rail-section-state">never leaves the machine</span>
        </summary>
        <div class="rail-section-body">
          <p class="privacy-note">
            Streaming stdin: the data is read by the local <code>looqlog</code> process
            and never leaves the machine — it reaches this page over a local WebSocket
            on the default bind (ADR-0010).
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
    ws.pane("table").innerHTML = `<looqlog-entry-table></looqlog-entry-table>`;
    ws.pane("detail").innerHTML = `<looqlog-entry-detail></looqlog-entry-detail>`;

    this.stateEl = this.querySelector("#conn-indicator") as HTMLSpanElement;
    this.rateEl = this.querySelector("#rate") as HTMLSpanElement;
    this.resumeBtn = this.querySelector("#resume-btn") as HTMLButtonElement;
    this.timelineEl = this.querySelector("looqlog-timeline") as LooqlogTimeline;
    this.tableEl = this.querySelector("looqlog-entry-table") as LooqlogEntryTable;
    this.filterBarEl = this.querySelector("looqlog-filter-bar") as LooqlogFilterBar;
    this.copyStatusEl = this.querySelector("#copy-status") as HTMLParagraphElement;
    this.errorEl = this.querySelector("#error-note") as HTMLParagraphElement;

    this.resumeBtn.addEventListener("click", () => this.resumeFollowing());
    this.tableEl.addEventListener("viewport-scroll", (event) => {
      this.handleTableScroll((event as CustomEvent<number>).detail);
    });
    this.tableEl.addEventListener("selection-change", (event) => {
      if ((event as CustomEvent<number | null>).detail !== null) {
        this.workspace.expandPane("detail");
      }
    });
    this.workspace.addEventListener(PANES_CHANGE_EVENT, (event) => {
      this.collapsedPanes = (event as CustomEvent<Set<CollapsiblePane>>).detail;
      this.scheduleHashWrite();
    });
    this.tableEl.addEventListener("columns-change", (event) => {
      this.columnWidths = (event as CustomEvent<ColumnWidths>).detail;
      this.scheduleHashWrite();
    });
    (this.querySelector("#copy-link") as HTMLButtonElement).addEventListener("click", () =>
      this.copyShareableLink(),
    );

    const client = new QueryClient({
      url: socketUrl(),
      token: readAuthToken(),
      source: this.getAttribute("source-id"),
      onState: (state) => this.renderConnState(state),
      onNotification: (n) => this.onNotification(n),
      onConnected: () => void this.onConnected(),
    });
    this.client = client;
    this.controller = new ViewController(
      client,
      {
        table: this.tableEl,
        timeline: this.timelineEl,
        detail: this.querySelector("looqlog-entry-detail") as LooqlogEntryDetail,
        filterBar: this.filterBarEl,
        detection: this.querySelector("looqlog-detection") as LooqlogDetection,
        diagnostics: this.querySelector("looqlog-diagnostics") as LooqlogDiagnostics,
      },
      {
        onStateChange: () => this.scheduleHashWrite(),
        onError: (message) => {
          if (this.gone) {
            return;
          }
          this.errorEl.hidden = false;
          this.errorEl.textContent = message;
        },
      },
    );
    this.controller.wire();
    this.renderConnState(client.getState());
    client.connect();

    this.tableRenderTimer = setInterval(() => this.tickTableRender(), TABLE_RENDER_INTERVAL_MS);
    this.timelineRenderTimer = setInterval(() => this.tickTimelineRender(), TIMELINE_RENDER_INTERVAL_MS);
    this.rateTimer = setInterval(() => this.tickRate(), RATE_INTERVAL_MS);
    this.statsTimer = setInterval(() => {
      if (this.connected) {
        void this.controller.refreshStats(true);
      }
    }, STATS_INTERVAL_MS);
  }

  disconnectedCallback(): void {
    for (const t of [this.tableRenderTimer, this.timelineRenderTimer, this.rateTimer, this.statsTimer]) {
      if (t !== null) {
        clearInterval(t);
      }
    }
    this.client?.dispose();
    this.client = null;
  }

  /** Every (re)connect re-issues the current query, so the view shows each retained
   * entry exactly once; the first notifications carry the totals. */
  private async onConnected(): Promise<void> {
    if (!this.connected) {
      this.controller.attach();
    }
    this.connected = true;
    if (!this.overrideApplied) {
      this.overrideApplied = true;
      if (this.formatOverride !== null || this.tzOffsetMinutes !== null || this.tzName !== null) {
        await this.applyFormatOverride();
      }
    }
    await this.controller.refreshStats(true);
    await this.controller.runQuery();
    if (this.following) {
      this.tableEl.scrollToBottom();
    }
  }

  private onNotification(n: Notification): void {
    switch (n.type) {
      case "grown":
        this.controller.onGrown(n.total, n.matched, n.rowCount);
        this.tableDirty = true;
        this.timelineDirty = true;
        break;
      case "evicted":
        this.evicted = n.count;
        this.tableEl.setEvicted(n.count);
        this.tableDirty = true;
        this.timelineDirty = true;
        break;
      case "reset":
        void this.controller.refreshStats(true).then(() => this.controller.runQuery());
        break;
      case "ended":
        void this.controller.refreshStats(true);
        break;
      case "closed":
        this.showGone(goneMessage(n.reason));
        break;
      case "progress":
      case "sources":
        break;
    }
  }

  /** Never an empty table: the stream's place says why it is gone and offers an upload. */
  private showGone(message: string): void {
    this.gone = true;
    const prompt = document.createElement("div");
    prompt.className = "unopened-prompt";
    prompt.innerHTML = `
      <p class="gone-note"></p>
      <looqlog-uploader></looqlog-uploader>
      <p><a href="/">Back to the source list</a></p>
    `;
    (prompt.querySelector(".gone-note") as HTMLParagraphElement).textContent = message;
    this.workspace.pane("table").replaceChildren(prompt);
    this.workspace.setTimelineVisible(false);
  }

  private async applyFormatOverride(): Promise<void> {
    try {
      await this.controller.session.setFormat(this.formatOverride, this.tzOffsetMinutes, this.tzName);
    } catch (err) {
      this.errorEl.hidden = false;
      this.errorEl.textContent = `could not apply the format override: ${err instanceof Error ? err.message : String(err)}`;
    }
  }

  private scheduleHashWrite(): void {
    this.hashWriter.schedule({
      range: this.controller?.range ?? null,
      fieldFilters: this.controller?.filters ?? new Map(),
      query: this.filterBarEl?.getState().queryText ?? "",
      formatOverride: this.formatOverride,
      tzOffsetMinutes: this.tzOffsetMinutes,
      tzName: this.tzName,
      columnWidths: this.columnWidths,
      collapsedPanes: this.collapsedPanes,
    });
  }

  /** Same caveat, same "flush before copy" reasoning as `looqlog-app.ts`. */
  private copyShareableLink(): void {
    this.hashWriter.flush();
    const caveat = SHARE_CAVEAT;
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

  private seenTotal(): number {
    return this.controller.session.counts.total + this.evicted;
  }

  /** Lines/sec from the cumulative counts: retained entries plus evicted lines. */
  private tickRate(): void {
    const seen = this.seenTotal();
    const n = this.lastRateCount === null ? 0 : Math.max(0, seen - this.lastRateCount);
    this.lastRateCount = seen;
    this.rateEl.textContent = `${n} lines/sec`;
  }

  private tickTableRender(): void {
    if (!this.tableDirty) {
      return;
    }
    this.tableDirty = false;
    if (this.following) {
      this.tableEl.scrollToBottom();
    } else {
      this.tableEl.refresh();
      this.updateAutoscrollControls();
    }
  }

  private tickTimelineRender(): void {
    if (!this.timelineDirty) {
      return;
    }
    this.timelineDirty = false;
    this.timelineEl.refresh();
  }

  private handleTableScroll(distanceFromBottom: number): void {
    const atBottom = distanceFromBottom <= AUTOSCROLL_THRESHOLD_PX;
    if (atBottom && !this.following) {
      this.resumeFollowing();
    } else if (!atBottom && this.following) {
      this.following = false;
      this.seenAtPause = this.seenTotal();
      this.updateAutoscrollControls();
    }
  }

  private resumeFollowing(): void {
    this.following = true;
    this.seenAtPause = null;
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
    const added = this.seenAtPause === null ? 0 : newSincePause(this.seenTotal(), this.seenAtPause);
    this.resumeBtn.textContent = resumeLabel(added);
  }
}

customElements.define("looqlog-live-tail", LooqlogLiveTail);
