// The shell. Owns every piece of state — current file, parse result, detection,
// diagnostics, status — and passes it down to presentational children by property
// assignment (design.md D7: "State lives in the shell, not in components"). Children
// only dispatch events ("file-selected"); they never read or hold parse state
// themselves.

import "./looq-drop-target";
import "./looq-detection";
import "./looq-diagnostics";
import "./looq-timeline";
import "./looq-entry-table";
import "./looq-live-tail";
import "./looq-filter-bar";

import { ParseCancelledError, ParserBridge, WorkerInitError, type ParseProgress } from "../bridge";
import { looksBinary, SNIFF_BYTES } from "../binary-detect";
import { EntryIndex } from "../entry-index";
import { formatBytes, HARD_CAP_BYTES, WARN_THRESHOLD_BYTES } from "../limits";
import { knownFieldNames, matchesFieldFilters, matchesQuery, type CompiledQuery, type FieldFilters } from "../predicate";
import type { TimeRange } from "../time-range";
import { decodeHash, HashWriter, type DecodedHash } from "../url-hash";
import type { FieldInventoryDto } from "../wasm-types";
import type { LooqDetection } from "./looq-detection";
import type { LooqDiagnostics } from "./looq-diagnostics";
import type { LooqDropTarget } from "./looq-drop-target";
import type { LooqEntryTable } from "./looq-entry-table";
import type { FiltersChangeDetail, LooqFilterBar } from "./looq-filter-bar";
import type { LooqTimeline } from "./looq-timeline";

// No "error" state here (`error-states` spec, design.md D5): a failed open must
// not destroy what's already loaded, so an error is a separate, persistent banner
// (`errorMessage`/`renderError`) rather than a status this type can even represent
// — there is no way to "be in the error state" and still show a previous file's
// `done` status at the same time if `error` were a member of this union.
type Status = "unopened" | "parsing" | "done";

/** A large-or-binary file awaiting an explicit continue/cancel before parsing
 * starts (`error-states` spec, design.md D3/D4). */
interface PendingConfirm {
  file: File;
  message: string;
}

/** Mirrors the `__LOOQ_MODE__` JSON substituted server-side
 * (`crates/looq/src/main.rs`, `assets.rs`). */
type RunMode = { mode: "file" } | { mode: "stdin"; maxLines: number };

/** Reads the `#mode-template` placeholder (`app-shell`/`live-tail-ui` specs).
 * Falls back to file mode both when the element is missing and when its content is
 * still the literal `__LOOQ_MODE__` placeholder — the latter is what `vite dev`
 * serves with no backend to substitute it, same convention as `#hint-template`. */
