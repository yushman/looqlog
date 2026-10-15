// The upload flow shared by the landing and a gone source's tab (`browser-file-loading`
// spec): a picker/drop target, the continue/cancel prompt for a large or binary-looking
// file, a persistent error, and on success a navigation to the new source's URL. A tab
// that shows a source never replaces it in place.

import "./looqlog-drop-target";

import { sourcePath } from "../routing";
import { preflight, uploadFile } from "../upload";
import type { LooqlogDropTarget } from "./looqlog-drop-target";

export class LooqlogUploader extends HTMLElement {
  private dropTarget!: LooqlogDropTarget;
  private errorEl!: HTMLDivElement;
  private confirmEl!: HTMLDivElement;
  private statusEl!: HTMLParagraphElement;
  private pending: File | null = null;

  connectedCallback(): void {
    if (this.dropTarget) {
      return;
    }
    this.innerHTML = `
      <div class="error-banner" hidden>
        <span class="error-text"></span>
        <button type="button" class="error-dismiss" aria-label="Dismiss">&times;</button>
      </div>
      <div class="confirm-banner" hidden>
        <span class="confirm-text"></span>
        <button type="button" class="confirm-continue">Continue</button>
        <button type="button" class="confirm-cancel">Cancel</button>
      </div>
      <p class="status parsing" hidden></p>
    `;
    this.errorEl = this.querySelector(".error-banner") as HTMLDivElement;
    this.confirmEl = this.querySelector(".confirm-banner") as HTMLDivElement;
    this.statusEl = this.querySelector(".status") as HTMLParagraphElement;
    this.dropTarget = document.createElement("looqlog-drop-target") as LooqlogDropTarget;
    this.appendChild(this.dropTarget);
    this.dropTarget.setHintHtml("Open a log file below.");

    this.dropTarget.addEventListener("file-selected", (event) => {
      event.stopPropagation();
      void this.open((event as CustomEvent<File>).detail, false);
    });
    (this.querySelector(".error-dismiss") as HTMLButtonElement).addEventListener("click", () => this.showError(null));
    (this.querySelector(".confirm-continue") as HTMLButtonElement).addEventListener("click", () => {
      const file = this.pending;
      this.showConfirm(null);
      if (file) {
        void this.open(file, true);
      }
    });
    (this.querySelector(".confirm-cancel") as HTMLButtonElement).addEventListener("click", () => this.showConfirm(null));
  }

  private async open(file: File, confirmed: boolean): Promise<void> {
    this.showError(null);
    if (!confirmed) {
      const check = await preflight(file);
      if (check.kind === "error") {
        this.showError(check.message);
        return;
      }
      if (check.kind === "confirm") {
        this.pending = file;
        this.showConfirm(check.message);
        return;
      }
    }
    this.dropTarget.setDisabled(true);
    this.statusEl.hidden = false;
    this.statusEl.textContent = `Uploading and parsing ${file.name}…`;
    try {
      const id = await uploadFile(file);
      location.assign(sourcePath(id));
    } catch (err) {
      this.showError(err instanceof Error ? err.message : String(err));
    } finally {
      this.dropTarget.setDisabled(false);
      this.statusEl.hidden = true;
    }
  }

  private showError(message: string | null): void {
    this.errorEl.hidden = message === null;
    (this.querySelector(".error-text") as HTMLSpanElement).textContent = message ?? "";
  }

  private showConfirm(message: string | null): void {
    if (message === null) {
      this.pending = null;
    }
    this.confirmEl.hidden = message === null;
    (this.querySelector(".confirm-text") as HTMLSpanElement).textContent = message ?? "";
  }
}

customElements.define("looqlog-uploader", LooqlogUploader);
