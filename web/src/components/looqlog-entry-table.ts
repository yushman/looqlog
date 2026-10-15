// Virtual-scrolled entry table (`entry-table` spec, design.md D2/D6/D7). Renders
// only the rows in and near the viewport regardless of dataset size (fixed row
// height, D6), and survives live growth/eviction by anchoring the visible window to
// a stable row identity (`RowDto.ordinal`, D2) across each `refresh()` rather than to
// a raw scroll offset that eviction would silently invalidate.
//
// Rows come from the backend (ADR-0010): the table asks the view session for windows
// of the current, chain-collapsed result set and keeps only a two-screen overscan
// cache around the viewport — the browser never holds the log.
//
// The detail view is no longer part of this component (`frontend-three-pane-layout`
// D6): the table holds the selection and emits it as `selection-change`, and the
// workspace's right pane (`looqlog-entry-detail`) renders it — so inspecting an entry
// cannot reflow the rows around it.

import {
  clampColumnWidth,
  DEFAULT_COLUMN_WIDTHS,
  RESIZABLE_COLUMNS,
  type ColumnWidths,
  type ResizableColumn,
} from "../column-widths";
import type { RowDto } from "../query-types";
import type { ViewSession } from "../view-session";

/** Uniform row height in CSS pixels — what makes virtual scrolling arithmetic
 * (index * height) rather than a measurement pass (D6).
 *
 * Exported because `style.css` carries the same number in `.entry-row { height }`
 * (design D3 — the height stopped being an inline style, which is what let the CSP
 * drop `'unsafe-inline'`). The two cannot be made to share one literal across a
 * `.ts`/`.css` boundary, so `entry-table-styles.test.ts` asserts they agree
 * instead: a drift would misplace every row by a growing offset, silently. */
export const ROW_HEIGHT = 24;
/** Extra rows rendered above/below the visible window so a fast scroll or
 * scrollbar drag doesn't show blank space for a frame. */
const OVERSCAN_ROWS = 8;
/** Messages are truncated at this many characters, not just visually clipped by
 * CSS — keeps each row's rendered text small regardless of how long the source
 * line was, and gives a `…` continuation marker that survives copy-paste. */
const MESSAGE_TRUNCATE_CHARS = 300;

export class LooqlogEntryTable extends HTMLElement {
  private session: ViewSession | null = null;
  /** Visible-row index → row, for the window last fetched around the viewport. */
  private cache = new Map<number, RowDto>();
  /** Bumped per fetch; a reply for an older fetch is dropped. */
  private fetchSeq = 0;
  private fetchFrame: number | null = null;
  private pendingFetchIndex = 0;
  private selectedOrdinal: number | null = null;
  private evictionBannerVisible = false;

  private summaryEl!: HTMLParagraphElement;
  private evictionEl!: HTMLParagraphElement;
  private evictedMarkerEl!: HTMLParagraphElement;
  private headerEl!: HTMLDivElement;
  private viewportEl!: HTMLDivElement;
  private rowsEl!: HTMLDivElement;

  /** Column widths in `rem` (`entry-table` spec, "Columns can be resized"). Held
   * here, written to the stylesheet below, and mirrored into the URL hash by
   * whichever shell owns this table — the component never touches the hash itself
   * (design.md D7: state travels through the shell). */
  private columnWidths: ColumnWidths = DEFAULT_COLUMN_WIDTHS;
  /** The application's own stylesheet and the three rules this component inserts
   * into it and then mutates, instead of writing `style` attributes (design
   * D2/D3/D4, `security` spec: `style-src` no longer permits inline styles). The
   * rules are non-optional: `createStyleSheet` throws if the sheet is missing
   * rather than leaving them unset (task 5.8), because a scroller with no rules
   * renders the first screenful of any dataset and scrolls nowhere. */
  private sheet: CSSStyleSheet | null = null;
  private columnsRule!: CSSStyleRule;
  private spacerRule!: CSSStyleRule;
  private rowsRule!: CSSStyleRule;
  /** In-flight column drag, or `null`. Started only from a header handle — there
   * are no handles in data rows, so a drag can never begin on a row the user meant
   * to select (`entry-table` spec, "Resizing does not select a row"). */
  private drag: {
    column: ResizableColumn;
    pointerId: number;
    handle: HTMLElement;
    startX: number;
    startWidthRem: number;
    remPx: number;
  } | null = null;

