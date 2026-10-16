export function resumeLabel(newCount: number): string {
  return newCount > 0 ? `Resume following · ${newCount.toLocaleString("en-US")} new` : "Resume following";
}

export function newSincePause(seenNow: number, seenAtPause: number): number {
  return Math.max(0, seenNow - seenAtPause);
}
