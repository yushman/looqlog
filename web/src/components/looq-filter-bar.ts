// The left rail (`filtering`/`search` specs, design.md D3/D4/D5 of
// `frontend-three-pane-layout`; D1/D2/D5 of `filtering-and-search`). Search, time
// range, level and one collapsible section per inventory field, all in one rail.
// Owns only UI-local mechanics — which values are toggled, the raw search text, the
// last successfully compiled query, the text in the range inputs. It never computes
// the predicate result itself (the shell/`EntryIndex` do that over the actual
// dataset); its job is to turn clicks and keystrokes into `filters-change` /
// `range-change` events carrying committed state.
//
// The combination rule is stated in the UI itself, not only in the spec —
// `filtering` spec, "Combination rule ... SHALL be stated in the UI or its help
// rather than left to be inferred".
//
// **Controls are reconciled, never rebuilt** (design.md D4, `filtering` spec's
// "Filter controls stay operable while entries arrive"): a live batch runs the count
// pass, which writes into existing count nodes; the structural pass only runs when
// the *set* of fields or values actually changes, and even then it adds and removes
// nodes rather than replacing the tree. The previous implementation reassigned the
// container's `innerHTML` on every inventory update, which under a stream detached
// the control between a user's `mousedown` and `mouseup` — the browser then
// synthesises no `click` at all, so filters silently stopped responding at ~25
// lines/sec while looking perfectly normal.

import { knownFieldNames } from "../predicate";
import type { CompiledQuery } from "../predicate";
import { compileQuery, extractFieldTokens } from "../search-query";
import type { TimeRange } from "../time-range";
import type { FieldInventoryDto } from "../wasm-types";
import { isNarrowLayout } from "./looq-workspace";

/** UI-side cap on how many values a field lists, independent of the parser's own
 * cardinality cap (`crates/looq-core/src/fields.rs`, `DEFAULT_FIELD_VALUE_CAP` =
 * 10,000). That cap bounds the inventory's *memory*, not the rail's *DOM* — a field
 * can have exactly as many distinct values as the cap and never trip
 * `highCardinality`. Found in the golden-path checkpoint (see `docs/devlog.md`): a
 * 10k-line fixture's `request_id` field rendered 10,000 buttons. Any field over this
 * threshold gets the same typed-entry fallback as a backend-flagged high-cardinality
 * one, whichever trips first. */
const CHIP_LIST_MAX_VALUES = 50;

/** How many inventory field sections start expanded (design.md D3: "Search, Level
 * and the first N field sections open"). */
const FIELD_SECTIONS_OPEN_BY_DEFAULT = 3;

/** Severity order, so the level rows read top-to-bottom the way the reference does
 * and — more importantly — keep a stable position as new levels appear mid-stream. */
const LEVEL_ORDER = ["TRACE", "DEBUG", "INFO", "WARN", "ERROR", "FATAL"];

export interface FiltersChangeDetail {
  fieldFilters: Map<string, Set<string>>;
  queryText: string;
  compiledQuery: CompiledQuery;
}

interface FieldModel {
  name: string;
  values: Map<string, number>;
  highCardinality: boolean;
}

/** One value control (a level row or a field value row) and the count node the
 * count pass writes into. The elements are created once and kept — that identity is
 * what makes a human-speed click work during a stream. */
interface ValueControl {
  root: HTMLButtonElement;
  countEl: HTMLElement;
}

interface FieldSection {
  detailsEl: HTMLDetailsElement;
  stateEl: HTMLElement;
  listEl: HTMLElement;
  controls: Map<string, ValueControl>;
  /** Whether this section renders a typed-value input instead of a value list. */
  typed: boolean;
}

function escapeHtml(input: string): string {
  return input.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");
}

function pad(n: number, width = 2): string {
  return String(n).padStart(width, "0");
}

/** `YYYY-MM-DDTHH:MM:SS.mmm`, UTC — the same shape the timeline and the table label
 * their timestamps with, so a bound read off a row can be typed back in verbatim. */
export function formatRangeBound(ms: number): string {
  const d = new Date(ms);
  return (
    `${d.getUTCFullYear()}-${pad(d.getUTCMonth() + 1)}-${pad(d.getUTCDate())}` +
    `T${pad(d.getUTCHours())}:${pad(d.getUTCMinutes())}:${pad(d.getUTCSeconds())}.${pad(d.getUTCMilliseconds(), 3)}`
  );
}