  connectedCallback(): void {
    // Scopes this instance's generated rules. File mode and stream mode each mount
    // their own table (`looqlog-app` / `looqlog-live-tail`), and a shared selector would
    // have one instance's scroll offset positioning the other's rows.
    const scope = `t${++tableInstanceSeq}`;
    this.dataset.looqlogTable = scope;

    this.innerHTML = `
      <div class="entry-table-wrap">
        <div class="entry-table-head">
          <p class="entry-table-summary" id="table-summary"></p>
          <button type="button" class="col-reset-all" id="reset-columns" hidden>Reset columns</button>
        </div>
        <p class="provisional-note eviction-marker" id="evicted-marker" hidden></p>
        <p class="provisional-note eviction-banner" id="eviction-banner" hidden>
          Earlier entries you were viewing are no longer retained (evicted at
          <code>--max-lines</code>) — jumped to the oldest entry still available.
        </p>
        <div class="entry-table-header" role="row" id="header">
          <span class="col-ordinal"><span class="header-label">#</span>${resizerHtml("ordinal", "#")}</span>
          <span class="col-timestamp"><span class="header-label">timestamp (UTC)</span>${resizerHtml("timestamp", "timestamp")}</span>
          <span class="col-level"><span class="header-label">level</span>${resizerHtml("level", "level")}</span>
          <span class="col-message"><span class="header-label">message</span></span>
        </div>
        <div class="entry-table-viewport" id="viewport" tabindex="0">
          <div class="entry-table-spacer" id="spacer">
            <div class="entry-table-rows" id="rows"></div>
          </div>
        </div>
      </div>
    `;
    this.summaryEl = this.querySelector("#table-summary") as HTMLParagraphElement;
    this.evictionEl = this.querySelector("#eviction-banner") as HTMLParagraphElement;
    this.evictedMarkerEl = this.querySelector("#evicted-marker") as HTMLParagraphElement;
    this.headerEl = this.querySelector("#header") as HTMLDivElement;
    this.viewportEl = this.querySelector("#viewport") as HTMLDivElement;
    this.rowsEl = this.querySelector("#rows") as HTMLDivElement;
    this.createStyleSheet(scope);

    // Pointer events (not mouse) so a touch or pen drag resizes too, and so that
    // `setPointerCapture` keeps the drag attached to the handle once the pointer
    // leaves it — without capture, dragging fast past the header edge drops the
    // move events on whatever is underneath.
    this.headerEl.addEventListener("pointerdown", this.handleResizeStart);
    this.headerEl.addEventListener("pointermove", this.handleResizeMove);
    this.headerEl.addEventListener("pointerup", this.handleResizeEnd);
    this.headerEl.addEventListener("pointercancel", this.handleResizeEnd);
    this.headerEl.addEventListener("dblclick", this.handleResizeDoubleClick);
    (this.querySelector("#reset-columns") as HTMLButtonElement).addEventListener("click", () => {
      this.setColumnWidths(DEFAULT_COLUMN_WIDTHS);
      this.emitColumnWidths();
    });

    this.viewportEl.addEventListener("scroll", () => {
      this.renderVisibleRows();
      // Fired on every scroll regardless of source (user drag or this
      // component's own `scrollToBottom`/anchor-restore assignments) — callers
      // that layer a follow/pause policy on top (`looqlog-live-tail.ts`) only ever
      // react to whether the result reads as "at the bottom", which a
      // programmatic scroll-to-bottom trivially satisfies too.
      this.dispatchEvent(new CustomEvent<number>("viewport-scroll", { detail: this.distanceFromBottom() }));
    });
    this.rowsEl.addEventListener("click", (event) => this.handleRowClick(event));
    // The viewport's height now comes from the layout, not from a fixed rule
    // (`app-shell` spec, "The table uses the height it is given"): a taller window
    // has to render more rows, and nothing else would trigger that re-render in
    // file mode, where no live tick is running.
    window.addEventListener("resize", this.handleResize);
  }

  disconnectedCallback(): void {
    window.removeEventListener("resize", this.handleResize);
    if (this.fetchFrame !== null) {
      cancelAnimationFrame(this.fetchFrame);
      this.fetchFrame = null;
    }
    // The three rules live in a stylesheet this component does not own, so it has
    // to take them back out again: a remount inserts a fresh set under a new
    // scope, and without this they would accumulate in `/assets/index.css` for the
    // lifetime of the page.
    this.deleteGeneratedRules();
  }

  private readonly handleResize = (): void => {
    this.renderVisibleRows();
  };

  // ---- generated stylesheet (design D2/D3/D4) -------------------------------

