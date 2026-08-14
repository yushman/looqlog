// Binary-file detection (`error-states` spec, design.md D4): a NUL byte anywhere in
// the first chunk is a strong, cheap signal that a file is not text — good enough to
// warn and offer an override, not to hard-refuse outright. A log with one stray NUL
// byte in an otherwise-text file is a real (if rare) case, so the default matters
// more than the check itself: warn, then let the user proceed anyway.

/** How much of the file to sniff — enough to catch a binary file reliably without
 * reading anything close to the whole thing first. */
export const SNIFF_BYTES = 8192;

export function looksBinary(bytes: Uint8Array): boolean {
  for (let i = 0; i < bytes.length; i += 1) {
    if (bytes[i] === 0) {
      return true;
    }
  }
  return false;
}