/** Parses what `formatRangeBound` writes (and anything `Date.parse` accepts).
 * A bound with no timezone marker is read as UTC, matching how entries are
 * displayed — not as the browser's local zone, which would silently shift a range
 * by hours. */
export function parseRangeBound(text: string): number | null {
  const trimmed = text.trim();
  if (trimmed.length === 0) {
    return null;
  }
  const hasZone = /(?:Z|[+-]\d{2}:?\d{2})$/i.test(trimmed);
  const ms = Date.parse(hasZone ? trimmed : `${trimmed}Z`);
  return Number.isFinite(ms) ? ms : null;
}

export class LooqFilterBar extends HTMLElement {
  private inventory: FieldInventoryDto | null = null;
  private levelCounts: ReadonlyMap<string, number> = new Map();
  private fieldFilters = new Map<string, Set<string>>();
  private compiledQuery: CompiledQuery = { kind: "none" };
  private queryError: string | null = null;
  private activeRange: TimeRange | null = null;
  /** Debounces `emitChange` for keystrokes only — value clicks stay immediate.
   * Measured cost (`docs/devlog.md`): re-filtering + re-rendering the table and
   * timeline on every keystroke cost ~30-50ms at 10k entries in a real browser.
   * `recompileQuery` still runs on every keystroke so the error banner and the
   * `field=value` promotion stay instant; only the expensive downstream re-render
   * waits for typing to pause. */
  private searchDebounceTimer: ReturnType<typeof setTimeout> | null = null;
  private static readonly SEARCH_DEBOUNCE_MS = 150;

  private sectionsEl!: HTMLDivElement;
  private searchInputEl!: HTMLInputElement;
  private errorEl!: HTMLParagraphElement;
  private countEl!: HTMLParagraphElement;
  private clearAllBtn!: HTMLButtonElement;
  private rangeStartEl!: HTMLInputElement;
  private rangeEndEl!: HTMLInputElement;
  private rangeErrorEl!: HTMLParagraphElement;
  private rangeStateEl!: HTMLElement;

  /** Live sections, keyed by field name. Never rebuilt wholesale — see the file
   * header and `syncSections`. */
  private sections = new Map<string, FieldSection>();
  private fieldSectionCount = 0;

  connectedCallback(): void {
    const railOpen = !isNarrowLayout();
    this.innerHTML = `
      <div class="rail-filters">
        <div class="rail-header">
          <p class="filter-count" id="filter-count"></p>
          <button type="button" id="clear-all">Clear all</button>
        </div>
        <details class="rail-section" id="section-search"${railOpen ? " open" : ""}>
          <summary><span class="rail-section-title">Search</span></summary>
          <div class="rail-section-body">
            <input type="search" id="search-input" placeholder="text, re:regex, or field=value…" />
            <p class="filter-query-error" id="query-error" role="alert" hidden></p>
            <p class="filter-rule-note">
              Several values of the <em>same</em> field match <strong>any</strong> of them;
              different fields must <strong>all</strong> match, together with the time range
              and the search text.
            </p>
          </div>
        </details>
        <details class="rail-section" id="section-time">
          <summary>
            <span class="rail-section-title">Time range</span>
            <span class="rail-section-state" id="range-state">all</span>
          </summary>
          <div class="rail-section-body">
            <label class="rail-range-label">from
              <input type="text" id="range-start" placeholder="YYYY-MM-DDTHH:MM:SS.mmm" />
            </label>
            <label class="rail-range-label">to
              <input type="text" id="range-end" placeholder="YYYY-MM-DDTHH:MM:SS.mmm" />
            </label>
            <div class="rail-range-actions">
              <button type="button" id="range-apply">Apply</button>
              <button type="button" id="range-clear">Clear</button>
            </div>
            <p class="rail-range-error" id="range-error" role="alert" hidden></p>
          </div>
        </details>
        <div class="rail-sections" id="field-sections"></div>
      </div>
    `;
    this.sectionsEl = this.querySelector("#field-sections") as HTMLDivElement;
    this.searchInputEl = this.querySelector("#search-input") as HTMLInputElement;
    this.errorEl = this.querySelector("#query-error") as HTMLParagraphElement;
    this.countEl = this.querySelector("#filter-count") as HTMLParagraphElement;
    this.clearAllBtn = this.querySelector("#clear-all") as HTMLButtonElement;
    this.rangeStartEl = this.querySelector("#range-start") as HTMLInputElement;
    this.rangeEndEl = this.querySelector("#range-end") as HTMLInputElement;
    this.rangeErrorEl = this.querySelector("#range-error") as HTMLParagraphElement;
    this.rangeStateEl = this.querySelector("#range-state") as HTMLElement;

    this.searchInputEl.addEventListener("input", () => this.handleSearchInput());
    this.searchInputEl.addEventListener("keydown", (event) => {
      if (event.key === "Escape") {
        // `search` spec, "Clearing search": clears only the query, filters stay.
        // Bypasses the debounce — Escape is a discrete, deliberate action.
        event.stopPropagation();
        this.clearSearchDebounce();
        this.searchInputEl.value = "";
        this.setQuery("");
      }
    });
    // Delegated on the sections container, which is created once and never
    // replaced — the controls inside it are reconciled in place (D4).
    this.sectionsEl.addEventListener("click", (event) => this.handleValueClick(event));
    this.clearAllBtn.addEventListener("click", () => this.clearAll());

    for (const input of [this.rangeStartEl, this.rangeEndEl]) {
      input.addEventListener("keydown", (event) => {
        if (event.key === "Enter") {
          event.preventDefault();
          this.applyRangeInputs();
        }
      });
    }
    (this.querySelector("#range-apply") as HTMLButtonElement).addEventListener("click", () =>
      this.applyRangeInputs(),
    );
    (this.querySelector("#range-clear") as HTMLButtonElement).addEventListener("click", () => {
      this.rangeStartEl.value = "";
      this.rangeEndEl.value = "";
      this.rangeErrorEl.hidden = true;
      this.dispatchEvent(new CustomEvent<TimeRange | null>("range-change", { detail: null }));
    });

    this.syncSections();
    this.renderCount();
  }