  /** Inserts the three rules this component mutates into the application's *own*
   * stylesheet — the one `index.html` already loads through
   * `<link rel="stylesheet" href="/assets/index.css">`, and which `style-src
   * 'self'` therefore permits. No element is created, so there is nothing for the
   * policy to gate (design D2).
   *
   * The mechanism this replaced appended an empty `<style>` element, on the
   * premise that empty content is nothing for a CSP to block. That premise is
   * false: `style-src` gates the *insertion of the element*, and Chrome refused it
   * under the served strict policy, naming the SHA-256 of the empty string. Not
   * `adoptedStyleSheets` either — Safari shipped those in 16.4 while PRD §11
   * targets Safari 16+, so 16.0–16.3 would get a table that never positions a row.
   *
   * The rules are appended at the end of the sheet, never inserted at an index, so
   * they cannot disturb the cascade of the rules `style.css` authored. */
  private createStyleSheet(scope: string): void {
    const sheet = findAppStyleSheet();
    if (!sheet) {
      // Fails LOUD, deliberately (task 5.8). The soft `if (!sheet) return` that
      // used to be here is precisely what turned a CSP-blocked stylesheet into a
      // table that quietly showed 36 rows of 50,000 and scrolled nowhere —
      // CLAUDE.md's silent-failure list exists for this. A missing stylesheet is
      // not a survivable state for a virtual scroller: without the rules, every
      // row sits at offset 0 and the viewport has no scrollable height.
      this.reportFatal(
        `The table's stylesheet (${APP_STYLESHEET_PATH}) could not be found, so rows cannot be ` +
          `positioned. The table is disabled rather than showing part of the log as if it were all of it.`,
      );
      throw new Error(`looqlog-entry-table: ${APP_STYLESHEET_PATH} is not in document.styleSheets`);
    }
    this.sheet = sheet;
    const at = `looqlog-entry-table[data-looqlog-table="${scope}"]`;
    this.columnsRule = insertRule(sheet, `${at} .entry-table-wrap`);
    this.spacerRule = insertRule(sheet, `${at} .entry-table-spacer`);
    this.rowsRule = insertRule(sheet, `${at} .entry-table-rows`);
    this.applyColumnWidths();
  }

  private deleteGeneratedRules(): void {
    const sheet = this.sheet;
    if (!sheet) {
      return;
    }
    // By identity, not by remembered index: another instance may have appended its
    // own rules after these, which would have shifted them.
    for (const rule of [this.rowsRule, this.spacerRule, this.columnsRule]) {
      const index = Array.from(sheet.cssRules).indexOf(rule);
      if (index >= 0) {
        sheet.deleteRule(index);
      }
    }
    this.sheet = null;
  }

  /** An unrecoverable failure in the table itself, reported the way the app shell
   * reports one (`error-states` spec, "Errors are reachable, not transient"): a
   * persistent `.error-banner`, in place of the table rather than above a broken
   * one, so it cannot be read as "the log is empty". */
  private reportFatal(message: string): void {
    this.innerHTML = `<div class="entry-table-wrap"><p class="error-banner"></p></div>`;
    (this.querySelector(".error-banner") as HTMLParagraphElement).textContent = message;
  }

  // ---- resizable columns (`entry-table` spec, "Columns can be resized") ------

  /** The current widths, in `rem`. */
  getColumnWidths(): ColumnWidths {
    return this.columnWidths;
  }

  /** Applies widths from outside the component — the URL hash, via the shell.
   * Values are clamped here as well as at parse time, so no caller can install a
   * width the user could not drag back (design D5). */
  setColumnWidths(widths: ColumnWidths): void {
    this.columnWidths = {
      ordinal: clampColumnWidth("ordinal", widths.ordinal),
      timestamp: clampColumnWidth("timestamp", widths.timestamp),
      level: clampColumnWidth("level", widths.level),
    };
    this.applyColumnWidths();
  }

  /** Writes the three widths onto the one rule that defines the grid template's
   * custom properties (design D1). Every row is its own grid, so all of them have
   * to read the same variables or the columns would not line up; the message
   * column is `minmax(0, 1fr)` in `style.css` and absorbs the remainder, which is
   * what keeps the row exactly as wide as the viewport with no horizontal
   * scrollbar to reconcile. */
  private applyColumnWidths(): void {
    const rule = this.columnsRule;
    rule.style.setProperty("--col-ordinal", `${this.columnWidths.ordinal}rem`);
    rule.style.setProperty("--col-timestamp", `${this.columnWidths.timestamp}rem`);
    rule.style.setProperty("--col-level", `${this.columnWidths.level}rem`);
    // The reset-all control only exists once there is something to reset — at the
    // defaults it would be a button that does nothing (design D5's "a control
    // restores all of them" is about the trap of having no way back, not about a
    // permanent chrome element).
    const resetBtn = this.querySelector("#reset-columns") as HTMLButtonElement | null;
    if (resetBtn) {
      resetBtn.hidden = RESIZABLE_COLUMNS.every((c) => this.columnWidths[c] === DEFAULT_COLUMN_WIDTHS[c]);
    }
  }

