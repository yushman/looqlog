// The shell. It routes on the path (`/` is the landing, `/l/<id>` one source). For a
// file source it owns the source-level state — what file it is, how far its ingest got,
// what failed, whether it is gone — and the URL hash, and leaves the view (filters,
// table, timeline, detail) to a `ViewController` over the backend (ADR-0010). Children
// only dispatch events; they never hold source state themselves.

import "./looqlog-detection";
import "./looqlog-diagnostics";
import "./looqlog-timeline";
import "./looqlog-entry-table";
import "./looqlog-entry-detail";
import "./looqlog-live-tail";
import "./looqlog-filter-bar";
import "./looqlog-workspace";
import "./looqlog-landing";
import "./looqlog-uploader";

import { DEFAULT_COLUMN_WIDTHS, type ColumnWidths } from "../column-widths";
import type { CollapsiblePane } from "../panes";
import { QueryClient, socketUrl, type ConnectionState } from "../query-client";
import type { CloseReason, Notification, SourceInfoDto, StatsReplyDto } from "../query-types";
import { goneMessage, routeFor } from "../routing";
import { readAuthToken } from "../token";
import { decodeHash, HashWriter, SHARE_CAVEAT, type DecodedHash } from "../url-hash";
import type { LooqlogDetection } from "./looqlog-detection";
import type { LooqlogDiagnostics } from "./looqlog-diagnostics";
import type { LooqlogEntryDetail } from "./looqlog-entry-detail";
import type { LooqlogEntryTable } from "./looqlog-entry-table";
import type { LooqlogFilterBar } from "./looqlog-filter-bar";
import type { LooqlogTimeline } from "./looqlog-timeline";
import { PANES_CHANGE_EVENT, type LooqlogWorkspace } from "./looqlog-workspace";
import { ViewController } from "./view-controller";

/** While a file is being read, live redraws are throttled to these intervals. */
const TABLE_REFRESH_MS = 300;
const TIMELINE_REFRESH_MS = 500;
const STATS_REFRESH_MS = 1000;

/** Mirrors the `__LOOQLOG_MODE__` JSON substituted server-side (`crates/looqlog/src/server.rs`). */
type RunMode = { mode: "file" } | { mode: "stdin"; maxLines: number };

/** Reads the `#mode-template` placeholder. Falls back to file mode both when the element
 * is missing and when its content is still the literal placeholder (`vite dev`).
 * `<template>` content is inert, so `.innerHTML` is what reflects it. */
function readRunMode(): RunMode {
  const template = document.getElementById("mode-template") as HTMLTemplateElement | null;
  const raw = template?.innerHTML.trim() ?? "";
  if (!raw || raw === "__LOOQLOG_MODE__") {
    return { mode: "file" };
  }
  try {
    const parsed = JSON.parse(raw) as RunMode;
    if (parsed.mode === "stdin" && typeof parsed.maxLines === "number") {
      return parsed;
    }
    if (parsed.mode === "file") {
      return parsed;
    }
  } catch {
    // fall through to the file-mode default below
  }
  return { mode: "file" };
}

export class LooqlogApp extends HTMLElement {
  private client: QueryClient | null = null;
  private controller!: ViewController;
  private source: SourceInfoDto | null = null;
  private stats: StatsReplyDto["stats"] | null = null;
  private progress: { bytes: number; total: number; done: boolean } | null = null;
  /** The last failure that is not the source's own (a request failed), shown in a
   * persistent banner until dismissed or superseded. */
  private errorMessage: string | null = null;
  private connState: ConnectionState = "connecting";
  /** Set once the backend says this tab's source is gone. */
  private closed: CloseReason | null = null;
  private firstConnect = true;

  private formatOverride: string | null = null;
  private tzOffsetMinutes: number | null = null;
  private tzName: string | null = null;
  private columnWidths: ColumnWidths = DEFAULT_COLUMN_WIDTHS;
  private collapsedPanes: Set<CollapsiblePane> = new Set();
  private readonly hashWriter = new HashWriter();
  /** Parsed once at connect, applied once the first source has stats to validate
   * against, then discarded (`url-state` spec, "A hash is applied on load"). */
  private pendingHash: DecodedHash | null = null;

  private tableTimer: ReturnType<typeof setTimeout> | null = null;
  private timelineTimer: ReturnType<typeof setTimeout> | null = null;
  private statsTimer: ReturnType<typeof setTimeout> | null = null;

  private workspace!: LooqlogWorkspace;
  private timelineEl!: LooqlogTimeline;
  private tableEl!: LooqlogEntryTable;
  private filterBarEl!: LooqlogFilterBar;
  private statusEl!: HTMLParagraphElement;
  private errorBannerEl!: HTMLDivElement;
  private connNoteEl!: HTMLParagraphElement;
  private sizeNoteEl!: HTMLParagraphElement;
  private hashNoticeEl!: HTMLParagraphElement;
  private copyStatusEl!: HTMLParagraphElement;
  /** The center pane's prompt when there is nothing to show: the source failed, is
   * gone, or has not produced anything yet. Holds the upload target. */
  private unopenedEl!: HTMLElement;
  private goneEl!: HTMLParagraphElement;