  disconnectedCallback(): void {
    this.clearSearchDebounce();
  }

  /** New dataset, or an updated field inventory for a live stream — the field
   * inventory is the parser's cumulative snapshot; `levelCounts` comes from
   * `EntryIndex.levelStats` since `level` never enters that inventory. Active
   * filters referencing fields/values not in this snapshot are kept (still shown,
   * still applied) rather than silently dropped — a value can be legitimately
   * active before the inventory has caught up to it (live mode) or after eviction
   * has aged it out.
   *
   * Called on every live batch: the structural pass inside `syncSections` no-ops
   * unless the set of fields/values changed; the counts always update in place. */
  setFieldInventory(inventory: FieldInventoryDto | null, levelCounts: ReadonlyMap<string, number>): void {
    this.inventory = inventory;
    this.levelCounts = levelCounts;
    this.syncSections();
  }

  setCounts(matching: number, total: number): void {
    this.renderCount(matching, total);
  }

  /** The shell's active range changed (timeline drag, hash, or this rail's own
   * inputs). Reflects it into the inputs; never emits, so this cannot loop. */
  setActiveRange(range: TimeRange | null): void {
    this.activeRange = range;
    this.rangeErrorEl.hidden = true;
    this.rangeStartEl.value = range ? formatRangeBound(range.startMs) : "";
    this.rangeEndEl.value = range ? formatRangeBound(range.endMs) : "";
    this.rangeStateEl.textContent = range ? "narrowed" : "all";
  }

  /** New file/stream: local UI state resets, but this does NOT itself notify the
   * shell — the caller is already mid-reset and recomputes its own predicate from
   * the now-empty state directly. */
  reset(): void {
    this.clearSearchDebounce();
    this.fieldFilters = new Map();
    this.compiledQuery = { kind: "none" };
    this.queryError = null;
    this.searchInputEl.value = "";
    this.syncSections();
    this.renderError();
  }

  /** Applies filters/query decoded from the URL hash (`url-state` spec). Values are
   * accepted unconditionally — a hash may name a value the inventory hasn't
   * reported yet in a fresh live session — but the caller (`looq-app.ts`) checks
   * field *names* against `knownFieldNames` and reports what it drops. */
  applyExternalState(fieldFilters: ReadonlyMap<string, ReadonlySet<string>>, queryText: string): void {
    this.clearSearchDebounce();
    this.fieldFilters = new Map([...fieldFilters].map(([f, vs]) => [f, new Set(vs)]));
    this.searchInputEl.value = queryText;
    this.recompileQuery(queryText, /* emit */ false);
    this.syncSections();
    this.emitChange();
  }

