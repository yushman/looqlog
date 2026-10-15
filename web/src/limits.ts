// File-size thresholds for uploads (`error-states` spec). The backend reports its own
// numbers with the source (`SourceInfoDto.warnBytes`/`capBytes`, set in
// `crates/looqlog/src/source.rs`); these mirror them for the moment before the first
// reply arrives. Both are derived from the measured worst case of ~10.24 bytes of
// retained heap per input byte (logfmt, `mem_probe -- session`, docs/devlog.md).

/** Above this, the user is warned before uploading and can continue or cancel. */
export const WARN_THRESHOLD_BYTES = 50 * 1024 * 1024;

/** Above this, the upload is refused: ~4.1 GB of retained heap at the worst case. */
export const HARD_CAP_BYTES = 400 * 1024 * 1024;

export function formatBytes(bytes: number): string {
  if (bytes < 1024) {
    return `${bytes} B`;
  }
  const units = ["KB", "MB", "GB"];
  let value = bytes / 1024;
  let unitIndex = 0;
  while (value >= 1024 && unitIndex < units.length - 1) {
    value /= 1024;
    unitIndex += 1;
  }
  return `${value.toFixed(1)} ${units[unitIndex]}`;
}