  private emitColumnWidths(): void {
    this.dispatchEvent(new CustomEvent<ColumnWidths>("columns-change", { detail: this.columnWidths }));
  }

  private readonly handleResizeStart = (event: PointerEvent): void => {
    const handle = (event.target as HTMLElement | null)?.closest<HTMLElement>(".col-resizer");
    const column = handle?.dataset.column as ResizableColumn | undefined;
    if (!handle || !column || event.button !== 0) {
      return;
    }
    // Stops the browser from starting a text selection over the header while the
    // pointer sweeps across the table.
    event.preventDefault();
    handle.setPointerCapture(event.pointerId);
    this.drag = {
      column,
      pointerId: event.pointerId,
      handle,
      startX: event.clientX,
      startWidthRem: this.columnWidths[column],
      // Widths are in `rem` but the pointer moves in px, so the conversion is read
      // once per drag rather than per move — the root font size cannot change
      // mid-drag in any way that matters, and reading it per move would force a
      // style recalculation on every pointer event.
      remPx: rootFontSizePx(),
    };
  };

  private readonly handleResizeMove = (event: PointerEvent): void => {
    const drag = this.drag;
    if (!drag || event.pointerId !== drag.pointerId) {
      return;
    }
    const deltaRem = (event.clientX - drag.startX) / drag.remPx;
    // Clamped, never refused (design D5): a handle that stops moving when the
    // pointer keeps going reads as broken, so the width simply stops at the floor
    // (or the ceiling) while the pointer is free to come back.
    const next = clampColumnWidth(drag.column, drag.startWidthRem + deltaRem);
    if (next === this.columnWidths[drag.column]) {
      return;
    }
    this.columnWidths = { ...this.columnWidths, [drag.column]: next };
    this.applyColumnWidths();
    // Emitted per move, not per drag-end: the shell's hash writer debounces
    // (`url-hash.ts`'s `HashWriter`), so this costs one scheduled timer rather than
    // a history entry per pointer move, and a link copied mid-drag is still right.
    this.emitColumnWidths();
  };

  private readonly handleResizeEnd = (event: PointerEvent): void => {
    const drag = this.drag;
    if (!drag || event.pointerId !== drag.pointerId) {
      return;
    }
    if (drag.handle.hasPointerCapture(event.pointerId)) {
      drag.handle.releasePointerCapture(event.pointerId);
    }
    this.drag = null;
  };

  /** Double-clicking a handle restores the column to its left (`entry-table` spec,
   * "Resetting one column"). */
  private readonly handleResizeDoubleClick = (event: MouseEvent): void => {
    const handle = (event.target as HTMLElement | null)?.closest<HTMLElement>(".col-resizer");
    const column = handle?.dataset.column as ResizableColumn | undefined;
    if (!column) {
      return;
    }
    event.preventDefault();
    this.columnWidths = { ...this.columnWidths, [column]: DEFAULT_COLUMN_WIDTHS[column] };
    this.applyColumnWidths();
    this.emitColumnWidths();
  };

  /** The selected entry's identity (D6: an ordinal, not a row index), or `null`.
   * The detail pane is a sibling in the workspace, not a child of this component,
   * so selection leaves here as an event and comes back as nothing. */
  private emitSelection(): void {
    this.dispatchEvent(new CustomEvent<number | null>("selection-change", { detail: this.selectedOrdinal }));
  }

  /** A new source (or none). Resets scroll and selection. */
  setSource(session: ViewSession | null): void {
    this.session = session;
    this.selectedOrdinal = null;
    this.evictionBannerVisible = false;
    this.evictionEl.hidden = true;
    this.reload();
    this.emitSelection();
  }

  /** The view changed (filters, search, range, or a re-parse): rows are re-fetched
   * from the top. Scroll from the old view has no meaning in the new one. */
  reload(): void {
    this.cache.clear();
    this.viewportEl.scrollTop = 0;
    this.renderSummary();
    this.renderVisibleRows();
    void this.fetchAround(0);
  }