  getState(): { fieldFilters: Map<string, Set<string>>; queryText: string } {
    return { fieldFilters: new Map(this.fieldFilters), queryText: this.searchInputEl.value };
  }

  private knownFields(): Set<string> {
    return knownFieldNames(this.inventory);
  }

  private applyRangeInputs(): void {
    const startText = this.rangeStartEl.value.trim();
    const endText = this.rangeEndEl.value.trim();
    if (startText.length === 0 && endText.length === 0) {
      this.rangeErrorEl.hidden = true;
      this.dispatchEvent(new CustomEvent<TimeRange | null>("range-change", { detail: null }));
      return;
    }
    const startMs = parseRangeBound(startText);
    const endMs = parseRangeBound(endText);
    if (startMs === null || endMs === null) {
      // Reported, never silently ignored — same rule the hash decoder follows.
      this.rangeErrorEl.hidden = false;
      this.rangeErrorEl.textContent =
        "Both bounds are needed, as YYYY-MM-DDTHH:MM:SS(.mmm) in UTC.";
      return;
    }
    if (endMs < startMs) {
      this.rangeErrorEl.hidden = false;
      this.rangeErrorEl.textContent = "The end bound is before the start bound.";
      return;
    }
    this.rangeErrorEl.hidden = true;
    this.dispatchEvent(new CustomEvent<TimeRange>("range-change", { detail: { startMs, endMs } }));
  }

  private handleSearchInput(): void {
    const raw = this.searchInputEl.value;
    const { tokens, remainingText } = extractFieldTokens(raw, this.knownFields());
    if (tokens.length > 0) {
      for (const t of tokens) {
        this.toggleValue(t.field, t.value, true, /* emit */ false);
      }
      this.searchInputEl.value = remainingText;
      this.syncSections();
    }
    this.recompileQuery(this.searchInputEl.value, /* emit */ false);
    this.scheduleEmit();
  }

  /** A compile error never touches `compiledQuery` — the previously active one
   * keeps being what `filters-change` reports and what the table renders against,
   * so a table full of results never flips to empty because of a typo. */
  private recompileQuery(text: string, emit: boolean): void {
    const result = compileQuery(text);
    if (result.error !== null) {
      this.queryError = result.error;
      this.renderError();
      return; // compiledQuery intentionally left untouched
    }
    this.queryError = null;
    this.compiledQuery = result.compiled!;
    this.renderError();
    if (emit) {
      this.emitChange();
    }
  }

  private setQuery(text: string): void {
    this.recompileQuery(text, true);
  }

  private scheduleEmit(): void {
    this.clearSearchDebounce();
    this.searchDebounceTimer = setTimeout(() => {
      this.searchDebounceTimer = null;
      this.emitChange();
    }, LooqFilterBar.SEARCH_DEBOUNCE_MS);
  }

  private clearSearchDebounce(): void {
    if (this.searchDebounceTimer !== null) {
      clearTimeout(this.searchDebounceTimer);
      this.searchDebounceTimer = null;
    }
  }

  private toggleValue(field: string, value: string, on: boolean, emit: boolean): void {
    let values = this.fieldFilters.get(field);
    if (on) {
      if (!values) {
        values = new Set();
        this.fieldFilters.set(field, values);
      }
      values.add(value);
    } else if (values) {
      values.delete(value);
      if (values.size === 0) {
        this.fieldFilters.delete(field);
      }
    }
    if (emit) {
      this.syncSections();
      this.emitChange();
    }
  }

  private handleValueClick(event: Event): void {
    const target = event.target as HTMLElement;
    const toggleEl = target.closest<HTMLElement>("[data-toggle-field]");
    if (toggleEl) {
      const field = toggleEl.dataset.toggleField!;
      const value = toggleEl.dataset.toggleValue!;
      const active = this.fieldFilters.get(field)?.has(value) ?? false;
      this.toggleValue(field, value, !active, true);
      return;
    }
    const addBtn = target.closest<HTMLElement>("[data-add-field]");
    if (addBtn) {
      const field = addBtn.dataset.addField!;
      const input = this.typedInput(field);
      if (input && input.value.trim().length > 0) {
        this.toggleValue(field, input.value.trim(), true, true);
        input.value = "";
      }
    }
  }