function readRunMode(): RunMode {
  // `<template>` content is inert: `.textContent` sees nothing (its children live
  // in `.content`, a separate DocumentFragment), so — same as `#hint-template`
  // elsewhere in this file — `.innerHTML` is the one that actually reflects it.
  const template = document.getElementById("mode-template") as HTMLTemplateElement | null;
  const raw = template?.innerHTML.trim() ?? "";
  if (!raw || raw === "__LOOQ_MODE__") {
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

export class LooqApp extends HTMLElement {
  private bridge = new ParserBridge();
  private status: Status = "unopened";
  private currentFileName: string | null = null;
  /** The last open-file failure, if any, shown in its own persistent banner
   * (`error-states` spec: "Errors are reachable, not transient" — stays until
   * dismissed or superseded, never a fading toast) — independent of `status`, so
   * a failed second file never erases a previous successful one's `done` state. */
  private errorMessage: string | null = null;
  /** A large or binary file waiting for an explicit continue/cancel
   * (design.md D3/D4) before `openFile` actually starts parsing it. */
  private pendingConfirm: PendingConfirm | null = null;
  private progress: ParseProgress | null = null;
  private entriesEmitted = 0;
  private blankLines = 0;
  private totalLines = 0;
  private parseMs: number | null = null;

  // Owned here rather than by the timeline/table (design.md D7): the range the
  // timeline selects is passed down to the table through this shell, not wired
  // component-to-component. `filtering-and-search` extends this same state with
  // chips/search/URL-hash rather than inventing a second state owner.
  private entryIndex: EntryIndex | null = null;
  private activeRange: TimeRange | null = null;

  // filtering-and-search state (extends the D7 state model above, doesn't
  // replace it): fieldFilters/compiledQuery are the two other axes of the one
  // predicate (D2) alongside activeRange; formatOverride/tzOffsetMinutes are
  // carried through from the URL hash for round-tripping (`url-state` spec) —
  // there is no UI to set them interactively in this change.
  private fieldFilters: FieldFilters = new Map();
  private compiledQuery: CompiledQuery = { kind: "none" };
  private fieldInventory: FieldInventoryDto | null = null;
  private formatOverride: string | null = null;
  private tzOffsetMinutes: number | null = null;
  private readonly hashWriter = new HashWriter();
  /** Parsed once at connect from whatever hash the page loaded with; applied
   * exactly once, right after the first successful parse, then discarded — a
   * hash typed/pasted before any file is open describes "the view once a file is
   * opened", not an ongoing constraint (`url-state` spec, "Fresh tab reproduces
   * the view"). */
  private pendingHash: DecodedHash | null = null;

  private dropTarget!: LooqDropTarget;
  private detectionEl!: LooqDetection;
  private diagnosticsEl!: LooqDiagnostics;
  private timelineEl!: LooqTimeline;
  private tableEl!: LooqEntryTable;
  private filterBarEl!: LooqFilterBar;
  private statusEl!: HTMLParagraphElement;
  private errorBannerEl!: HTMLDivElement;
  private confirmBannerEl!: HTMLDivElement;
  private hashNoticeEl!: HTMLParagraphElement;
  private copyLinkBtn!: HTMLButtonElement;
  private copyStatusEl!: HTMLParagraphElement;

  connectedCallback(): void {
    const runMode = readRunMode();
    if (runMode.mode === "stdin") {
      // Live tail owns its own connection/render lifecycle entirely
      // (`components/looq-live-tail.ts`) — the file-mode state this class tracks
      // (status/progress/bridge) has no role in stdin mode (design.md open
      // question resolved in task 6.1: stdin mode replaces the view rather than
      // trying to also offer file-open at the same time — see docs/devlog.md).
      this.innerHTML = `<looq-live-tail max-lines="${runMode.maxLines}"></looq-live-tail>`;
      return;
    }

    const hintTemplate = document.getElementById("hint-template") as HTMLTemplateElement | null;
    const hintHtml = hintTemplate?.innerHTML.trim() ?? "";

    this.innerHTML = `
      <looq-drop-target></looq-drop-target>
      <p class="status" id="status"></p>
      <div class="error-banner" id="error-banner" hidden>
        <span id="error-banner-text"></span>
        <button type="button" id="error-banner-dismiss" aria-label="Dismiss">&times;</button>
      </div>
      <div class="confirm-banner" id="confirm-banner" hidden>
        <span id="confirm-banner-text"></span>
        <button type="button" id="confirm-continue">Continue</button>
        <button type="button" id="confirm-cancel">Cancel</button>
      </div>
      <div id="results" hidden>
        <looq-detection></looq-detection>
        <looq-diagnostics></looq-diagnostics>
        <p class="hash-notice" id="hash-notice" hidden></p>
        <looq-filter-bar></looq-filter-bar>
        <div class="share-link-row">
          <button type="button" id="copy-link">Copy shareable link</button>
          <p class="share-caveat" id="copy-status" hidden></p>
        </div>
        <looq-timeline></looq-timeline>
        <looq-entry-table></looq-entry-table>
      </div>
    `;

    this.dropTarget = this.querySelector("looq-drop-target") as LooqDropTarget;
    this.detectionEl = this.querySelector("looq-detection") as LooqDetection;
    this.diagnosticsEl = this.querySelector("looq-diagnostics") as LooqDiagnostics;
    this.timelineEl = this.querySelector("looq-timeline") as LooqTimeline;
    this.tableEl = this.querySelector("looq-entry-table") as LooqEntryTable;
    this.filterBarEl = this.querySelector("looq-filter-bar") as LooqFilterBar;
    this.statusEl = this.querySelector("#status") as HTMLParagraphElement;
    this.errorBannerEl = this.querySelector("#error-banner") as HTMLDivElement;
    this.confirmBannerEl = this.querySelector("#confirm-banner") as HTMLDivElement;
    this.hashNoticeEl = this.querySelector("#hash-notice") as HTMLParagraphElement;
    this.copyLinkBtn = this.querySelector("#copy-link") as HTMLButtonElement;
    this.copyStatusEl = this.querySelector("#copy-status") as HTMLParagraphElement;

    (this.querySelector("#error-banner-dismiss") as HTMLButtonElement).addEventListener("click", () => {
      this.errorMessage = null;
      this.renderError();
    });
    (this.querySelector("#confirm-continue") as HTMLButtonElement).addEventListener("click", () => {
      const pending = this.pendingConfirm;
      if (!pending) {
        return;
      }
      this.pendingConfirm = null;
      this.renderConfirm();
      void this.openFile(pending.file, { skipPreflight: true });
    });
    (this.querySelector("#confirm-cancel") as HTMLButtonElement).addEventListener("click", () => {
      this.pendingConfirm = null;
      this.renderConfirm();
    });

    // Read once, on connect (`url-state` spec, "A hash is applied on load") —
    // applied after the first successful parse (see `openFile`), since chips/
    // range need a dataset to validate and render against.
    if (location.hash.length > 1) {
      this.pendingHash = decodeHash(location.hash);
    }

    this.dropTarget.setHintHtml(hintHtml);
    this.dropTarget.addEventListener("file-selected", (event) => {
      const file = (event as CustomEvent<File>).detail;
      void this.openFile(file);
    });
    this.timelineEl.addEventListener("range-change", (event) => {
      this.setActiveRange((event as CustomEvent<TimeRange | null>).detail);
    });
    this.filterBarEl.addEventListener("filters-change", (event) => {
      const detail = (event as CustomEvent<FiltersChangeDetail>).detail;
      this.fieldFilters = detail.fieldFilters;
      this.compiledQuery = detail.compiledQuery;
      this.applyPredicate();
      this.scheduleHashWrite();
    });
    this.copyLinkBtn.addEventListener("click", () => this.copyShareableLink());

    this.renderStatus();
  }

  /** The one place `activeRange` is written (D7) — both consumers are told
   * through their own setters, never through a direct wire between them. */
  private setActiveRange(range: TimeRange | null): void {
    this.activeRange = range;
    this.timelineEl.setActiveRange(range);
    this.tableEl.setActiveRange(range);
    this.updateFilterCounts();
    this.scheduleHashWrite();
  }

  /** Recomputes `EntryIndex`'s predicate (D2/D9) from the current chips + query
   * and pushes the result to every reader — table, timeline, counts — so they
   * never see three different filtered views (`filtering` spec, "Table, timeline
   * and every count read from the same predicate result"). */
  private applyPredicate(): void {
    if (!this.entryIndex) {
      return;
    }
    const hasFilters = this.fieldFilters.size > 0 || this.compiledQuery.kind !== "none";
    this.entryIndex.setPredicate(
      hasFilters ? (e) => matchesFieldFilters(e, this.fieldFilters) && matchesQuery(e, this.compiledQuery) : null,
    );
    this.tableEl.setQuery(this.compiledQuery);
    this.tableEl.refreshFilters();
    this.timelineEl.refresh();
    this.updateFilterCounts();
  }

  /** The filter bar's "N of total" must agree with what the table actually shows
   * (D2) — when a time range is active, that means counting within the range too,
   * not just the predicate over the whole dataset, or the two visible counts on
   * screen at once would disagree about the same view. */
  private updateFilterCounts(): void {
    if (!this.entryIndex) {
      return;
    }
    const matching = this.activeRange
      ? this.entryIndex.countInRange(this.activeRange.startMs, this.activeRange.endMs)
      : this.entryIndex.matchingCount;
    this.filterBarEl.setCounts(matching, this.entryIndex.totalCount);
  }

  private scheduleHashWrite(): void {
    this.hashWriter.schedule({
      range: this.activeRange,
      fieldFilters: this.fieldFilters,
      query: this.filterBarEl?.getState().queryText ?? "",
      formatOverride: this.formatOverride,
      tzOffsetMinutes: this.tzOffsetMinutes,
    });
  }

  /** D8: the caveat is presented at the moment of copying, not only in the
   * README — `flush()` first so the copied URL reflects what's on screen right
   * now (design.md risk: debounce must not make a copied link stale). */
  private copyShareableLink(): void {
    this.hashWriter.flush();
    const caveat =
      "Copied — this link encodes your search text and filter values, which are fragments of the log itself.";
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

  /** Applies a hash decoded on load, once a dataset exists to validate it against
   * (`url-state` spec, "A malformed hash is reported, not ignored"): recognised
   * keys and known field names are applied, everything else is named in a visible
   * notice rather than silently dropped. */
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

    const known = knownFieldNames(this.fieldInventory);
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

    // `applyExternalState` triggers `filters-change`, which calls `applyPredicate`
    // and `scheduleHashWrite` — the normal path, not a special case.
    this.filterBarEl.applyExternalState(validFilters, decoded.query);
    if (decoded.range) {
      this.setActiveRange(decoded.range);
    }

    this.hashNoticeEl.hidden = notices.length === 0;
    this.hashNoticeEl.textContent =
      notices.length > 0 ? `From the shared URL, could not be applied: ${notices.join("; ")}.` : "";
  }

  /** `error-states` spec, "Failure does not destroy existing state": nothing here
   * touches `entryIndex`/`resultsEl` until a NEW parse has actually succeeded —
   * a failed second `openFile` call only ever populates `errorMessage`, never
   * clears what a previous successful call already produced. `skipPreflight`
   * lets the confirm-banner's "Continue" button resume a file past the
   * size/binary checks it already showed a prompt for, without re-prompting. */
  private async openFile(file: File, opts: { skipPreflight?: boolean } = {}): Promise<void> {
    this.errorMessage = null;
    this.renderError();

    // `error-states` spec, "Empty file": distinct message, checked before any of
    // the preflight/parse machinery below even starts.
    if (file.size === 0) {
      this.errorMessage = `${file.name} is empty — there is nothing to parse.`;
      this.renderError();
      return;
    }

    if (!opts.skipPreflight) {
      if (file.size > HARD_CAP_BYTES) {
        this.errorMessage =
          `${file.name} is ${formatBytes(file.size)}, above the ${formatBytes(HARD_CAP_BYTES)} limit looq ` +
          `enforces for browser memory (measured at roughly 3.4x a file's raw size once parsed and indexed — ` +
          `see TDR §14) — starting a parse this large risks an out-of-memory failure partway through instead ` +
          `of a clean refusal. Split the file (e.g. by time range) or filter it down before opening, then retry.`;
        this.renderError();
        return;
      }
      if (file.size > WARN_THRESHOLD_BYTES) {
        this.pendingConfirm = {
          file,
          message:
            `${file.name} is ${formatBytes(file.size)} — parsing a file this large can be slow and use ` +
            `significant browser memory. Continue?`,
        };
        this.renderConfirm();
        return;
      }

      // `error-states` spec, "Binary file" (design.md D4): sniff the first chunk
      // for a NUL byte before committing to a full parse. Also doubles as the
      // "Unreadable file" check (design.md's own framing) — `File.slice().
      // arrayBuffer()` throwing means the browser itself couldn't read the file
      // (e.g. permission revoked after selection), reported the same way.
      try {
        const head = await file.slice(0, SNIFF_BYTES).arrayBuffer();
        if (looksBinary(new Uint8Array(head))) {
          this.pendingConfirm = {
            file,
            message: `${file.name} doesn't look like a text log file (found binary data near the start). Proceed anyway?`,
          };
          this.renderConfirm();
          return;
        }
      } catch (err) {
        this.errorMessage = `couldn't read ${file.name}: ${String(err)}`;
        this.renderError();
        return;
      }
    }

    this.status = "parsing";
    this.currentFileName = file.name;
    this.progress = { bytesConsumed: 0, totalBytes: file.size, entriesSoFar: 0 };
    this.dropTarget.setDisabled(true);
    this.hashNoticeEl.hidden = true;
    this.copyStatusEl.hidden = true;
    this.renderStatus();

    // `pendingHash`'s format/tz apply to THIS parse (`url-state` spec, "Format
    // override from the hash") — read before the async parse call, not after,
    // since `applyPendingHash` (which would otherwise clear it) only runs once
    // the parse has produced a dataset to validate chips/range against.
    if (this.pendingHash) {
      this.formatOverride = this.pendingHash.formatOverride;
      this.tzOffsetMinutes = this.pendingHash.tzOffsetMinutes;
    }

    const start = performance.now();
    try {
      const parseOptions: { formatOverride?: string; tzOffsetMinutes?: number } = {};
      if (this.formatOverride !== null) {
        parseOptions.formatOverride = this.formatOverride;
      }
      if (this.tzOffsetMinutes !== null) {
        parseOptions.tzOffsetMinutes = this.tzOffsetMinutes;
      }
      const result = await this.bridge.parseFile(file, parseOptions, (progress) => {
        this.progress = progress;
        this.renderStatus();
      });
      this.parseMs = performance.now() - start;
      this.status = "done";
      this.entriesEmitted = result.entriesEmitted;
      this.blankLines = result.blankLines;
      this.totalLines = result.totalLines;

      this.detectionEl.setDetection(result.detection);
      this.diagnosticsEl.setDiagnostics(result.diagnostics, result.entriesEmitted);

      const index = new EntryIndex();
      index.append(result.entries);
      this.entryIndex = index;
      this.timelineEl.setIndex(index);
      this.tableEl.setIndex(index);

      // A previous file's filters/range must not leak into this one (same
      // reasoning as `setActiveRange(null)` below, extended to the other two
      // predicate axes): reset local UI state, then re-derive from the field
      // inventory this file actually produced.
      this.fieldInventory = result.fieldInventory;
      this.fieldFilters = new Map();
      this.compiledQuery = { kind: "none" };
      this.filterBarEl.reset();
      this.filterBarEl.setFieldInventory(result.fieldInventory, index.levelStats);
      this.applyPredicate();

      // A previous file's selection must not leak into this one either —
      // `setIndex` above resets each component's own scroll/zoom state, but
      // `activeRange` is shell state (D7), so the shell is what has to actively
      // clear it rather than relying on the components to reset a field they
      // don't own.
      this.setActiveRange(null);

      // Applied once, right here, before `#results` becomes visible — the hash's
      // filters/range are in effect from the very first paint the user sees
      // rather than a flash of the unfiltered view followed by a jump
      // (`url-state` spec, "A hash is applied on load").
      if (this.pendingHash) {
        this.applyPendingHash(this.pendingHash);
        this.pendingHash = null;
      }

      this.resultsEl().hidden = false;
    } catch (err) {
      if (err instanceof ParseCancelledError) {
        // A newer file superseded this one; that file's own openFile() call owns
        // the status now — don't overwrite it with a stale error.
        return;
      }
      // `error-states` spec, "Failure does not destroy existing state" /
      // "WASM module load failure": revert to whatever was true before this
      // attempt — the previous file's `done` state if there was one, `unopened`
      // otherwise — and report the failure only in the separate error banner,
      // never by replacing the status line a previous success left behind.
      this.status = this.entryIndex ? "done" : "unopened";
      this.errorMessage =
        err instanceof WorkerInitError
          ? `the WASM parser failed to load: ${err.message}`
          : `failed to parse ${file.name}: ${String(err)}`;
    } finally {
      this.dropTarget.setDisabled(false);
      this.renderStatus();
      this.renderError();
    }
  }

  private resultsEl(): HTMLElement {
    return this.querySelector("#results") as HTMLElement;
  }

  private renderStatus(): void {
    switch (this.status) {
      case "unopened":
        this.statusEl.textContent = "";
        this.statusEl.className = "status";
        break;
      case "parsing": {
        const pct = this.progress && this.progress.totalBytes > 0
          ? Math.round((this.progress.bytesConsumed / this.progress.totalBytes) * 100)
          : 0;
        this.statusEl.textContent = `Parsing ${this.currentFileName}... ${pct}% (${this.progress?.entriesSoFar ?? 0} entries so far)`;
        this.statusEl.className = "status parsing";
        break;
      }
      case "done": {
        const timing = this.parseMs !== null ? ` in ${this.parseMs.toFixed(1)}ms` : "";
        if (this.entriesEmitted === 0 && this.totalLines > 0) {
          // `error-states` spec, "A format that produced no entries reads as
          // such, distinct from an empty file [and from a filter matching
          // nothing]": every line was read (this.totalLines > 0, so the file
          // itself was not empty — that's caught earlier in `openFile`), but
          // none of them produced a recognisable entry.
          this.statusEl.textContent =
            `${this.currentFileName}: read ${this.totalLines} line(s), but none matched a known log ` +
            `format (0 entries)${timing} — try a different file, or override detection with #format=.`;
          this.statusEl.className = "status done zero-entries-warning";
        } else {
          this.statusEl.textContent =
            `${this.currentFileName}: ${this.entriesEmitted} entries, ${this.blankLines} blank lines, ` +
            `${this.totalLines} total lines${timing}`;
          this.statusEl.className = "status done";
        }
        break;
      }
    }
  }

  /** `error-states` spec, "Errors are reachable, not transient": a persistent
   * banner, independent of `renderStatus()`'s status line, so a failed second
   * `openFile` never overwrites what a previous success already displayed there. */
  private renderError(): void {
    this.errorBannerEl.hidden = this.errorMessage === null;
    (this.querySelector("#error-banner-text") as HTMLSpanElement).textContent = this.errorMessage ?? "";
  }

  /** design.md D3/D4: the large-file and binary-file confirm prompts share one
   * banner and one continue/cancel pair, resolved via `pendingConfirm`. */
  private renderConfirm(): void {
    this.confirmBannerEl.hidden = this.pendingConfirm === null;
    (this.querySelector("#confirm-banner-text") as HTMLSpanElement).textContent =
      this.pendingConfirm?.message ?? "";
  }
}

customElements.define("looq-app", LooqApp);
