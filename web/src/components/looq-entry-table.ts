// Virtual-scrolled entry table (`entry-table` spec, design.md D2/D6/D7). Renders
// only the rows in and near the viewport regardless of dataset size (fixed row
// height, D6), reflects the shell-owned active range (D7), and survives live
// growth/eviction by anchoring the visible window to a stable row identity
// (`EntryDto.ordinal`, D2) across each `refresh()` rather than to a raw scroll
// offset that eviction would silently invalidate.
//
// Replaces the provisional table from `browser-app-shell` (task 3.8): that one
// rendered every entry into the DOM up to a 500-row cap; this one is bounded by
// the viewport, not the dataset, at any size.
//
// The detail view is no longer part of this component (`frontend-three-pane-layout`
// D6): the table holds the selection and emits it as `selection-change`, and the
// workspace's right pane (`looq-entry-detail`) renders it — so inspecting an entry
// cannot reflow the rows around it.

import {
  clampColumnWidth,
  DEFAULT_COLUMN_WIDTHS,
  RESIZABLE_COLUMNS,
  type ColumnWidths,
  type ResizableColumn,
} from "../column-widths";
import type { EntryIndex } from "../entry-index";
import { findMatchRanges, type CompiledQuery } from "../predicate";
import type { TimeRange } from "../time-range";
import type { EntryDto } from "../wasm-types";

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

export class LooqEntryTable extends HTMLElement {
  private index: EntryIndex | null = null;
  private activeRange: TimeRange | null = null;
  private displayed: EntryDto[] = [];
  /** `displayed` minus the members of collapsed chains — the rows virtual scrolling
   * actually addresses (`entry-table` spec, "A continuation chain renders as one
   * collapsible group"). Every row is still one `ROW_HEIGHT`, so the arithmetic that
   * makes this a virtual scroller rather than a measurement pass is untouched. */
  private rows: EntryDto[] = [];
  /** Chain root ordinal → total lines in the chain (root included), for the roots
   * present in `displayed`. Absent for an entry that roots no visible chain. */
  private chainSizes = new Map<number, number>();
  /** Member ordinal → its root, for members whose root is also in `displayed`. A
   * member without one — a chain cut by a time range, or orphaned by live-tail
   * eviction — renders as an ordinary standalone row. */
  private chainOf = new Map<number, number>();
  /** Roots the user has expanded. Chains start collapsed, so a file full of stack
   * traces opens showing events rather than frames. Held on the component, so it
   * survives the row recycling in `renderVisibleRows` and every `refresh()`. */
  private expandedChains = new Set<number>();
  private selectedOrdinal: number | null = null;
  private evictionBannerVisible = false;
  /** For highlighting only (`search` spec, "Match highlighting") — filtering
   * itself already happened inside `EntryIndex` via its predicate (D2); this is
   * never consulted to decide whether a row is shown, only how it's drawn. */
  private compiledQuery: CompiledQuery = { kind: "none" };

  private summaryEl!: HTMLParagraphElement;
  private evictionEl!: HTMLParagraphElement;
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
    // their own table (`looq-app` / `looq-live-tail`), and a shared selector would
    // have one instance's scroll offset positioning the other's rows.
    const scope = `t${++tableInstanceSeq}`;
    this.dataset.looqTable = scope;

