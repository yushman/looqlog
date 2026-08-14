import { describe, expect, it } from "vitest";

import { bucketCountForSpan, pickBucketWidthMs, TARGET_BUCKET_COUNT } from "./timeline-bucket";

describe("pickBucketWidthMs", () => {
  it("picks a round interval for a ~10 minute span (timeline spec)", () => {
    const width = pickBucketWidthMs(10 * 60 * 1000);
    expect([5000, 10000]).toContain(width);
  });

  it("never exceeds the target bucket count", () => {
    for (const spanMs of [1000, 60_000, 3_600_000, 86_400_000, 30 * 86_400_000, 400 * 86_400_000]) {
      const width = pickBucketWidthMs(spanMs);
      const count = bucketCountForSpan(spanMs, width);
      expect(count).toBeLessThanOrEqual(TARGET_BUCKET_COUNT);
    }
  });

  it("picks the smallest ladder rung that fits, not an oversized one", () => {
    // 120 seconds at target 120 → 1s buckets fit exactly (120/1s = 120 <= 120).
    expect(pickBucketWidthMs(120_000, 120)).toBe(1000);
  });
});