  private typedInput(field: string): HTMLInputElement | null {
    const section = this.sections.get(field);
    return section?.detailsEl.querySelector<HTMLInputElement>("input[data-typed-field]") ?? null;
  }

  private clearAll(): void {
    this.clearSearchDebounce();
    this.fieldFilters = new Map();
    this.searchInputEl.value = "";
    this.compiledQuery = { kind: "none" };
    this.queryError = null;
    this.syncSections();
    this.renderError();
    this.emitChange();
  }

  private emitChange(): void {
    const detail: FiltersChangeDetail = {
      fieldFilters: new Map(this.fieldFilters),
      queryText: this.searchInputEl.value,
      compiledQuery: this.compiledQuery,
    };
    this.dispatchEvent(new CustomEvent<FiltersChangeDetail>("filters-change", { detail }));
  }

  private renderError(): void {
    if (this.queryError) {
      this.errorEl.hidden = false;
      this.errorEl.textContent = this.queryError;
    } else {
      this.errorEl.hidden = true;
      this.errorEl.textContent = "";
    }
  }

  private renderCount(matching?: number, total?: number): void {
    if (matching === undefined || total === undefined) {
      this.countEl.textContent = "";
      return;
    }
    const hasFilters = this.fieldFilters.size > 0 || this.compiledQuery.kind !== "none";
    if (matching === 0 && total > 0 && hasFilters) {
      // `filtering` spec: "A filter that matches nothing is distinguishable" — from
      // an empty file, and from a search that's still mid-typo.
      this.countEl.textContent = `0 of ${total} — filters exclude every entry`;
      this.countEl.className = "filter-count filter-count-empty";
    } else {
      this.countEl.textContent = `${matching} / ${total} entries`;
      this.countEl.className = "filter-count";
    }
  }

  /** The field/value model the rail renders: `level` first (its counts come from
   * the index, not the parser's inventory), then every reported field. */
  private fieldModels(): FieldModel[] {
    const models: FieldModel[] = [];
    const levelValues = new Map<string, number>();
    const active = this.fieldFilters.get("level") ?? new Set<string>();
    const seen = new Set<string>([...this.levelCounts.keys(), ...active]);
    for (const level of LEVEL_ORDER) {
      if (seen.has(level)) {
        levelValues.set(level, this.levelCounts.get(level) ?? 0);
        seen.delete(level);
      }
    }
    for (const level of [...seen].sort()) {
      levelValues.set(level, this.levelCounts.get(level) ?? 0);
    }
    models.push({ name: "level", values: levelValues, highCardinality: false });

    if (this.inventory) {
      for (const [name, stats] of Object.entries(this.inventory.fields)) {
        models.push({
          name,
          values: new Map(Object.entries(stats.values)),
          highCardinality: stats.highCardinality,
        });
      }
    }
    return models;
  }

  /** The structural pass (adds/removes sections and value controls only when the
   * set changed) plus the count pass (always). Existing element instances survive
   * — that identity is the whole point (D4). */
  private syncSections(): void {
    const models = this.fieldModels();
    const wanted = new Set<string>();

    for (const model of models) {
      const active = this.fieldFilters.get(model.name) ?? new Set<string>();
      if (model.values.size === 0 && active.size === 0) {
        continue; // nothing to offer and nothing active — no empty section
      }
      wanted.add(model.name);
      const typed = model.highCardinality || model.values.size > CHIP_LIST_MAX_VALUES;
      let section = this.sections.get(model.name);
      if (section && section.typed !== typed) {
        // A field that grew past the list cap changes shape once, then stays.
        section.detailsEl.remove();
        this.sections.delete(model.name);
        section = undefined;
      }
      if (!section) {
        section = this.createSection(model.name, typed);
        this.sections.set(model.name, section);
        this.sectionsEl.appendChild(section.detailsEl);
      }
      this.syncValues(section, model, active, typed);
      this.renderSectionState(section, model, active, typed);
    }

    for (const [name, section] of this.sections) {
      if (!wanted.has(name)) {
        section.detailsEl.remove();
        this.sections.delete(name);
      }
    }
  }