    this.innerHTML = `
      <div class="entry-table-wrap">
        <div class="entry-table-head">
          <p class="entry-table-summary" id="table-summary"></p>
          <button type="button" class="col-reset-all" id="reset-columns" hidden>Reset columns</button>
        </div>
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
      // that layer a follow/pause policy on top (`looq-live-tail.ts`) only ever
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
      throw new Error(`looq-entry-table: ${APP_STYLESHEET_PATH} is not in document.styleSheets`);
    }
    this.sheet = sheet;
    const at = `looq-entry-table[data-looq-table="${scope}"]`;
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

  /** A new dataset (new file, or a fresh stream). Resets scroll and selection. */
  setIndex(index: EntryIndex): void {
    this.index = index;
    this.selectedOrdinal = null;
    this.expandedChains.clear(); // a new file's ordinals mean nothing to the old set
    this.recomputeDisplayed();
    this.viewportEl.scrollTop = 0;
    this.renderVisibleRows();
    this.emitSelection();
  }

  /** The shell narrowed or cleared the active range (D7). Resets scroll — this is
   * a new view, not a continuation of the old one. */
  setActiveRange(range: TimeRange | null): void {
    this.activeRange = range;
    this.recomputeDisplayed();
    this.viewportEl.scrollTop = 0;
    this.renderVisibleRows();
  }

  /** The compiled search query, for match highlighting only (`search` spec) — set
   * independently of filtering, which already happened via `EntryIndex`'s
   * predicate before `refreshFilters`/`refresh` is called. */
  setQuery(compiled: CompiledQuery): void {
    this.compiledQuery = compiled;
    this.expandChainsMatchingQuery();
    this.renderVisibleRows();
  }

  /** The shell's predicate (chips/search, `filtering-and-search`) changed. Same
   * "new view" semantics as `setActiveRange`: this reflects `EntryIndex` having
   * already been re-filtered (`EntryIndex.setPredicate`), so the scroll position
   * from before the filter change has no guaranteed meaning here. */
  refreshFilters(): void {
    this.recomputeDisplayed();
    this.viewportEl.scrollTop = 0;
    this.renderVisibleRows();
  }

  /** Called after live append/eviction (design.md D8-adjacent: the caller
   * throttles the call rate). Preserves the row the user was looking at by
   * identity (`ordinal`), not by raw scroll offset, so growth at the tail leaves
   * a paused user's view untouched and front-eviction adjusts cleanly instead of
   * silently showing the wrong rows. */
  refresh(): void {
    if (!this.index) {
      return;
    }
    const oldRows = this.rows;
    const scrollTop = this.viewportEl.scrollTop;
    const topIdx = Math.floor(scrollTop / ROW_HEIGHT);
    const anchor = oldRows[topIdx];
    const anchorOffsetPx = scrollTop - topIdx * ROW_HEIGHT;

    this.recomputeDisplayed();

    if (anchor) {
      const newIdx = lowerBoundOrdinal(this.rows, anchor.ordinal);
      const found = this.rows[newIdx]?.ordinal === anchor.ordinal;
      if (!found && this.rows.length > 0 && (this.rows[0]?.ordinal ?? 0) > anchor.ordinal) {
        // The row the user was anchored to (or rows above it) were evicted out
        // from under them — jumping to the nearest survivor is the "adjusts
        // cleanly" half of the requirement; the banner is the "indicates lost
        // history" half.
        this.evictionBannerVisible = true;
        this.evictionEl.hidden = false;
      }
      const clampedIdx = Math.min(newIdx, Math.max(0, this.rows.length - 1));
      this.viewportEl.scrollTop = clampedIdx * ROW_HEIGHT + (found ? anchorOffsetPx : 0);
    }

    if (this.evictionBannerVisible) {
      this.evictionEl.hidden = false;
    }

    this.renderVisibleRows();
  }

  /** Distance in px from the bottom of the scrollable viewport. Exposed for
   * callers that layer their own autoscroll/follow policy on top of this
   * component's scroll-preserving `refresh()` (`looq-live-tail.ts`'s "follow the
   * tail unless the user scrolled away" behaviour, `live-tail-ui` spec). */
  distanceFromBottom(): number {
    const el = this.viewportEl;
    return el.scrollHeight - el.scrollTop - el.clientHeight;
  }

  /** Scrolls to the newest row. Used by a "follow" caller after each `refresh()`;
   * this component itself never auto-follows (design.md D6/`entry-table` spec —
   * "without losing the user's scroll position" is the default, not following). */
  scrollToBottom(): void {
    this.viewportEl.scrollTop = this.viewportEl.scrollHeight;
    this.renderVisibleRows();
  }

  private recomputeDisplayed(): void {
    if (!this.index) {
      this.displayed = [];
    } else if (this.activeRange) {
      this.displayed = this.index.queryRange(this.activeRange.startMs, this.activeRange.endMs).entries;
    } else {
      this.displayed = this.index.entriesInInputOrder();
    }
    this.recomputeRows();
    this.renderSummary();
  }

  /** Groups `displayed` into chains and drops the members of collapsed ones. Derived
   * from `displayed` rather than from the index on purpose: a member whose root the
   * active range or eviction removed is then, by construction, a row with no group —
   * there is no lookup left to fail. */
  private recomputeRows(): void {
    this.chainSizes.clear();
    this.chainOf.clear();
    const present = new Set(this.displayed.map((e) => e.ordinal));
    for (const entry of this.displayed) {
      const root = entry.continuationOf;
      if (root === null || !present.has(root)) {
        continue;
      }
      this.chainOf.set(entry.ordinal, root);
      this.chainSizes.set(root, (this.chainSizes.get(root) ?? 1) + 1);
    }
    // Expansion state is kept for chains that are no longer visible: a filter change
    // that hides a chain and a later one that brings it back should not silently
    // re-collapse it.
    this.rows = this.displayed.filter((entry) => {
      const root = this.chainOf.get(entry.ordinal);
      return root === undefined || this.expandedChains.has(root);
    });
  }

  private renderSummary(): void {
    if (!this.index) {
      this.summaryEl.textContent = "";
      return;
    }
    const total = this.index.totalCount;
    const shown = this.displayed.length;
    const filtered = this.index.hasActivePredicate;
    let text: string;
    if (!this.activeRange) {
      if (total === 0) {
        text = "No entries.";
      } else if (shown === 0 && filtered) {
        // `filtering` spec, "A filter that matches nothing is distinguishable" —
        // reads differently from "No entries." above, which means an empty file.
        text = `Filters and search exclude all ${total} entries.`;
      } else {
        text = filtered
          ? `Showing ${shown} of ${total} entries matching the active filters.`
          : `Showing all ${shown} of ${total} entries.`;
      }
    } else {
      text = filtered
        ? `Showing ${shown} of ${total} entries matching the active filters, in the selected range.`
        : `Showing ${shown} of ${total} entries in the selected range.`;
      const timestamplessTotal = this.index.timestamplessCount;
      if (timestamplessTotal > 0) {
        text += ` (${timestamplessTotal} entr${timestamplessTotal === 1 ? "y has" : "ies have"} no ` +
          `timestamp and ${timestamplessTotal === 1 ? "is" : "are"} excluded from any time range.)`;
      }
    }
    this.summaryEl.textContent = text;
  }

  private renderVisibleRows(): void {
    const total = this.rows.length;
    const clientHeight = this.viewportEl.clientHeight || ROW_HEIGHT * 20;
    const scrollTop = this.viewportEl.scrollTop;
    const firstIdx = Math.max(0, Math.floor(scrollTop / ROW_HEIGHT) - OVERSCAN_ROWS);
    const visibleCount = Math.ceil(clientHeight / ROW_HEIGHT) + OVERSCAN_ROWS * 2;
    const lastIdxExclusive = Math.min(total, firstIdx + visibleCount);

    // Two rule mutations, not two `style` attribute writes (design D3/D4): the
    // existing `CSSStyleDeclaration` is assigned in place, which is what keeps this
    // off the "re-parse the whole stylesheet every frame" path `replaceSync`/
    // `textContent =` would put it on. Measured against the recorded 50k baseline —
    // see the change's tasks.md 1.1/5.1.
    this.spacerRule.style.height = `${total * ROW_HEIGHT}px`;
    this.rowsRule.style.transform = `translateY(${firstIdx * ROW_HEIGHT}px)`;

    // Rows are reused, not rebuilt. Replacing `rowsEl.innerHTML` on every render
    // detached whatever the user was pressing on: under a live stream the row went
    // away between mousedown and mouseup, the browser never synthesised a `click`,
    // and selecting an entry silently did nothing (the same failure the filter rail
    // had). A row whose content key is unchanged is left completely untouched, so
    // the node under the pointer survives the update.
    const count = lastIdxExclusive - firstIdx;
    while (this.rowsEl.children.length > count) {
      this.rowsEl.lastElementChild!.remove();
    }
    while (this.rowsEl.children.length < count) {
      this.rowsEl.appendChild(createRowElement());
    }
    const queryKey = compiledQueryKey(this.compiledQuery);
    for (let j = 0; j < count; j++) {
      const entry = this.rows[firstIdx + j]!; // firstIdx + j < lastIdxExclusive <= total
      const el = this.rowsEl.children[j] as HTMLElement;
      const selected = entry.ordinal === this.selectedOrdinal;
      const chainSize = this.chainSizes.get(entry.ordinal) ?? 0;
      const expanded = chainSize > 0 && this.expandedChains.has(entry.ordinal);
      const isMember = this.chainOf.has(entry.ordinal);
      const key = `${entry.ordinal}|${selected ? 1 : 0}|${queryKey}|${chainSize}|${expanded ? 1 : 0}|${isMember ? 1 : 0}`;
      if (el.dataset.key === key) {
        continue;
      }
      el.dataset.key = key;
      el.dataset.ordinal = String(entry.ordinal);
      el.classList.toggle("selected", selected);
      el.classList.toggle("chain-member", isMember);
      el.innerHTML = renderRowCellsHtml(entry, this.compiledQuery, chainSize, expanded);
    }
  }

  private handleRowClick(event: Event): void {
    // The group control comes first: clicking it expands the chain, it does not
    // select the row underneath it.
    const toggle = (event.target as HTMLElement).closest<HTMLElement>(".chain-toggle");
    if (toggle) {
      event.stopPropagation();
      this.toggleChain(Number(toggle.dataset.chain));
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

  /** Expands or collapses the chain rooted at `rootOrdinal`, keeping the scroll
   * anchored on that root: the rows below it move, and a user who opened a
   * sixty-frame trace should still be looking at its first line. */
  private toggleChain(rootOrdinal: number): void {
    if (!this.chainSizes.has(rootOrdinal)) {
      return;
    }
    if (this.expandedChains.has(rootOrdinal)) {
      this.expandedChains.delete(rootOrdinal);
    } else {
      this.expandedChains.add(rootOrdinal);
    }
    const offsetPx = this.viewportEl.scrollTop - this.rows.findIndex((e) => e.ordinal === rootOrdinal) * ROW_HEIGHT;
    this.recomputeRows();
    const at = this.rows.findIndex((e) => e.ordinal === rootOrdinal);
    if (at >= 0) {
      this.viewportEl.scrollTop = Math.max(0, at * ROW_HEIGHT + offsetPx);
    }
    this.renderVisibleRows();
  }

  /** Expands every visible chain with a member the query matched, so the highlighted
   * frame is actually on screen (`entry-table` spec, "Search expands the chain it
   * matched"). Collapsing again is the user's call — this never re-collapses. */
  private expandChainsMatchingQuery(): void {
    if (this.compiledQuery.kind === "none") {
      return;
    }
    for (const entry of this.displayed) {
      const root = this.chainOf.get(entry.ordinal);
      if (root === undefined || this.expandedChains.has(root)) {
        continue;
      }
      if (findMatchRanges(entry.message, this.compiledQuery).length > 0) {
        this.expandedChains.add(root);
      }
    }
    this.recomputeRows();
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

/** Identifies what a row's cells were rendered from: the entry, whether it is
 * selected, and the query the highlighting came from. Equal keys mean the rendered
 * output would be identical, so the row can be left alone. */
function compiledQueryKey(compiled: CompiledQuery): string {
  switch (compiled.kind) {
    case "none":
      return "none";
    case "substring":
      return `sub:${compiled.needle}`;
    case "regex":
      return `re:${compiled.source}`;
  }
}

function renderRowCellsHtml(
  entry: EntryDto,
  compiled: CompiledQuery,
  chainSize: number,
  expanded: boolean,
): string {
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
  const messageHtml = highlightHtml(messageShown, compiled);
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

/** First index in `displayed` (sorted ascending by ordinal — input order, D2)
 * whose ordinal is >= `ordinal`. */
function lowerBoundOrdinal(displayed: readonly EntryDto[], ordinal: number): number {
  let lo = 0;
  let hi = displayed.length;
  while (lo < hi) {
    const mid = (lo + hi) >>> 1;
    if (displayed[mid]!.ordinal < ordinal) {
      lo = mid + 1;
    } else {
      hi = mid;
    }
  }
  return lo;
}

/** Drops the zone suffix a row does not need: `Entry::timestamp` is a
 * `DateTime<Utc>` in `looq-core`, so every value the parser emits ends in `+00:00`
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

/** Escapes `text` and wraps every range `findMatchRanges` reports in a styled
 * `<span class="hl">` (`search` spec, "Match highlighting"). A plain `<span>`
 * rather than the semantically-closer `<mark>` deliberately (task 7.3/7.4,
 * `docs/devlog.md`): measured in a real browser at 10k entries, writing 36
 * `<mark>`-containing rows via `innerHTML` cost ~19ms avg versus ~5ms for the
 * identical content in `<span class="hl">` — `<mark>` alone, isolated from every
 * other variable, was responsible for most of one filter-change's latency budget.
 * Escaping happens per-segment so a `<`/`&` inside a matched range can't break out
 * of the span it's wrapped in. */
function highlightHtml(text: string, compiled: CompiledQuery): string {
  const ranges = findMatchRanges(text, compiled);
  if (ranges.length === 0) {
    return escapeHtml(text);
  }
  const parts: string[] = [];
  let cursor = 0;
  for (const [start, end] of ranges) {
    parts.push(escapeHtml(text.slice(cursor, start)));
    parts.push(`<span class="hl">${escapeHtml(text.slice(start, end))}</span>`);
    cursor = end;
  }
  parts.push(escapeHtml(text.slice(cursor)));
  return parts.join("");
}

customElements.define("looq-entry-table", LooqEntryTable);
