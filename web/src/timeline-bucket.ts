// Bucket-width ladder for the timeline histogram (design.md D3, `timeline` spec
// "Bucket width is readable"). Dividing a span into a fixed *number* of buckets
// produces widths like 3.7 seconds — unreadable axis labels, and boundaries that
// move every time a new entry extends the span. Instead the width is drawn from a
// fixed ladder of human-readable intervals, picking the smallest rung that keeps
// the bucket count under a target.

const SECOND = 1000;
const MINUTE = 60 * SECOND;
const HOUR = 60 * MINUTE;
const DAY = 24 * HOUR;

/** Target bucket count driving the ladder choice (task 4.3, an open question in
 * design.md resolved here): a fixed number rather than one derived from the
 * timeline's pixel width. The timeline is rendered at a fixed, unstyled width in
 * this change (release-hardening owns responsive layout), and a fixed target keeps
 * bucket selection independent of any particular render's DOM measurements —
 * simpler, and it's what `uPlot` needs before its first layout pass anyway. 120
 * keeps individual bars visible at typical viewport widths without the axis
 * turning into a wall of ticks. */
export const TARGET_BUCKET_COUNT = 120;

const LADDER_MS: readonly number[] = [
  1 * SECOND,
  5 * SECOND,
  10 * SECOND,
  30 * SECOND,
  1 * MINUTE,
  5 * MINUTE,
  10 * MINUTE,
  30 * MINUTE,
  1 * HOUR,
  3 * HOUR,
  6 * HOUR,
  12 * HOUR,
  1 * DAY,
  7 * DAY,
  30 * DAY,
  365 * DAY,
];

/** Picks the smallest ladder width whose bucket count over `spanMs` stays under
 * `targetCount`. Falls back to a multiple of the largest rung for spans so large
 * even that overflows (millennia of log data). */
export function pickBucketWidthMs(spanMs: number, targetCount: number = TARGET_BUCKET_COUNT): number {
  const safeSpan = Math.max(spanMs, 1);
  for (const width of LADDER_MS) {
    if (safeSpan / width <= targetCount) {
      return width;
    }
  }
  const largest = LADDER_MS[LADDER_MS.length - 1]!; // LADDER_MS is a non-empty literal
  const multiple = Math.max(1, Math.ceil(safeSpan / (largest * targetCount)));
  return largest * multiple;
}

export function bucketCountForSpan(spanMs: number, bucketWidthMs: number): number {
  return Math.max(1, Math.ceil(spanMs / bucketWidthMs));
}
