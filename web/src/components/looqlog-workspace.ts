// The three-pane workspace layout (`app-shell` spec, "Three-pane workspace layout",
// design.md D1/D2). Defined exactly once and mounted by both shells — `looqlog-app.ts`
// (file mode) and `looqlog-live-tail.ts` (stream mode) — so a layout change lands in
// both at the same time instead of drifting between two hand-written page stacks.
//
// Light DOM, like every other component here (no shadow root, so `style.css` keeps
// applying), which means "slots" are plain containers a shell fills rather than
// `<slot>` elements. `pane(name)` is the only way in: shells own their content, the
// workspace owns where it goes.

import { COLLAPSIBLE_PANES, type CollapsedPanes, type CollapsiblePane } from "../panes";

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

/** Fired when the user toggles a pane, or when something inside the workspace
 * expands one on its own (a selection, an auto-expanded warning). Detail is the
 * full collapsed set, so the shell mirrors state rather than reconstructing it. */
export const PANES_CHANGE_EVENT = "panes-change";

/** Raised by a rail surface whose state `app-shell`'s "Secondary surfaces are
 * collapsible without hiding warnings" calls out (collapsible-workspace-panes
 * design D3). The workspace listens for it on itself, because the guarantee is
 * about the *pane*: a collapsed rail would otherwise swallow both the warning and
 * the requirement that it be visible. */
export const RAIL_ATTENTION_EVENT = "rail-attention";

export interface RailAttentionDetail {
  /** The surface is in a state the user should not have to go looking for:
   * skipped lines, or fallback / low-confidence detection. */
  needsAttention: boolean;
  /** The surface just opened itself. A condition severe enough to do that must
   * also expand the pane containing it, or the auto-expanded surface is expanded
   * inside a pane of zero width — visible to nobody. */
  autoExpand: boolean;
}

export function dispatchRailAttention(source: HTMLElement, detail: RailAttentionDetail): void {
  source.dispatchEvent(new CustomEvent<RailAttentionDetail>(RAIL_ATTENTION_EVENT, { bubbles: true, detail }));
}

/** D8: below a certain width the grid collapses to one column and the rail's
 * sections start collapsed — a fallback for a small window, not a mobile design.
 *
 * The width itself is defined exactly once, in `style.css`'s media query, and
 * published from there as the `--narrow-layout` custom property (0/1) so this
 * function reads the stylesheet's own definition instead of repeating the number
 * in a `matchMedia` string that could drift from it
 * (collapsible-workspace-panes design D5). */
export function isNarrowLayout(): boolean {
  return getComputedStyle(document.documentElement).getPropertyValue("--narrow-layout").trim() === "1";
}

/** The name each pane answers to — on its strip when collapsed, on its header row
 * when expanded, and as its button's accessible name in both states. */
const PANE_LABELS: Record<CollapsiblePane, string> = {
  rail: "Filters",
  detail: "Details",
};

/** A chevron pointing left. CSS mirrors it per pane and per state, so one glyph
 * covers all four combinations and always points the way the click moves the pane.
 * Inline, because the CSP is `default-src 'self'` and this app ships no assets
 * beyond the bundle. */
const CHEVRON_SVG = `<svg class="ws-pane-toggle-icon" viewBox="0 0 16 16" width="12" height="12" aria-hidden="true" focusable="false">
                  <path d="M10.5 2.5 5 8l5.5 5.5" fill="none" stroke="currentColor" stroke-width="2"
                        stroke-linecap="round" stroke-linejoin="round" />
                </svg>`;

/** The strip a pane keeps when collapsed (design D1): the reopen button and the
 * pane's name. The name is real text — `writing-mode` turns it on its side when
 * collapsed — so it stays selectable, zoomable and readable to a screen reader,
 * which a rotated background image or an icon would not be. */
function paneStrip(pane: CollapsiblePane): string {
  return `
          <div class="ws-pane-strip">
            <button type="button" class="ws-pane-toggle" data-toggle-pane="${pane}"
                    id="ws-toggle-${pane}" aria-controls="ws-pane-${pane}-content"
                    aria-expanded="true" aria-label="${PANE_LABELS[pane]}">
              ${CHEVRON_SVG}${
                // Only the rail holds surfaces the no-hidden-warnings rule is about
                // (detection, diagnostics); the detail pane shows the entry you just
                // selected and has nothing of its own to warn about, so it gets no
                // badge that could never light up.
                pane === "rail" ? `<span class="ws-pane-attention" data-attention-for="rail" hidden>!</span>` : ""
              }
            </button>
            <span class="ws-pane-strip-label">${PANE_LABELS[pane]}</span>
          </div>`;
}

