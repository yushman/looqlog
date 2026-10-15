// Which page a tab shows, from its path (`app-shell` spec, design.md D4): `/` is the
// landing, `/l/<id>` is one source. The server serves the same page for both.

import type { CloseReason } from "./query-types";

export type Route = { kind: "landing" } | { kind: "source"; id: string };

export function routeFor(pathname: string): Route {
  const match = /^\/l\/([^/]+)\/?$/.exec(pathname);
  return match ? { kind: "source", id: decodeURIComponent(match[1]!) } : { kind: "landing" };
}

export function sourcePath(id: string): string {
  return `/l/${encodeURIComponent(id)}`;
}

/** What a tab says when its source is gone, per reason (`query-api` spec). */
export function goneMessage(reason: CloseReason): string {
  switch (reason) {
    case "unknown":
      return "This source is not open in this looqlog process: the link is from an earlier run, or another process.";
    case "closed":
      return "This source was closed.";
    case "unloaded":
      return "This upload was unloaded after 10 minutes with no tab open.";
    case "budget":
      return "This upload was unloaded to stay within the looqlog memory budget.";
  }
}
