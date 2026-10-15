// The landing's source list (`source-registry` spec): server snapshots replace the
// list, a close in flight is remembered until the next snapshot drops or keeps the row.

import { formatBytes } from "./limits";
import type { SourceRowDto } from "./query-types";
import { sourcePath } from "./routing";

export interface LandingState {
  /** `null` until the first snapshot arrives. */
  sources: SourceRowDto[] | null;
  closing: ReadonlySet<string>;
}

export type LandingAction =
  | { type: "sources"; sources: SourceRowDto[] }
  | { type: "closing"; id: string }
  | { type: "close_failed"; id: string };

export const INITIAL_LANDING: LandingState = { sources: null, closing: new Set() };

export function reduceLanding(state: LandingState, action: LandingAction): LandingState {
  switch (action.type) {
    case "sources": {
      const ids = new Set(action.sources.map((s) => s.id));
      return { sources: action.sources, closing: new Set([...state.closing].filter((id) => ids.has(id))) };
    }
    case "closing":
      return { ...state, closing: new Set([...state.closing, action.id]) };
    case "close_failed":
      return { ...state, closing: new Set([...state.closing].filter((id) => id !== action.id)) };
  }
}

export interface LandingRow {
  id: string;
  name: string;
  kind: string;
  size: string;
  state: string;
  tabs: string;
  href: string;
  /** Stdin cannot be closed while it is being read. */
  closable: boolean;
  closing: boolean;
  error: string | null;
}

const KIND_LABEL: Record<SourceRowDto["kind"], string> = { argv: "file", stdin: "stdin", upload: "upload" };

function stateLabel(row: SourceRowDto): string {
  if (row.state === "ended") {
    return row.kind === "stdin" ? "ended" : "loaded";
  }
  return row.state;
}

export function landingRows(state: LandingState): LandingRow[] {
  return (state.sources ?? []).map((row) => ({
    id: row.id,
    name: row.name,
    kind: KIND_LABEL[row.kind],
    size: formatBytes(row.size),
    state: stateLabel(row),
    tabs: row.tabs === 1 ? "1 tab" : `${row.tabs} tabs`,
    href: sourcePath(row.id),
    closable: row.kind !== "stdin",
    closing: state.closing.has(row.id),
    error: row.error,
  }));
}
