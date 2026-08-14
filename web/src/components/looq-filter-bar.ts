// Filter chips + search box (`filtering`/`search` specs, design.md D1/D3/D4/D5).
// Owns only UI-local mechanics — which chips are toggled, the raw search text, the
// last successfully compiled query. It never computes the predicate result itself
// (D2: the shell/`EntryIndex` do that over the actual dataset); this component's
// only job is to turn clicks and keystrokes into one `filters-change` event
// carrying committed state, so the shell has exactly one thing to listen to.
//
// The combination rule (D1) is stated here in the UI itself, not only in the spec
// — `filtering` spec, "Combination rule ... SHALL be stated in the UI or its help
// rather than left to be inferred".

import { knownFieldNames } from "../predicate";
import type { CompiledQuery } from "../predicate";
import { compileQuery, extractFieldTokens } from "../search-query";
import type { FieldInventoryDto } from "../wasm-types";

/** UI-side cap on how many value chips a field lists, independent of the parser's
 * own cardinality cap (`crates/looq-core/src/fields.rs`, `DEFAULT_FIELD_VALUE_CAP`
 * = 10,000). That cap bounds the inventory's *memory*, not the chip bar's *DOM* —
 * a field can have exactly as many distinct values as the cap and never trip
 * `highCardinality` (one row per value, no value repeated: cap reached, never
 * exceeded). Found in the golden-path checkpoint (task 7.1/7.2, `docs/devlog.md`):
 * a 10k-line fixture's `request_id` field (one distinct value per line) rendered
 * 10,000 chip buttons — unusable, and no field's own flag caught it. Any field
 * over this threshold gets the same typed-entry fallback as a backend-flagged
 * high-cardinality one, whichever trips first. */
const CHIP_LIST_MAX_VALUES = 50;

export interface FiltersChangeDetail {
  fieldFilters: Map<string, Set<string>>;
  queryText: string;
  compiledQuery: CompiledQuery;
}

function escapeHtml(input: string): string {
  return input.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");
}

export class LooqFilterBar extends HTMLElement {
  private inventory: FieldInventoryDto | null = null;
  private levelCounts: ReadonlyMap<string, number> = new Map();
  private fieldFilters = new Map<string, Set<string>>();
  private compiledQuery: CompiledQuery = { kind: "none" };
  private queryError: string | null = null;
  /** Debounces `emitChange` for keystrokes only — chip clicks stay immediate.
   * Measured cost (`docs/devlog.md`, task 7.3/7.4): re-filtering + re-rendering
   * the table and timeline on every keystroke cost ~30-50ms at 10k entries in a
   * real browser (the DOM write/reflow the virtual-scroll table pays per render,
   * not `EntryIndex`'s predicate matching itself, which measured under 2ms) —
   * fine for one keystroke, not for fifteen back to back. `recompileQuery` itself
   * still runs on every keystroke so the error banner and `field=value` chip
   * conversion stay instant (D5); only the expensive downstream re-render waits
   * for typing to pause. */
  private searchDebounceTimer: ReturnType<typeof setTimeout> | null = null;
  private static readonly SEARCH_DEBOUNCE_MS = 150;

  private chipsEl!: HTMLDivElement;
  private searchInputEl!: HTMLInputElement;
  private errorEl!: HTMLParagraphElement;
  private countEl!: HTMLParagraphElement;
  private clearAllBtn!: HTMLButtonElement;

  connectedCallback(): void {
    this.innerHTML = `
      <div class="filter-bar">
        <p class="filter-rule-note">
          Selecting more than one value of the <em>same</em> field shows entries matching
          <strong>any</strong> of them; different fields must <strong>all</strong> match
          (and both combine with the time range and search below).
        </p>
        <div class="filter-chips" id="chips"></div>
        <div class="filter-search-row">
          <input type="search" id="search-input" placeholder="Search message and field values, or type field=value… (re: for regex, Esc to clear)" />
          <button type="button" id="clear-all">Clear all filters</button>
        </div>
        <p class="filter-query-error" id="query-error" role="alert" hidden></p>
        <p class="filter-count" id="filter-count"></p>
      </div>
    `;
    this.chipsEl = this.querySelector("#chips") as HTMLDivElement;
    this.searchInputEl = this.querySelector("#search-input") as HTMLInputElement;
    this.errorEl = this.querySelector("#query-error") as HTMLParagraphElement;
    this.countEl = this.querySelector("#filter-count") as HTMLParagraphElement;
    this.clearAllBtn = this.querySelector("#clear-all") as HTMLButtonElement;

    this.searchInputEl.addEventListener("input", () => this.handleSearchInput());
    this.searchInputEl.addEventListener("keydown", (event) => {
      if (event.key === "Escape") {
        // `search` spec, "Clearing search": clears only the query, chips stay.
        // Bypasses the debounce below — Escape is a discrete, deliberate action,
        // not a keystroke to coalesce with the next one.
        event.stopPropagation();
        this.clearSearchDebounce();
        this.searchInputEl.value = "";
        this.setQuery("");
      }
    });
    this.chipsEl.addEventListener("click", (event) => this.handleChipClick(event));
    this.clearAllBtn.addEventListener("click", () => this.clearAll());

    this.renderChips();
    this.renderCount();
  }