  connectedCallback(): void {
    const route = routeFor(location.pathname);
    if (route.kind === "landing") {
      this.innerHTML = `<looqlog-landing></looqlog-landing>`;
      return;
    }
    const runMode = readRunMode();
    if (runMode.mode === "stdin") {
      const liveTail = document.createElement("looqlog-live-tail");
      liveTail.setAttribute("max-lines", String(runMode.maxLines));
      liveTail.setAttribute("source-id", route.id);
      this.replaceChildren(liveTail);
      return;
    }

    this.innerHTML = `<looqlog-workspace></looqlog-workspace>`;
    const ws = this.querySelector("looqlog-workspace") as LooqlogWorkspace;
    this.workspace = ws;

    ws.pane("topbar").innerHTML = `
      <span class="ws-mode">file mode</span>
      <p class="status" id="status"></p>
    `;
    ws.pane("messages").innerHTML = `
      <p class="provisional-note conn-note" id="conn-note" hidden></p>
      <div class="error-banner" id="error-banner" hidden>
        <span id="error-banner-text"></span>
        <button type="button" id="error-banner-dismiss" aria-label="Dismiss">&times;</button>
      </div>
      <p class="provisional-note size-note" id="size-note" hidden></p>
      <p class="hash-notice" id="hash-notice" hidden></p>
    `;
    ws.pane("timeline").innerHTML = `<looqlog-timeline></looqlog-timeline>`;
    ws.pane("rail").innerHTML = `<looqlog-filter-bar></looqlog-filter-bar>`;
    ws.pane("rail-secondary").innerHTML = `
      <details class="rail-section" id="open-file-section">
        <summary><span class="rail-section-title">Open another file</span></summary>
        <div class="rail-section-body">
          <p class="privacy-note">This tab stays on this log. Other files open from the source list.</p>
          <a href="/" target="_blank" rel="noopener">Source list in a new tab</a>
        </div>
      </details>
      <looqlog-detection></looqlog-detection>
      <looqlog-diagnostics></looqlog-diagnostics>
      <details class="rail-section">
        <summary>
          <span class="rail-section-title">Privacy</span>
          <span class="rail-section-state">never leaves the machine</span>
        </summary>
        <div class="rail-section-body">
          <p class="privacy-note">
            The file is read by the local <code>looqlog</code> process and never leaves
            the machine: it reaches this page over a local connection on the default
            bind (ADR-0010). A file uploaded from the source list goes to that process,
            not to any server elsewhere.
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
    ws.pane("table").innerHTML = `
      <div class="unopened-prompt" id="unopened" hidden>
        <p class="gone-note" id="gone-note" hidden></p>
        <looqlog-uploader></looqlog-uploader>
        <p><a href="/">Back to the source list</a></p>
      </div>
      <looqlog-entry-table></looqlog-entry-table>
    `;
    ws.pane("detail").innerHTML = `<looqlog-entry-detail></looqlog-entry-detail>`;

    this.timelineEl = this.querySelector("looqlog-timeline") as LooqlogTimeline;
    this.tableEl = this.querySelector("looqlog-entry-table") as LooqlogEntryTable;
    this.filterBarEl = this.querySelector("looqlog-filter-bar") as LooqlogFilterBar;
    const detailEl = this.querySelector("looqlog-entry-detail") as LooqlogEntryDetail;
    this.statusEl = this.querySelector("#status") as HTMLParagraphElement;
    this.unopenedEl = this.querySelector("#unopened") as HTMLElement;
    this.goneEl = this.querySelector("#gone-note") as HTMLParagraphElement;
    this.errorBannerEl = this.querySelector("#error-banner") as HTMLDivElement;
    this.connNoteEl = this.querySelector("#conn-note") as HTMLParagraphElement;
    this.sizeNoteEl = this.querySelector("#size-note") as HTMLParagraphElement;
    this.hashNoticeEl = this.querySelector("#hash-notice") as HTMLParagraphElement;
    this.copyStatusEl = this.querySelector("#copy-status") as HTMLParagraphElement;
    this.workspace.setTimelineVisible(false);

    (this.querySelector("#error-banner-dismiss") as HTMLButtonElement).addEventListener("click", () => {
      this.errorMessage = null;
      this.renderError();
    });

    if (location.hash.length > 1) {
      this.pendingHash = decodeHash(location.hash);
      // Collapsed panes need no dataset to validate against, so they apply at once.
      this.collapsedPanes = this.pendingHash.collapsedPanes;
      ws.setCollapsedPanes(this.collapsedPanes);
      this.formatOverride = this.pendingHash.formatOverride;
      this.tzOffsetMinutes = this.pendingHash.tzOffsetMinutes;
      this.tzName = this.pendingHash.tzName;
    }

    this.tableEl.addEventListener("selection-change", (event) => {
      // A selection whose only visible effect is inside a collapsed pane looks like
      // a broken click, so selecting opens the pane.
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
      source: route.id,
      onState: (state) => this.onConnState(state),
      onNotification: (n) => this.onNotification(n),
      onConnected: () => void this.onConnected(),
    });
    this.client = client;
    this.controller = new ViewController(
      client,
      {
        table: this.tableEl,
        timeline: this.timelineEl,
        detail: detailEl,
        filterBar: this.filterBarEl,
        detection: this.querySelector("looqlog-detection") as LooqlogDetection,
        diagnostics: this.querySelector("looqlog-diagnostics") as LooqlogDiagnostics,
      },
      {
        onStateChange: () => this.scheduleHashWrite(),
        onError: (message) => {
          if (this.closed !== null) {
            return; // the gone-source state already says why requests fail
          }
          this.errorMessage = message;
          this.renderError();
        },
      },
    );
    this.controller.wire();
    this.renderStatus();
    client.connect();
  }

  disconnectedCallback(): void {
    this.client?.dispose();
    this.client = null;
    for (const t of [this.tableTimer, this.timelineTimer, this.statsTimer]) {
      if (t !== null) {
        clearTimeout(t);
      }
    }
  }

  /** Every (re)connect: re-read the source and re-issue the current query, so the
   * view shows each entry exactly once. The first one also applies the URL hash. */
  private async onConnected(): Promise<void> {
    const first = this.firstConnect;
    this.firstConnect = false;
    if (first) {
      this.controller.attach();
      if (this.formatOverride !== null || this.tzOffsetMinutes !== null || this.tzName !== null) {
        await this.applyFormatOverride();
      }
    }
    await this.loadStats();
    if (first && this.pendingHash) {
      this.applyPendingHash(this.pendingHash);
      this.pendingHash = null;
    } else {
      await this.controller.runQuery();
    }
  }

  private async applyFormatOverride(): Promise<void> {
    try {
      await this.controller.session.setFormat(this.formatOverride, this.tzOffsetMinutes, this.tzName);
    } catch (err) {
      this.errorMessage = `could not apply the format override: ${err instanceof Error ? err.message : String(err)}`;
      this.renderError();
    }
  }

  private async loadStats(): Promise<void> {
    const reply = await this.controller.refreshStats(false);
    if (reply) {
      this.source = reply.source;
      this.stats = reply.stats;
      this.renderSource();
    }
  }

  private onConnState(state: ConnectionState): void {
    this.connState = state;
    this.connNoteEl.hidden = state !== "disconnected";
    this.connNoteEl.textContent =
      state === "disconnected"
        ? "The connection to looqlog was lost — retrying. If the looqlog process has exited, start it again."
        : "";
  }

  private onNotification(n: Notification): void {
    switch (n.type) {
      case "progress":
        this.progress = { bytes: n.bytes, total: n.total, done: n.done };
        this.renderStatus();
        if (n.done) {
          void this.loadStats().then(() => {
            this.tableEl.refresh();
            this.timelineEl.refresh();
          });
        } else {
          this.throttleStats();
        }
        break;
      case "grown":
        this.controller.onGrown(n.total, n.matched, n.rowCount);
        this.throttle("table", TABLE_REFRESH_MS, () => this.tableEl.refresh());
        this.throttle("timeline", TIMELINE_REFRESH_MS, () => this.timelineEl.refresh());
        this.renderStatus();
        break;
      case "reset":
        // This source was re-parsed, by this tab or another one on it: same lines,
        // same ordinals, so the view stays and re-queries.
        void this.loadStats().then(() => this.controller.runQuery());
        break;
      case "closed":
        this.closed = n.reason;
        this.errorMessage = null;
        this.renderSource();
        break;
      case "evicted":
      case "ended":
      case "sources":
        break;
    }
  }

  private throttle(which: "table" | "timeline", ms: number, fn: () => void): void {
    const key = which === "table" ? "tableTimer" : "timelineTimer";
    if (this[key] !== null) {
      return;
    }
    this[key] = setTimeout(() => {
      this[key] = null;
      fn();
    }, ms);
  }

  private throttleStats(): void {
    if (this.statsTimer !== null) {
      return;
    }
    this.statsTimer = setTimeout(() => {
      this.statsTimer = null;
      void this.loadStats();
    }, STATS_REFRESH_MS);
  }

  /** Shows the workspace while the source has entries (or is being read), the prompt
   * with an upload target when it failed or is gone — never an empty table — and the
   * source's own error or size warning. */
  private renderSource(): void {
    const source = this.closed === null ? this.source : null;
    const failed = source?.error != null && (this.stats?.entries ?? 0) === 0;
    const prompt = this.closed !== null || failed;
    this.unopenedEl.hidden = !prompt;
    this.tableEl.hidden = prompt;
    this.workspace.setTimelineVisible(source !== null && !prompt);
    this.goneEl.hidden = this.closed === null;
    this.goneEl.textContent = this.closed === null ? "" : goneMessage(this.closed);
    this.sizeNoteEl.hidden = !source?.warning;
    this.sizeNoteEl.textContent = source?.warning ? `${source.name}: ${source.warning}` : "";
    if (source?.error) {
      this.errorMessage = source.error;
    }
    this.renderError();
    this.renderStatus();
  }

  private renderStatus(): void {
    const name = this.closed === null ? (this.source?.name ?? null) : null;
    const stats = this.stats;
    if (name === null) {
      this.statusEl.textContent = this.connState === "connecting" && this.closed === null ? "Connecting…" : "";
      this.statusEl.className = "status";
      return;
    }
    const progress = this.progress;
    if (progress && !progress.done) {
      const pct = progress && progress.total > 0 ? Math.round((progress.bytes / progress.total) * 100) : 0;
      this.statusEl.textContent = `Parsing ${name}... ${pct}% (${this.controller.session.counts.total} entries so far)`;
      this.statusEl.className = "status parsing";
      return;
    }
    if (!stats || this.source?.error) {
      this.statusEl.textContent = "";
      this.statusEl.className = "status";
      return;
    }
    if (stats.finished && stats.totalLines === 0) {
      // `error-states` spec, "Empty file" — distinct from a filter matching nothing.
      this.statusEl.textContent = `${name} is empty — there is nothing to parse.`;
      this.statusEl.className = "status done zero-entries-warning";
    } else if (stats.entries === 0 && stats.totalLines > 0) {
      this.statusEl.textContent =
        `${name}: read ${stats.totalLines} line(s), but none matched a known log ` +
        `format (0 entries) — try a different file, or override detection with #format=.`;
      this.statusEl.className = "status done zero-entries-warning";
    } else {
      this.statusEl.textContent =
        `${name}: ${stats.entries} entries, ${stats.blankLines} blank lines, ${stats.totalLines} total lines`;
      this.statusEl.className = "status done";
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

  /** The caveat is presented at the moment of copying; `flush()` first so the copied
   * URL reflects what is on screen right now. */
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

  /** Applies a hash decoded on load once the source's field inventory exists to
   * validate it against: recognised keys and known field names are applied, the rest
   * is named in a visible notice rather than silently dropped. */
  private applyPendingHash(decoded: DecodedHash): void {
    const notices: string[] = [];
    if (decoded.unknownKeys.length > 0) {
      notices.push(`unrecognised hash key(s): ${decoded.unknownKeys.join(", ")}`);
    }
    if (decoded.rangeError) {
      notices.push(decoded.rangeError);
    }
    if (decoded.malformedFilterCount > 0) {
      notices.push(
        `${decoded.malformedFilterCount} malformed filter entr${decoded.malformedFilterCount === 1 ? "y" : "ies"}`,
      );
    }
    this.columnWidths = decoded.columnWidths;
    this.tableEl.setColumnWidths(decoded.columnWidths);
    notices.push(...decoded.columnWidthErrors);
    notices.push(...decoded.collapsedPaneErrors);

    const known = new Set(["level", ...Object.keys(this.stats?.fieldInventory.fields ?? {})]);
    const validFilters = new Map<string, Set<string>>();
    const unknownFields: string[] = [];
    for (const [field, values] of decoded.fieldFilters) {
      if (known.has(field)) {
        validFilters.set(field, values);
      } else {
        unknownFields.push(field);
      }
    }
    if (unknownFields.length > 0) {
      notices.push(`field(s) not present in this file: ${unknownFields.join(", ")}`);
    }

    if (decoded.range) {
      this.controller.setRange(decoded.range);
    }
    // Dispatches `filters-change`, which sends the query with the range just set.
    this.filterBarEl.applyExternalState(validFilters, decoded.query);

    this.hashNoticeEl.hidden = notices.length === 0;
    this.hashNoticeEl.textContent =
      notices.length > 0 ? `From the shared URL, could not be applied: ${notices.join("; ")}.` : "";
  }

  /** `error-states` spec, "Errors are reachable, not transient": a persistent banner,
   * independent of the status line. */
  private renderError(): void {
    this.errorBannerEl.hidden = this.errorMessage === null;
    (this.querySelector("#error-banner-text") as HTMLSpanElement).textContent = this.errorMessage ?? "";
  }
}

customElements.define("looqlog-app", LooqlogApp);