  /** Called after live growth/eviction (the caller throttles the call rate). Keeps
   * the row the user was looking at by identity (`ordinal`), not by scroll offset:
   * growth at the tail leaves a paused view untouched, and front-eviction moves it to
   * where that entry is now — or to the nearest survivor, with the banner saying so. */
  refresh(): void {
    const session = this.session;
    if (!session) {
      return;
    }
    this.renderSummary();
    const scrollTop = this.viewportEl.scrollTop;
    const topIdx = Math.floor(scrollTop / ROW_HEIGHT);
    const anchor = this.cache.get(topIdx);
    if (!anchor || topIdx === 0) {
      this.cache.clear();
      this.renderVisibleRows();
      void this.fetchAround(topIdx);
      return;
    }
    const offsetPx = scrollTop - topIdx * ROW_HEIGHT;
    const limit = this.screenRows() * 3;
    const seq = ++this.fetchSeq;
    void session
      .window(0, limit, anchor.ordinal)
      .then((reply) => {
        if (!reply || seq !== this.fetchSeq) {
          return;
        }
        const found = reply.rows[0]?.ordinal === anchor.ordinal;
        if (!found && (reply.rows[0]?.ordinal ?? Infinity) > anchor.ordinal && reply.offset === 0) {
          this.evictionBannerVisible = true;
          this.evictionEl.hidden = false;
        }
        this.storeRows(reply.offset, reply.rows);
        this.viewportEl.scrollTop = reply.offset * ROW_HEIGHT + (found ? offsetPx : 0);
        this.renderVisibleRows();
      })
      .catch(() => undefined);
  }

  /** Total lines evicted from retention so far; shown above the first row. */
  setEvicted(count: number): void {
    this.evictedMarkerEl.hidden = count === 0;
    this.evictedMarkerEl.textContent =
      count === 0
        ? ""
        : `${count} oldest line${count === 1 ? "" : "s"} evicted (--max-lines) — earlier entries are no longer retained.`;
  }

  /** Distance in px from the bottom of the scrollable viewport. Exposed for
   * callers that layer their own autoscroll/follow policy on top of this
   * component's scroll-preserving `refresh()` (`looqlog-live-tail.ts`'s "follow the
   * tail unless the user scrolled away" behaviour, `live-tail-ui` spec). */
  distanceFromBottom(): number {
    const el = this.viewportEl;
    return el.scrollHeight - el.scrollTop - el.clientHeight;
  }

  /** Scrolls to the newest row. Used by a "follow" caller after each `refresh()`;
   * this component itself never auto-follows. */
  scrollToBottom(): void {
    this.viewportEl.scrollTop = this.viewportEl.scrollHeight;
    this.renderVisibleRows();
  }

  private get rowCount(): number {
    return this.session?.counts.rowCount ?? 0;
  }

  private screenRows(): number {
    return Math.ceil((this.viewportEl.clientHeight || ROW_HEIGHT * 20) / ROW_HEIGHT);
  }

  /** Fetches a window two screens either side of `index` — the overscan cache. The
   * browser never holds more rows than that. */
  private async fetchAround(index: number): Promise<void> {
    const session = this.session;
    if (!session) {
      return;
    }
    const screen = this.screenRows();
    const offset = Math.max(0, index - screen * 2);
    const seq = ++this.fetchSeq;
    try {
      const reply = await session.window(offset, screen * 5);
      if (!reply || seq !== this.fetchSeq) {
        return;
      }
      this.storeRows(reply.offset, reply.rows);
      this.renderSummary();
      this.renderVisibleRows();
    } catch {
      // A lost connection is reported by the shell; the rows simply stay as they were.
    }
  }

  private storeRows(offset: number, rows: readonly RowDto[]): void {
    this.cache.clear();
    rows.forEach((row, i) => this.cache.set(offset + i, row));
  }

  private renderSummary(): void {
    const session = this.session;
    if (!session) {
      this.summaryEl.textContent = "";
      return;
    }
    const { total, matched, timestamplessExcluded } = session.counts;
    const filtered = session.hasFilters;
    const range = session.state.range;
    let text: string;
    if (!range) {
      if (total === 0) {
        text = "No entries.";
      } else if (matched === 0 && filtered) {
        // `filtering` spec, "A filter that matches nothing is distinguishable".
        text = `Filters and search exclude all ${total} entries.`;
      } else {
        text = filtered
          ? `Showing ${matched} of ${total} entries matching the active filters.`
          : `Showing all ${matched} of ${total} entries.`;
      }
    } else {
      text = filtered
        ? `Showing ${matched} of ${total} entries matching the active filters, in the selected range.`
        : `Showing ${matched} of ${total} entries in the selected range.`;
      if (timestamplessExcluded > 0) {
        text += ` (${timestamplessExcluded} entr${timestamplessExcluded === 1 ? "y has" : "ies have"} no ` +
          `timestamp and ${timestamplessExcluded === 1 ? "is" : "are"} excluded from any time range.)`;
      }
    }
    this.summaryEl.textContent = text;
  }

