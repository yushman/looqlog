// Uploading a picked or dropped file as a new source (`file-ingest` spec). Checks that
// need no upload — empty, above the cap, large or binary-looking — run first.

import { looksBinary, SNIFF_BYTES } from "./binary-detect";
import { formatBytes, HARD_CAP_BYTES, WARN_THRESHOLD_BYTES } from "./limits";
import { readAuthToken } from "./token";

export type Preflight = { kind: "ok" } | { kind: "error"; message: string } | { kind: "confirm"; message: string };

export async function preflight(
  file: File,
  capBytes = HARD_CAP_BYTES,
  warnBytes = WARN_THRESHOLD_BYTES,
): Promise<Preflight> {
  if (file.size === 0) {
    return { kind: "error", message: `${file.name} is empty — there is nothing to parse.` };
  }
  if (file.size > capBytes) {
    return {
      kind: "error",
      message:
        `${file.name} is ${formatBytes(file.size)}, above the ${formatBytes(capBytes)} limit: the looqlog ` +
        `process cannot hold the index for a file that size (measured at up to ~10x the file size in ` +
        `memory). Split the file (e.g. by time range) or filter it down before opening, then retry.`,
    };
  }
  if (file.size > warnBytes) {
    return {
      kind: "confirm",
      message:
        `${file.name} is ${formatBytes(file.size)} — parsing a file this large takes a while and the ` +
        `looqlog process may use several times that in memory. Continue?`,
    };
  }
  try {
    const head = await file.slice(0, SNIFF_BYTES).arrayBuffer();
    if (looksBinary(new Uint8Array(head))) {
      return {
        kind: "confirm",
        message: `${file.name} doesn't look like a text log file (found binary data near the start). Proceed anyway?`,
      };
    }
  } catch (err) {
    return { kind: "error", message: `couldn't read ${file.name}: ${String(err)}` };
  }
  return { kind: "ok" };
}

/** Streams `file` to `POST /upload`; resolves with the new source's id. */
export async function uploadFile(file: File): Promise<string> {
  let response: Response;
  try {
    response = await fetch("/upload", {
      method: "POST",
      headers: {
        "Content-Type": "application/octet-stream",
        "X-Looqlog-Token": readAuthToken(),
        "X-Looqlog-Filename": encodeURIComponent(file.name),
      },
      body: file,
    });
  } catch (err) {
    throw new Error(`could not upload ${file.name} to looqlog: ${String(err)}`);
  }
  const body = (await response.json().catch(() => ({}))) as { id?: string; error?: string };
  if (!response.ok || typeof body.id !== "string") {
    throw new Error(`could not open ${file.name}: ${body.error ?? `HTTP ${response.status}`}`);
  }
  return body.id;
}