  disconnectedCallback(): void {
    this.clearSearchDebounce();
  }

  /** New dataset, or an updated field inventory for a live stream — the field
   * inventory is the parser's cumulative snapshot (`bridge.ts`/`live-tail.ts`);
   * `levelCounts` comes from `EntryIndex.levelStats` since `level` never enters
   * that inventory (`predicate.ts`). Active filters referencing fields/values not
   * in this snapshot are kept (still shown, still applied) rather than silently
   * dropped — a value can be legitimately active before the inventory has caught
   * up to it (live mode) or after eviction has aged it out. */
  setFieldInventory(inventory: FieldInventoryDto | null, levelCounts: ReadonlyMap<string, number>): void {
    this.inventory = inventory;
    this.levelCounts = levelCounts;
    this.renderChips();
  }

  setCounts(matching: number, total: number): void {
    this.renderCount(matching, total);
  }

  /** New file/stream: local UI state resets, but this does NOT itself notify the
   * shell — the caller is already mid-reset (`looq-app.ts`'s `openFile`) and will
   * recompute its own predicate from the now-empty state directly. */
  reset(): void {
    this.clearSearchDebounce();
    this.fieldFilters = new Map();
    this.compiledQuery = { kind: "none" };
    this.queryError = null;
    this.searchInputEl.value = "";
    this.renderChips();
    this.renderError();
  }

  /** Applies filters/query decoded from the URL hash (`url-state` spec, "A hash is
   * applied on load"). Chip values are accepted unconditionally — same reasoning
   * as `setFieldInventory` above, a hash may name a field/value the inventory
   * hasn't reported yet in a fresh live session — but the caller (`looq-app.ts`)
   * is the one that checks field *names* against `knownFieldNames` and reports
   * what it drops, since only it knows whether "unknown" should be reported at
   * all (nothing to check against before a file is even opened). */
  applyExternalState(fieldFilters: ReadonlyMap<string, ReadonlySet<string>>, queryText: string): void {
    this.clearSearchDebounce();
    this.fieldFilters = new Map([...fieldFilters].map(([f, vs]) => [f, new Set(vs)]));
    this.searchInputEl.value = queryText;
    this.recompileQuery(queryText, /* emit */ false);
    this.renderChips();
    this.emitChange();
  }

  getState(): { fieldFilters: Map<string, Set<string>>; queryText: string } {
    return { fieldFilters: new Map(this.fieldFilters), queryText: this.searchInputEl.value };
  }

  private knownFields(): Set<string> {
    return knownFieldNames(this.inventory);
  }

  private handleSearchInput(): void {
    const raw = this.searchInputEl.value;
    const { tokens, remainingText } = extractFieldTokens(raw, this.knownFields());
    if (tokens.length > 0) {
      for (const t of tokens) {
        this.toggleChip(t.field, t.value, true, /* emit */ false);
      }
      this.searchInputEl.value = remainingText;
      this.renderChips();
    }
    // Instant: error banner / compiled-query state (D5, "Error clears on
    // correction" reads live). Debounced: the emission that triggers the shell's
    // expensive re-filter + re-render (see `searchDebounceTimer`'s doc comment).
    this.recompileQuery(this.searchInputEl.value, /* emit */ false);
    this.scheduleEmit();
  }

