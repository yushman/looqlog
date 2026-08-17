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

import type { EntryIndex } from "../entry-index";
import { findMatchRanges, type CompiledQuery } from "../predicate";
import type { TimeRange } from "../time-range";
import type { EntryDto } from "../wasm-types";

/** Uniform row height in CSS pixels — what makes virtual scrolling arithmetic
 * (index * height) rather than a measurement pass (D6). */
const ROW_HEIGHT = 24;
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
  private selectedOrdinal: number | null = null;
  private evictionBannerVisible = false;
  /** For highlighting only (`search` spec, "Match highlighting") — filtering
   * itself already happened inside `EntryIndex` via its predicate (D2); this is
   * never consulted to decide whether a row is shown, only how it's drawn. */
  private compiledQuery: CompiledQuery = { kind: "none" };

  private summaryEl!: HTMLParagraphElement;
  private evictionEl!: HTMLParagraphElement;
  private viewportEl!: HTMLDivElement;
  private spacerEl!: HTMLDivElement;
  private rowsEl!: HTMLDivElement;

  connectedCallback(): void {
    this.innerHTML = `
      <div class="entry-table-wrap">
        <p class="entry-table-summary" id="table-summary"></p>
        <p class="provisional-note eviction-banner" id="eviction-banner" hidden>
          Earlier entries you were viewing are no longer retained (evicted at
          <code>--max-lines</code>) — jumped to the oldest entry still available.
        </p>
        <div class="entry-table-header" role="row">
          <span class="col-ordinal">#</span>
          <span class="col-timestamp">timestamp (UTC)</span>
          <span class="col-level">level</span>
          <span class="col-message">message</span>
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
    this.viewportEl = this.querySelector("#viewport") as HTMLDivElement;
    this.spacerEl = this.querySelector("#spacer") as HTMLDivElement;
    this.rowsEl = this.querySelector("#rows") as HTMLDivElement;

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
  }

  private readonly handleResize = (): void => {
    this.renderVisibleRows();
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
    const oldDisplayed = this.displayed;
    const scrollTop = this.viewportEl.scrollTop;
    const topIdx = Math.floor(scrollTop / ROW_HEIGHT);
    const anchor = oldDisplayed[topIdx];
    const anchorOffsetPx = scrollTop - topIdx * ROW_HEIGHT;

    this.recomputeDisplayed();

    if (anchor) {
      const newIdx = lowerBoundOrdinal(this.displayed, anchor.ordinal);
      const found = this.displayed[newIdx]?.ordinal === anchor.ordinal;
      if (!found && this.displayed.length > 0 && (this.displayed[0]?.ordinal ?? 0) > anchor.ordinal) {
        // The row the user was anchored to (or rows above it) were evicted out
        // from under them — jumping to the nearest survivor is the "adjusts
        // cleanly" half of the requirement; the banner is the "indicates lost
        // history" half.
        this.evictionBannerVisible = true;
        this.evictionEl.hidden = false;
      }
      const clampedIdx = Math.min(newIdx, Math.max(0, this.displayed.length - 1));
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
    this.renderSummary();
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
    const total = this.displayed.length;
    const clientHeight = this.viewportEl.clientHeight || ROW_HEIGHT * 20;
    const scrollTop = this.viewportEl.scrollTop;
    const firstIdx = Math.max(0, Math.floor(scrollTop / ROW_HEIGHT) - OVERSCAN_ROWS);
    const visibleCount = Math.ceil(clientHeight / ROW_HEIGHT) + OVERSCAN_ROWS * 2;
    const lastIdxExclusive = Math.min(total, firstIdx + visibleCount);

    this.spacerEl.style.height = `${total * ROW_HEIGHT}px`;
    this.rowsEl.style.transform = `translateY(${firstIdx * ROW_HEIGHT}px)`;

    const html: string[] = [];
    for (let i = firstIdx; i < lastIdxExclusive; i++) {
      html.push(this.renderRowHtml(this.displayed[i]!)); // i < lastIdxExclusive <= total
    }
    this.rowsEl.innerHTML = html.join("");
  }

  private renderRowHtml(entry: EntryDto): string {
    const timestampHtml = entry.timestamp
      ? `<span title="${escapeHtml(entry.timestamp)}">${escapeHtml(rowTimestamp(entry.timestamp))}</span>`
      : `<span class="absent" title="no timestamp extracted">no timestamp</span>`;
    // Visible text is compressed to the level's first letter (all six of
    // TRACE/DEBUG/INFO/WARN/ERROR/FATAL are already unique on that letter); the
    // full word stays available via `aria-label` (assistive tech) and `title`
    // (hover tooltip) so the abbreviation is a visual compression, not an
    // information loss (`entry-table` spec, "Abbreviated level still exposes its
    // full name").
    const levelHtml = entry.level
      ? `<span class="level-badge level-${escapeHtml(entry.level.toLowerCase())}" aria-label="${escapeHtml(entry.level)}" title="${escapeHtml(entry.level)}">${escapeHtml(entry.level[0]!)}</span>`
      : `<span class="absent" title="no level extracted">no level</span>`;
    const truncated = entry.message.length > MESSAGE_TRUNCATE_CHARS;
    const messageShown = truncated ? `${entry.message.slice(0, MESSAGE_TRUNCATE_CHARS)}…` : entry.message;
    const selected = entry.ordinal === this.selectedOrdinal;
    // Highlighting runs on the truncated text (`search` spec: "within the
    // truncated visible text") so ranges line up with what's actually rendered.
    const messageHtml = highlightHtml(messageShown, this.compiledQuery);
    return `
      <div class="entry-row${selected ? " selected" : ""}" role="row" data-ordinal="${entry.ordinal}"
           style="height:${ROW_HEIGHT}px">
        <span class="col-ordinal">${entry.ordinal}</span>
        <span class="col-timestamp">${timestampHtml}</span>
        <span class="col-level">${levelHtml}</span>
        <span class="col-message" title="${truncated ? "truncated — click the row for the full message" : ""}">${messageHtml}</span>
      </div>`;
  }

  private handleRowClick(event: Event): void {
    const rowEl = (event.target as HTMLElement).closest<HTMLElement>("[data-ordinal]");
    if (!rowEl) {
      return;
    }
    const ordinal = Number(rowEl.dataset.ordinal);
    this.selectedOrdinal = this.selectedOrdinal === ordinal ? null : ordinal;
    this.renderVisibleRows(); // to toggle the `.selected` class
    this.emitSelection();
  }
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