  private renderVisibleRows(): void {
    const total = this.rowCount;
    const clientHeight = this.viewportEl.clientHeight || ROW_HEIGHT * 20;
    const scrollTop = this.viewportEl.scrollTop;
    const firstIdx = Math.max(0, Math.floor(scrollTop / ROW_HEIGHT) - OVERSCAN_ROWS);
    const visibleCount = Math.ceil(clientHeight / ROW_HEIGHT) + OVERSCAN_ROWS * 2;
    const lastIdxExclusive = Math.min(total, firstIdx + visibleCount);

    // Two rule mutations, not two `style` attribute writes (design D3/D4).
    this.spacerRule.style.height = `${total * ROW_HEIGHT}px`;
    this.rowsRule.style.transform = `translateY(${firstIdx * ROW_HEIGHT}px)`;

    // Rows are reused, not rebuilt: a row whose content key is unchanged is left
    // untouched, so the node under the pointer survives a live update.
    const count = lastIdxExclusive - firstIdx;
    while (this.rowsEl.children.length > count) {
      this.rowsEl.lastElementChild!.remove();
    }
    while (this.rowsEl.children.length < count) {
      this.rowsEl.appendChild(createRowElement());
    }
    let missing = false;
    for (let j = 0; j < count; j++) {
      const el = this.rowsEl.children[j] as HTMLElement;
      const row = this.cache.get(firstIdx + j);
      if (!row) {
        missing = true;
        if (el.dataset.key !== "loading") {
          el.dataset.key = "loading";
          delete el.dataset.ordinal;
          el.classList.remove("selected", "chain-member");
          el.innerHTML = `<span class="col-ordinal"></span><span class="col-timestamp"></span><span class="col-level"></span><span class="col-message absent">…</span>`;
        }
        continue;
      }
      const selected = row.ordinal === this.selectedOrdinal;
      const ranges = row.matchRanges.map((r) => r.join("-")).join(",");
      const key = `${row.ordinal}|${selected ? 1 : 0}|${ranges}|${row.chainSize}|${row.expanded ? 1 : 0}|${row.chainMember ? 1 : 0}`;
      if (el.dataset.key === key) {
        continue;
      }
      el.dataset.key = key;
      el.dataset.ordinal = String(row.ordinal);
      el.classList.toggle("selected", selected);
      el.classList.toggle("chain-member", row.chainMember);
      el.innerHTML = renderRowCellsHtml(row);
    }
    if (missing && this.session) {
      this.scheduleFetch(Math.floor(scrollTop / ROW_HEIGHT));
    }
  }

  /** One fetch per frame at most while scrolling through rows not yet cached. */
  private scheduleFetch(index: number): void {
    this.pendingFetchIndex = index;
    if (this.fetchFrame !== null) {
      return;
    }
    this.fetchFrame = requestAnimationFrame(() => {
      this.fetchFrame = null;
      void this.fetchAround(this.pendingFetchIndex);
    });
  }

  private handleRowClick(event: Event): void {
    // The group control comes first: clicking it expands the chain, it does not
    // select the row underneath it.
    const toggle = (event.target as HTMLElement).closest<HTMLElement>(".chain-toggle");
    if (toggle) {
      event.stopPropagation();
      void this.toggleChain(Number(toggle.dataset.chain));
      return;
    }
    const rowEl = (event.target as HTMLElement).closest<HTMLElement>("[data-ordinal]");
    if (!rowEl) {
      return;
    }
    const ordinal = Number(rowEl.dataset.ordinal);
    this.selectedOrdinal = this.selectedOrdinal === ordinal ? null : ordinal;
    this.renderVisibleRows(); // to toggle the `.selected` class
    this.emitSelection();
  }

  /** Expands or collapses the chain rooted at `rootOrdinal` on the backend, keeping
   * that root where it was on screen: the rows below it move, and a user who opened
   * a sixty-frame trace should still be looking at its first line. */
  private async toggleChain(rootOrdinal: number): Promise<void> {
    const session = this.session;
    if (!session) {
      return;
    }
    let rootIdx = -1;
    for (const [idx, row] of this.cache) {
      if (row.ordinal === rootOrdinal) {
        rootIdx = idx;
      }
    }
    const offsetPx = this.viewportEl.scrollTop - rootIdx * ROW_HEIGHT;
    try {
      await session.toggleChain(rootOrdinal);
      const seq = ++this.fetchSeq;
      const reply = await session.window(0, this.screenRows() * 3, rootOrdinal);
      if (!reply || seq !== this.fetchSeq) {
        return;
      }
      this.storeRows(reply.offset, reply.rows);
      this.renderVisibleRows();
      if (rootIdx >= 0) {
        this.viewportEl.scrollTop = Math.max(0, reply.offset * ROW_HEIGHT + offsetPx);
      }
      this.renderVisibleRows();
    } catch {
      // Reported by the shell's connection indicator.
    }
  }
}