export class LooqlogWorkspace extends HTMLElement {
  /** Which panes are collapsed. The panes' own `display` is untouched (D2): this
   * drives `.workspace`'s grid template, so a collapsed pane keeps its box — and
   * its scroll offset — instead of being destroyed and rebuilt on every toggle.
   * A collapsed track is the strip's width, not zero (D1): the strip is the only
   * way back, so it never leaves the screen. */
  private collapsed = new Set<CollapsiblePane>();
  private workspaceEl!: HTMLElement;
  private toggles = new Map<CollapsiblePane, HTMLButtonElement>();
  /** The rail surfaces currently asking for attention, keyed by the element that
   * said so — two surfaces can warn at once, and the badge must survive one of
   * them going quiet. */
  private railAttentionSources = new Set<EventTarget>();

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
        <aside class="ws-rail" id="ws-pane-rail" aria-label="Filters">${paneStrip("rail")}
          <div class="ws-pane-content" id="ws-pane-rail-content">
            <div class="ws-rail-main" data-pane="rail"></div>
            <div class="ws-rail-secondary" data-pane="rail-secondary"></div>
          </div>
        </aside>
        <section class="ws-center" aria-label="Log entries">
          <div class="ws-table-toolbar" data-pane="table-toolbar"></div>
          <div class="ws-table" data-pane="table"></div>
        </section>
        <aside class="ws-detail" id="ws-pane-detail" aria-label="Selected entry">${paneStrip("detail")}
          <div class="ws-pane-content" id="ws-pane-detail-content" data-pane="detail"></div>
        </aside>
      </div>
    `;
    this.workspaceEl = this.querySelector(".workspace") as HTMLElement;
    for (const pane of COLLAPSIBLE_PANES) {
      const toggle = this.querySelector<HTMLButtonElement>(`[data-toggle-pane="${pane}"]`)!;
      toggle.addEventListener("click", () => this.togglePane(pane));
      this.toggles.set(pane, toggle);
    }
    // Listened for here rather than in each shell: the rule being enforced is a
    // property of the pane, and both shells mount the same surfaces into it.
    this.addEventListener(RAIL_ATTENTION_EVENT, this.handleRailAttention as EventListener);
    this.renderCollapsed();
  }

  pane(name: WorkspacePane): HTMLElement {
    const el = this.querySelector<HTMLElement>(`[data-pane="${name}"]`);
    if (!el) {
      throw new Error(`looqlog-workspace: no pane "${name}"`);
    }
    return el;
  }

  /** The timeline row is collapsible; the shells hide it entirely while there is
   * nothing to plot (file mode before a file is opened). */
  setTimelineVisible(visible: boolean): void {
    (this.querySelector("#ws-timeline") as HTMLDetailsElement).hidden = !visible;
  }

  getCollapsedPanes(): Set<CollapsiblePane> {
    return new Set(this.collapsed);
  }

  /** Applies a collapsed set that came from outside — a shared link, on load —
   * without echoing it back as a change the shell would then re-serialise. */
  setCollapsedPanes(panes: CollapsedPanes): void {
    this.collapsed = new Set(COLLAPSIBLE_PANES.filter((pane) => panes.has(pane)));
    this.renderCollapsed();
  }

  /** Opens a pane that must not stay closed (design D3/D4): a selection landing in
   * the detail pane, or a warning severe enough to open its own section. A no-op
   * when the pane is already open, so it never emits a spurious change. */
  expandPane(pane: CollapsiblePane): void {
    if (!this.collapsed.delete(pane)) {
      return;
    }
    this.renderCollapsed();
    this.emitPanesChange();
  }

  private togglePane(pane: CollapsiblePane): void {
    if (!this.collapsed.delete(pane)) {
      this.collapsed.add(pane);
    }
    this.renderCollapsed();
    this.emitPanesChange();
  }

  private renderCollapsed(): void {
    const collapsed = COLLAPSIBLE_PANES.filter((pane) => this.collapsed.has(pane));
    // A space-separated token list matched with `[data-collapsed~="rail"]`, so one
    // attribute drives both grid tracks and the stacked layout's hidden blocks.
    if (collapsed.length > 0) {
      this.workspaceEl.dataset.collapsed = collapsed.join(" ");
    } else {
      delete this.workspaceEl.dataset.collapsed;
    }
    for (const pane of COLLAPSIBLE_PANES) {
      this.toggles.get(pane)?.setAttribute("aria-expanded", String(!this.collapsed.has(pane)));
    }
  }

  private emitPanesChange(): void {
    this.dispatchEvent(
      new CustomEvent<Set<CollapsiblePane>>(PANES_CHANGE_EVENT, { detail: this.getCollapsedPanes() }),
    );
  }

  private handleRailAttention = (event: CustomEvent<RailAttentionDetail>): void => {
    const source = event.target;
    if (!source) {
      return;
    }
    if (event.detail.needsAttention) {
      this.railAttentionSources.add(source);
    } else {
      this.railAttentionSources.delete(source);
    }
    this.renderRailAttention();
    if (event.detail.autoExpand) {
      this.expandPane("rail");
    }
  };

  private renderRailAttention(): void {
    const needsAttention = this.railAttentionSources.size > 0;
    const badge = this.querySelector<HTMLElement>(`[data-attention-for="rail"]`);
    if (badge) {
      badge.hidden = !needsAttention;
    }
    const toggle = this.toggles.get("rail");
    if (toggle) {
      // On the rail's own button — which is what stays on screen when the rail is
      // collapsed to its strip — not only inside the pane: a collapsed rail must
      // still say that something in it needs attention (design D1/D3).
      toggle.classList.toggle("attention", needsAttention);
      toggle.title = needsAttention ? "A surface in the filter rail needs attention" : "";
    }
  }
}

customElements.define("looqlog-workspace", LooqlogWorkspace);