  private createSection(field: string, typed: boolean): FieldSection {
    const detailsEl = document.createElement("details");
    detailsEl.className = "rail-section rail-field-section";
    detailsEl.dataset.field = field;
    // design.md D3: `open` encodes the default and is written exactly once, at
    // creation — never on an update, or a live batch would reopen a section the
    // user just closed. Section state is per-session UI state and deliberately
    // never written to the URL hash (the hash describes the query, not furniture).
    const isPrimary = field === "level";
    detailsEl.open =
      !isNarrowLayout() && (isPrimary || this.fieldSectionCount < FIELD_SECTIONS_OPEN_BY_DEFAULT);
    if (!isPrimary) {
      this.fieldSectionCount += 1;
    }
    detailsEl.innerHTML = `
      <summary>
        <span class="rail-section-title">${escapeHtml(field)}</span>
        <span class="rail-section-state"></span>
      </summary>
      <div class="rail-section-body">
        ${
          typed
            ? `<span class="filter-typed-entry">
                 <input type="text" data-typed-field="${escapeHtml(field)}" placeholder="type a value…" />
                 <button type="button" data-add-field="${escapeHtml(field)}">Add</button>
               </span>`
            : ""
        }
        <div class="rail-value-list"></div>
      </div>
    `;
    return {
      detailsEl,
      stateEl: detailsEl.querySelector(".rail-section-state") as HTMLElement,
      listEl: detailsEl.querySelector(".rail-value-list") as HTMLElement,
      controls: new Map(),
      typed,
    };
  }

  /** Structural pass for one section's values, plus the count write. New controls
   * are inserted at their ordered position; existing ones are never moved, so a
   * control cannot be detached under a finger that is pressing it. */
  private syncValues(
    section: FieldSection,
    model: FieldModel,
    active: ReadonlySet<string>,
    typed: boolean,
  ): void {
    // A typed-entry field lists only what is actually selected (its value space is
    // too large to enumerate); a listed field shows the inventory's values plus any
    // active value the current snapshot doesn't contain.
    const order = typed ? [...active] : [...new Set([...model.values.keys(), ...active])];
    const wanted = new Set(order);

    for (const [value, control] of section.controls) {
      if (!wanted.has(value)) {
        control.root.remove();
        section.controls.delete(value);
      }
    }

    for (let i = 0; i < order.length; i++) {
      const value = order[i]!;
      let control = section.controls.get(value);
      if (!control) {
        control = createValueControl(model.name, value);
        section.controls.set(value, control);
        // Insert at the ordered position without moving any existing node.
        const before = findFollowingControl(section, order, i);
        section.listEl.insertBefore(control.root, before);
      }
      // The count pass: a text write into a node that stays put.
      const count = model.values.get(value);
      control.countEl.textContent = count === undefined ? "" : String(count);
      const isActive = active.has(value);
      control.root.setAttribute("aria-pressed", String(isActive));
      control.root.classList.toggle("active", isActive);
    }
  }

  private renderSectionState(
    section: FieldSection,
    model: FieldModel,
    active: ReadonlySet<string>,
    typed: boolean,
  ): void {
    // `filtering` spec, "Many fields stay manageable": a collapsed section still
    // names its field (the summary title) and says how many values it holds.
    const values = typed ? null : model.values.size;
    const parts: string[] = [];
    if (active.size > 0) {
      parts.push(`${active.size} selected`);
    }
    parts.push(values === null ? "typed values" : `${values} value${values === 1 ? "" : "s"}`);
    section.stateEl.textContent = parts.join(" · ");
    section.detailsEl.classList.toggle("has-active", active.size > 0);
  }
}

function createValueControl(field: string, value: string): ValueControl {
  const root = document.createElement("button");
  root.type = "button";
  root.className =
    field === "level"
      ? `rail-value-row rail-level-row level-${value.toLowerCase()}`
      : "rail-value-row";
  root.dataset.toggleField = field;
  root.dataset.toggleValue = value;
  root.setAttribute("aria-pressed", "false");

  const nameEl = document.createElement("span");
  nameEl.className = "rail-value-name";
  nameEl.textContent = value;
  const countEl = document.createElement("span");
  countEl.className = "rail-value-count";

  root.appendChild(nameEl);
  root.appendChild(countEl);
  return { root, countEl };
}

/** The first already-mounted control that must come after `order[i]`, or `null` to
 * append — lets a new value land in its ordered position without relocating any
 * existing control. */
function findFollowingControl(section: FieldSection, order: readonly string[], i: number): Node | null {
  for (let j = i + 1; j < order.length; j++) {
    const next = section.controls.get(order[j]!);
    if (next) {
      return next.root;
    }
  }
  return null;
}

customElements.define("looq-filter-bar", LooqFilterBar);