/** Monotonic per-instance suffix for the generated rules' selectors. */
let tableInstanceSeq = 0;

/** The application stylesheet's path, as `index.html` links it (design D2). */
const APP_STYLESHEET_PATH = "/assets/index.css";
/** A selector `style.css` authors and nothing else does. Used to recognise the
 * same stylesheet under `vite dev`, where Vite injects each imported `.css` as a
 * separate `<style>` element with no `href` instead of serving one bundled file —
 * so a path match alone would find nothing there. */
const APP_STYLESHEET_MARKER = ".entry-row";

/** The stylesheet this component inserts its rules into, or `null` if it is not
 * there. Deliberately never waits for it: the served page loads `index.css` with a
 * `<link>` in `<head>`, and a stylesheet blocks script execution, so by the time
 * any module — and therefore any `connectedCallback` — runs, the sheet is either
 * loaded or has failed. Making this async to "handle" a load ordering that cannot
 * happen would only reintroduce a window in which the rules are missing and the
 * scroller silently renders one screenful. */
function findAppStyleSheet(): CSSStyleSheet | null {
  const sheets = Array.from(document.styleSheets);
  const linked = sheets.find(
    (sheet) => sheet.href !== null && new URL(sheet.href, document.baseURI).pathname === APP_STYLESHEET_PATH,
  );
  return linked ?? sheets.find(hasMarkerRule) ?? null;
}

function hasMarkerRule(sheet: CSSStyleSheet): boolean {
  let rules: CSSRuleList;
  try {
    rules = sheet.cssRules; // throws `SecurityError` on a cross-origin stylesheet
  } catch {
    return false;
  }
  return Array.from(rules).some(
    (rule) => rule instanceof CSSStyleRule && rule.selectorText === APP_STYLESHEET_MARKER,
  );
}

/** Appends an empty rule at the end of the sheet and hands back the `CSSStyleRule`
 * to mutate later. `insertRule` needs a syntactically complete rule, hence the
 * empty block; appending (rather than inserting at an index) is what keeps the
 * authored rules' cascade in `style.css` untouched. */
function insertRule(sheet: CSSStyleSheet, selector: string): CSSStyleRule {
  const index = sheet.insertRule(`${selector} {}`, sheet.cssRules.length);
  return sheet.cssRules[index] as CSSStyleRule;
}

/** px per `rem`, i.e. the root font size. 16 is the spec default and the only
 * sensible answer if the computed value is ever unreadable. */
function rootFontSizePx(): number {
  const raw = parseFloat(getComputedStyle(document.documentElement).fontSize);
  return Number.isFinite(raw) && raw > 0 ? raw : 16;
}

/** A drag handle at a column's right boundary. Header only, never in a data row
 * (`entry-table` spec, "Resizing does not select a row"): a handle inside a row
 * would put a drag target on top of the thing the user clicks to select. */
function resizerHtml(column: ResizableColumn, label: string): string {
  return (
    `<span class="col-resizer" data-column="${column}" role="separator" aria-orientation="vertical" ` +
    `aria-label="Resize the ${label} column" title="Drag to resize the ${label} column; double-click to reset it"></span>`
  );
}

/** An empty row shell, built once and then reused across renders (see
 * `renderVisibleRows`). Only its cells are rewritten, and only when the entry it
 * shows actually changes. The height is no longer written here: it is a constant,
 * so it belongs in `.entry-row` in `style.css` (design D3) — see `ROW_HEIGHT`. */
function createRowElement(): HTMLElement {
  const el = document.createElement("div");
  el.className = "entry-row";
  el.setAttribute("role", "row");
  return el;
}

