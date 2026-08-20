// Presentational: a file picker + drop target. Owns no parse state (design.md D7) —
// it only reads a `File` from a user gesture and dispatches it upward. Preserves
// ADR-0007 (the CLI path is a hint, not an auto-loaded file — the user still has to
// pick it). The ADR-0002 no-network guarantee is stated by the shell's own privacy
// surface, which is worded per mode (TDR §12) and therefore cannot live in a
// component only file mode mounts.

export class LooqlogDropTarget extends HTMLElement {
  private hintHtml = "";
  private disabled = false;

  setHintHtml(html: string): void {
    this.hintHtml = html;
    this.render();
  }

  setDisabled(disabled: boolean): void {
    this.disabled = disabled;
    const input = this.querySelector<HTMLInputElement>("#file-picker");
    if (input) {
      input.disabled = disabled;
    }
  }

  connectedCallback(): void {
    this.render();
  }

  private render(): void {
    this.innerHTML = `
      <div class="hint">${this.hintHtml || "Open a log file below."}</div>
      <div id="drop-zone" class="drop-zone">
        <p>Drag and drop a log file here, or</p>
        <input type="file" id="file-picker" />
      </div>
    `;

    const dropZone = this.querySelector<HTMLDivElement>("#drop-zone");
    const picker = this.querySelector<HTMLInputElement>("#file-picker");
    if (!dropZone || !picker) {
      return;
    }

    picker.addEventListener("change", () => {
      const file = picker.files?.[0];
      if (file) {
        this.emitFile(file);
      }
    });

    dropZone.addEventListener("dragover", (event) => {
      event.preventDefault();
      dropZone.classList.add("drag");
    });
    dropZone.addEventListener("dragleave", () => dropZone.classList.remove("drag"));
    dropZone.addEventListener("drop", (event) => {
      event.preventDefault();
      dropZone.classList.remove("drag");
      const file = event.dataTransfer?.files?.[0];
      if (file) {
        this.emitFile(file);
      }
    });
  }

  private emitFile(file: File): void {
    this.dispatchEvent(new CustomEvent<File>("file-selected", { detail: file, bubbles: true }));
  }
}

customElements.define("looqlog-drop-target", LooqlogDropTarget);