  /** D5: a compile error never touches `compiledQuery` — the previously active
   * one keeps being what `filters-change` reports and what the table renders
   * against, so a table full of results never flips to empty because of a typo. */
  private recompileQuery(text: string, emit: boolean): void {
    const result = compileQuery(text);
    if (result.error !== null) {
      this.queryError = result.error;
      this.renderError();
      return; // compiledQuery intentionally left untouched (D5)
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

  private toggleChip(field: string, value: string, on: boolean, emit: boolean): void {
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
      this.renderChips();
      this.emitChange();
    }
  }

  private handleChipClick(event: Event): void {
    const target = event.target as HTMLElement;
    const toggleEl = target.closest<HTMLElement>("[data-toggle-field]");
    if (toggleEl) {
      const field = toggleEl.dataset.toggleField!;
      const value = toggleEl.dataset.toggleValue!;
      const active = this.fieldFilters.get(field)?.has(value) ?? false;
      this.toggleChip(field, value, !active, true);
      return;
    }
    const addBtn = target.closest<HTMLElement>("[data-add-field]");
    if (addBtn) {
      const field = addBtn.dataset.addField!;
      const input = this.chipsEl.querySelector<HTMLInputElement>(`input[data-typed-field="${cssEscape(field)}"]`);
      if (input && input.value.trim().length > 0) {
        this.toggleChip(field, input.value.trim(), true, true);
        input.value = "";
      }
    }
  }

  private clearAll(): void {
    this.clearSearchDebounce();
    this.fieldFilters = new Map();
    this.searchInputEl.value = "";
    this.compiledQuery = { kind: "none" };
    this.queryError = null;
    this.renderChips();
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
      // `filtering` spec: "A filter that matches nothing is distinguishable" —
      // from an empty file, and from a search that's still mid-typo.
      this.countEl.textContent = `0 of ${total} — filters and search exclude every entry.`;
      this.countEl.className = "filter-count filter-count-empty";
    } else {
      this.countEl.textContent = `${matching} of ${total} entries shown.`;
      this.countEl.className = "filter-count";
    }
  }

  private renderChips(): void {
    const fields: Array<{ name: string; values: Map<string, number>; highCardinality: boolean }> = [];
    fields.push({ name: "level", values: new Map(this.levelCounts), highCardinality: false });
    if (this.inventory) {
      for (const [name, stats] of Object.entries(this.inventory.fields)) {
        fields.push({ name, values: new Map(Object.entries(stats.values)), highCardinality: stats.highCardinality });
      }
    }

    const html: string[] = [];
    for (const field of fields) {
      const active = this.fieldFilters.get(field.name) ?? new Set<string>();
      if (field.values.size === 0 && active.size === 0) {
        continue; // nothing to offer and nothing active — don't show an empty row
      }
      html.push(`<div class="filter-field" data-field="${escapeHtml(field.name)}">`);
      html.push(`<span class="filter-field-name">${escapeHtml(field.name)}</span>`);
      if (field.highCardinality || field.values.size > CHIP_LIST_MAX_VALUES) {
        // Task 2.2: typed entry instead of a value list — either the field
        // inventory stopped tracking distinct values at its cap
        // (`crates/looq-core/src/fields.rs`), or it's under that cap but still far
        // too many to usefully list as buttons (`CHIP_LIST_MAX_VALUES`'s doc
        // comment).
        for (const value of active) {
          html.push(chipHtml(field.name, value, null, true));
        }
        html.push(`
          <span class="filter-typed-entry">
            <input type="text" data-typed-field="${escapeHtml(field.name)}" placeholder="type a value…" />
            <button type="button" data-add-field="${escapeHtml(field.name)}">Add</button>
          </span>`);
      } else {
        // Union of known values and any active value not currently in the
        // inventory snapshot (still valid to keep showing/removable).
        const allValues = new Set<string>([...field.values.keys(), ...active]);
        for (const value of allValues) {
          html.push(chipHtml(field.name, value, field.values.get(value) ?? null, active.has(value)));
        }
      }
      html.push(`</div>`);
    }
    this.chipsEl.innerHTML = html.join("");
  }
}

function chipHtml(field: string, value: string, count: number | null, active: boolean): string {
  const countLabel = count !== null ? ` <span class="chip-count">(${count})</span>` : "";
  return `
    <button type="button" class="filter-chip${active ? " active" : ""}"
            data-toggle-field="${escapeHtml(field)}" data-toggle-value="${escapeHtml(value)}"
            aria-pressed="${active}">
      ${escapeHtml(value)}${countLabel}
    </button>`;
}

function cssEscape(value: string): string {
  return value.replace(/["\\]/g, "\\$&");
}

customElements.define("looq-filter-bar", LooqFilterBar);
