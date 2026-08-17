// The three-pane workspace layout (`app-shell` spec, "Three-pane workspace layout",
// design.md D1/D2). Defined exactly once and mounted by both shells — `looq-app.ts`
// (file mode) and `looq-live-tail.ts` (stream mode) — so a layout change lands in
// both at the same time instead of drifting between two hand-written page stacks.
//
// Light DOM, like every other component here (no shadow root, so `style.css` keeps
// applying), which means "slots" are plain containers a shell fills rather than
// `<slot>` elements. `pane(name)` is the only way in: shells own their content, the
// workspace owns where it goes.

/** The mountable regions of the workspace. */
export type WorkspacePane =
  /** Mode-specific header line: status (file mode), connection + rate (stream mode). */
  | "topbar"
  /** Full-width notices that must not be collapsed away: errors, confirms, gap/eviction notes. */
  | "messages"
  /** The full-width collapsible timeline row. */
  | "timeline"
  /** Left rail, filter sections. */
  | "rail"
  /** Left rail, secondary surfaces (file open, detection, diagnostics, privacy, link). */
  | "rail-secondary"
  /** Above the table: mode-specific controls (stream mode's autoscroll resume). */
  | "table-toolbar"
  /** The entry table itself. */
  | "table"
  /** Right pane: the selected entry's detail. */
  | "detail";

/** D8: below this width the grid collapses to one column and the rail's sections
 * start collapsed — a fallback for a small window, not a mobile design. Kept next
 * to the media query it mirrors in `style.css`; both must move together. */
export const NARROW_LAYOUT_QUERY = "(max-width: 1100px)";

export function isNarrowLayout(): boolean {
  return window.matchMedia(NARROW_LAYOUT_QUERY).matches;
}

export class LooqWorkspace extends HTMLElement {
  connectedCallback(): void {
    if (this.querySelector(".workspace")) {
      return; // already mounted (a re-connect must not discard what a shell put in the panes)
    }
    this.innerHTML = `
      <div class="workspace">
        <header class="ws-topbar">
          <div class="ws-topbar-line" data-pane="topbar"></div>
          <div class="ws-messages" data-pane="messages"></div>
        </header>
        <details class="ws-timeline" id="ws-timeline" open>
          <summary class="ws-timeline-summary">Timeline</summary>
          <div class="ws-timeline-body" data-pane="timeline"></div>
        </details>
        <aside class="ws-rail" aria-label="Filters">
          <div class="ws-rail-main" data-pane="rail"></div>
          <div class="ws-rail-secondary" data-pane="rail-secondary"></div>
        </aside>
        <section class="ws-center" aria-label="Log entries">
          <div class="ws-table-toolbar" data-pane="table-toolbar"></div>
          <div class="ws-table" data-pane="table"></div>
        </section>
        <aside class="ws-detail" aria-label="Selected entry" data-pane="detail"></aside>
      </div>
    `;
  }

  pane(name: WorkspacePane): HTMLElement {
    const el = this.querySelector<HTMLElement>(`[data-pane="${name}"]`);
    if (!el) {
      throw new Error(`looq-workspace: no pane "${name}"`);
    }
    return el;
  }

  /** The timeline row is collapsible; the shells hide it entirely while there is
   * nothing to plot (file mode before a file is opened). */
  setTimelineVisible(visible: boolean): void {
    (this.querySelector("#ws-timeline") as HTMLDetailsElement).hidden = !visible;
  }
}

customElements.define("looq-workspace", LooqWorkspace);
