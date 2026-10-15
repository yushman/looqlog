// The wiring both shells share (file mode `looqlog-app`, stream mode
// `looqlog-live-tail`): one view session over the `/ws` client, and the table, timeline,
// detail pane, rail and rail surfaces fed from it. The shell keeps what differs per mode
// — banners, indicators, upload, the URL hash — and calls in here for the rest.
// State still lives in one place (design.md D7): filters, search and range are this
// controller's, the components only render and dispatch events.

import { ConnectionLostError } from "../query-client";
import type { QueryReplyDto, StatsReplyDto } from "../query-types";
import type { TimeRange } from "../time-range";
import { ViewSession, type ViewState } from "../view-session";
import type { LooqlogDetection } from "./looqlog-detection";
import type { LooqlogDiagnostics } from "./looqlog-diagnostics";
import type { LooqlogEntryDetail } from "./looqlog-entry-detail";
import type { LooqlogEntryTable } from "./looqlog-entry-table";
import type { FiltersChangeDetail, LooqlogFilterBar } from "./looqlog-filter-bar";
import type { LooqlogTimeline } from "./looqlog-timeline";

export interface ViewElements {
  table: LooqlogEntryTable;
  timeline: LooqlogTimeline;
  detail: LooqlogEntryDetail;
  filterBar: LooqlogFilterBar;
  detection: LooqlogDetection;
  diagnostics: LooqlogDiagnostics;
}

export interface ViewHooks {
  /** The view state changed (for the URL hash). */
  onStateChange: () => void;
  /** A request failed for a reason other than a lost connection. */
  onError: (message: string) => void;
}

export class ViewController {
  readonly session: ViewSession;
  private state: ViewState = { filters: new Map(), search: "", range: null };
  private querySeq = 0;
  lastStats: StatsReplyDto | null = null;

  constructor(
    requester: ConstructorParameters<typeof ViewSession>[0],
    private readonly els: ViewElements,
    private readonly hooks: ViewHooks,
  ) {
    this.session = new ViewSession(requester);
  }

  /** Wires the components' events. Call once. */
  wire(): void {
    const { table, timeline, detail, filterBar } = this.els;
    filterBar.addEventListener("filters-change", (event) => {
      const d = (event as CustomEvent<FiltersChangeDetail>).detail;
      this.state = { ...this.state, filters: d.fieldFilters, search: d.queryText };
      void this.runQuery();
    });
    for (const source of [timeline, filterBar]) {
      source.addEventListener("range-change", (event) => {
        this.setRange((event as CustomEvent<TimeRange | null>).detail);
      });
    }
    table.addEventListener("selection-change", (event) => {
      detail.setSelectedOrdinal((event as CustomEvent<number | null>).detail);
    });
  }

  get range(): TimeRange | null {
    return this.state.range;
  }

  /** The filters and search text the rail holds now, for the URL hash. */
  get filters(): ViewState["filters"] {
    return this.state.filters;
  }

  /** Points every component at this session (a fresh connection or a new source). */
  attach(): void {
    const { table, timeline, detail } = this.els;
    table.setSource(this.session);
    timeline.setSource(this.session);
    detail.setSource(this.session);
  }

  setRange(range: TimeRange | null): void {
    this.state = { ...this.state, range };
    this.els.timeline.setActiveRange(range);
    this.els.filterBar.setActiveRange(range);
    void this.runQuery();
  }

  /** Sends the current view state and redraws everything that reads it. */
  async runQuery(): Promise<QueryReplyDto | null> {
    const seq = ++this.querySeq;
    const sent = this.state;
    let reply: QueryReplyDto;
    try {
      reply = await this.session.query(sent);
    } catch (err) {
      this.reportError(err);
      return null;
    }
    if (seq !== this.querySeq) {
      return reply;
    }
    this.els.filterBar.applyQueryReply(reply, sent.search);
    if (reply.compileError === null) {
      this.state = this.session.state;
      this.els.table.reload();
      this.els.timeline.refresh();
      this.els.detail.refresh();
    }
    this.renderCounts();
    this.hooks.onStateChange();
    return reply;
  }

  /** Detection, diagnostics and the field inventory, re-read from the backend. */
  async refreshStats(cumulative: boolean): Promise<StatsReplyDto | null> {
    let reply: StatsReplyDto;
    try {
      reply = await this.session.stats();
    } catch (err) {
      this.reportError(err);
      return null;
    }
    this.lastStats = reply;
    const { stats } = reply;
    this.els.detection.setDetection(stats.detection, stats.format);
    this.els.diagnostics.setDiagnostics(stats.diagnostics, stats.entries, cumulative);
    this.els.filterBar.setFieldInventory(stats.fieldInventory, new Map(Object.entries(stats.levelCounts)));
    return reply;
  }

  /** A `grown` notification. */
  onGrown(total: number, matched: number, rowCount: number): void {
    this.session.applyGrown(total, matched, rowCount);
    this.renderCounts();
  }

  renderCounts(): void {
    const { matched, total } = this.session.counts;
    this.els.filterBar.setCounts(matched, total);
  }

  private reportError(err: unknown): void {
    // A lost connection is the shell's to show (its indicator/banner); anything else
    // is a real failure the user has to see.
    if (!(err instanceof ConnectionLostError)) {
      this.hooks.onError(err instanceof Error ? err.message : String(err));
    }
  }
}
