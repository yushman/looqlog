// The landing at `/` (`source-registry` spec): the process's sources, kept live over a
// `/ws` connection that names no source, a close control for each except stdin, and an
// upload that opens the new source in this tab. It never shows a log itself.

import "./looqlog-uploader";

import { INITIAL_LANDING, landingRows, reduceLanding, type LandingAction, type LandingState } from "../landing-list";
import { QueryClient, socketUrl, type ConnectionState } from "../query-client";
import type { Notification } from "../query-types";
import { readAuthToken } from "../token";

export class LooqlogLanding extends HTMLElement {
  private client: QueryClient | null = null;
  private state: LandingState = INITIAL_LANDING;
  private listEl!: HTMLTableSectionElement;
  private emptyEl!: HTMLParagraphElement;
  private noteEl!: HTMLParagraphElement;

  connectedCallback(): void {
    this.innerHTML = `
      <div class="landing">
        <section class="landing-section">
          <h2>Sources</h2>
          <p class="provisional-note conn-note" hidden></p>
          <p class="landing-empty">Connecting…</p>
          <table class="landing-sources" hidden>
            <thead>
              <tr><th>Name</th><th>Kind</th><th>Size</th><th>State</th><th>Open tabs</th><th></th></tr>
            </thead>
            <tbody></tbody>
          </table>
        </section>
        <section class="landing-section">
          <h2>Open a log file</h2>
          <looqlog-uploader></looqlog-uploader>
          <p class="privacy-note">
            A file picked or dropped here is uploaded to the local <code>looqlog</code>
            process, not to any server elsewhere, and opens in this tab. An upload with no
            tab open for 10 minutes is unloaded.
          </p>
        </section>
      </div>
    `;
    this.listEl = this.querySelector("tbody") as HTMLTableSectionElement;
    this.emptyEl = this.querySelector(".landing-empty") as HTMLParagraphElement;
    this.noteEl = this.querySelector(".conn-note") as HTMLParagraphElement;

    const client = new QueryClient({
      url: socketUrl(),
      token: readAuthToken(),
      source: null,
      onState: (s) => this.onConnState(s),
      onNotification: (n) => this.onNotification(n),
      onConnected: () => undefined,
    });
    this.client = client;
    client.connect();
  }

  disconnectedCallback(): void {
    this.client?.dispose();
    this.client = null;
  }

  private dispatch(action: LandingAction): void {
    this.state = reduceLanding(this.state, action);
    this.render();
  }

  private onConnState(state: ConnectionState): void {
    this.noteEl.hidden = state !== "disconnected";
    this.noteEl.textContent =
      state === "disconnected"
        ? "The connection to looqlog was lost — retrying. If the looqlog process has exited, start it again."
        : "";
  }

  private onNotification(n: Notification): void {
    if (n.type === "sources") {
      this.dispatch({ type: "sources", sources: n.sources });
    }
  }

  private async close(id: string): Promise<void> {
    this.dispatch({ type: "closing", id });
    try {
      await this.client?.request({ type: "close", source: id });
    } catch (err) {
      this.dispatch({ type: "close_failed", id });
      this.noteEl.hidden = false;
      this.noteEl.textContent = `could not close the source: ${err instanceof Error ? err.message : String(err)}`;
    }
  }

  private render(): void {
    const rows = landingRows(this.state);
    const table = this.querySelector(".landing-sources") as HTMLTableElement;
    table.hidden = rows.length === 0;
    this.emptyEl.hidden = rows.length > 0;
    this.emptyEl.textContent =
      this.state.sources === null ? "Connecting…" : "No sources yet — open a log file below.";
    this.listEl.replaceChildren(
      ...rows.map((row) => {
        const tr = document.createElement("tr");
        const link = document.createElement("a");
        link.href = row.href;
        link.textContent = row.name;
        const nameCell = document.createElement("td");
        nameCell.appendChild(link);
        if (row.error) {
          const error = document.createElement("div");
          error.className = "landing-error";
          error.textContent = row.error;
          nameCell.appendChild(error);
        }
        tr.appendChild(nameCell);
        for (const text of [row.kind, row.size, row.state, row.tabs]) {
          const td = document.createElement("td");
          td.textContent = text;
          tr.appendChild(td);
        }
        const actions = document.createElement("td");
        if (row.closable) {
          const button = document.createElement("button");
          button.type = "button";
          button.textContent = row.closing ? "Closing…" : "Close";
          button.disabled = row.closing;
          button.addEventListener("click", () => void this.close(row.id));
          actions.appendChild(button);
        } else {
          actions.title = "stdin cannot be closed while it is being read";
        }
        tr.appendChild(actions);
        return tr;
      }),
    );
  }
}

customElements.define("looqlog-landing", LooqlogLanding);