function renderRowCellsHtml(entry: RowDto): string {
  const chainSize = entry.chainSize;
  const expanded = entry.expanded;
  const timestampHtml = entry.timestamp
    ? `<span title="${escapeHtml(entry.timestamp)}">${escapeHtml(rowTimestamp(entry.timestamp))}</span>`
    // An em dash, not the words "no timestamp": the marker has to fit a column
    // sized for a value, and a phrase that outgrows its cell is what made the
    // level column paint over the message column. `title`/`aria-label` carry the
    // meaning, the same way the level abbreviation does (`entry-table` spec,
    // "Missing values are explicit" asks for an explicit marker, not for words).
    : `<span class="absent" aria-label="no timestamp extracted" title="no timestamp extracted">—</span>`;
  // Visible text is compressed to the level's first letter (all six of
  // TRACE/DEBUG/INFO/WARN/ERROR/FATAL are already unique on that letter); the
  // full word stays available via `aria-label` (assistive tech) and `title`
  // (hover tooltip) so the abbreviation is a visual compression, not an
  // information loss (`entry-table` spec, "Abbreviated level still exposes its
  // full name").
  const levelHtml = entry.level
    ? `<span class="level-badge level-${escapeHtml(entry.level.toLowerCase())}" aria-label="${escapeHtml(entry.level)}" title="${escapeHtml(entry.level)}">${escapeHtml(entry.level[0]!)}</span>`
    : `<span class="absent" aria-label="no level extracted" title="no level extracted">—</span>`;
  const truncated = entry.message.length > MESSAGE_TRUNCATE_CHARS;
  const messageShown = truncated ? `${entry.message.slice(0, MESSAGE_TRUNCATE_CHARS)}…` : entry.message;
  // Highlighting runs on the truncated text (`search` spec: "within the
  // truncated visible text") so ranges line up with what's actually rendered.
  const messageHtml = highlightHtml(messageShown, entry.matchRanges);
  // A chain root carries the control and the line count; a member carries neither and
  // is set apart by the row's `chain-member` class alone (`entry-table` spec).
  const toggleHtml = chainSize > 0 ? chainToggleHtml(entry.ordinal, chainSize, expanded) : "";
  const countHtml =
    chainSize > 0
      ? ` <span class="chain-count">${chainSize} lines</span>`
      : "";
  // The control lives at the head of the message cell, not in the ordinal cell: the
  // ordinal column is sized for its digits, and a control wedged in beside them
  // truncates the line number on exactly the rows a user is most likely to want it.
  return `
        <span class="col-ordinal">${entry.ordinal}</span>
        <span class="col-timestamp">${timestampHtml}</span>
        <span class="col-level">${levelHtml}</span>
        <span class="col-message" title="${truncated ? "truncated — click the row for the full message" : ""}">${toggleHtml}${messageHtml}${countHtml}</span>`;
}

/** The expand/collapse control on a chain root. A real `<button>`, so it is reachable
 * by keyboard and announces its state, and `aria-expanded` is what carries that state
 * rather than the glyph. */
function chainToggleHtml(rootOrdinal: number, chainSize: number, expanded: boolean): string {
  const label = expanded
    ? `Collapse this ${chainSize}-line event`
    : `Expand this ${chainSize}-line event`;
  return (
    `<button type="button" class="chain-toggle" data-chain="${rootOrdinal}" ` +
    `aria-expanded="${expanded}" aria-label="${label}" title="${label}">${expanded ? "▾" : "▸"}</button>`
  );
}

/** Drops the zone suffix a row does not need: `Entry::timestamp` is a
 * `DateTime<Utc>` in `looqlog-core`, so every value the parser emits ends in `+00:00`
 * (or `Z`) and the column header already says UTC — six characters repeated on
 * every row, taking width from the message column. Only that exactly-redundant
 * suffix is stripped; any other offset stays visible rather than being silently
 * dropped, and the full RFC 3339 string is kept as the cell's `title` and in the
 * detail pane. */
function rowTimestamp(timestamp: string): string {
  return timestamp.replace(/(\+00:00|Z)$/, "");
}

function escapeHtml(input: string): string {
  return input
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;");
}

/** Escapes `text` and wraps every range the backend reported (`search` spec, "Match
 * highlighting") in a styled `<span class="hl">` — a plain span rather than `<mark>`,
 * which measured several times slower to write via `innerHTML` (`docs/devlog.md`).
 * Ranges past the end of `text` (it may be truncated) are clipped. Escaping happens
 * per segment so a `<`/`&` inside a match cannot break out of its span. */
export function highlightHtml(text: string, ranges: ReadonlyArray<readonly [number, number]>): string {
  const parts: string[] = [];
  let cursor = 0;
  for (const [rawStart, rawEnd] of ranges) {
    const start = Math.max(cursor, Math.min(rawStart, text.length));
    const end = Math.min(rawEnd, text.length);
    if (end <= start) {
      continue;
    }
    parts.push(escapeHtml(text.slice(cursor, start)));
    parts.push(`<span class="hl">${escapeHtml(text.slice(start, end))}</span>`);
    cursor = end;
  }
  parts.push(escapeHtml(text.slice(cursor)));
  return parts.join("");
}

customElements.define("looqlog-entry-table", LooqlogEntryTable);
