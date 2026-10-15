//! Bucket-width ladder for the timeline histogram (`timeline` spec). Ported from the
//! former `web/src/timeline-bucket.ts`. Dividing a span into a fixed *number* of
//! buckets gives widths like 3.7 seconds; the width is instead the smallest rung of a
//! human-readable ladder that keeps the bucket count under a target.

const SECOND: i64 = 1000;
const MINUTE: i64 = 60 * SECOND;
const HOUR: i64 = 60 * MINUTE;
const DAY: i64 = 24 * HOUR;

/// A fixed target rather than one derived from the chart's pixel width, so bucket
/// selection does not depend on any particular render's layout.
pub const TARGET_BUCKET_COUNT: usize = 120;

const LADDER_MS: [i64; 16] = [
    SECOND,
    5 * SECOND,
    10 * SECOND,
    30 * SECOND,
    MINUTE,
    5 * MINUTE,
    10 * MINUTE,
    30 * MINUTE,
    HOUR,
    3 * HOUR,
    6 * HOUR,
    12 * HOUR,
    DAY,
    7 * DAY,
    30 * DAY,
    365 * DAY,
];

/// The smallest ladder width whose bucket count over `span_ms` stays within
/// `target_count`; a multiple of the largest rung for spans even that overflows.
pub fn pick_bucket_width_ms(span_ms: i64, target_count: usize) -> i64 {
    let span = span_ms.max(1);
    let target = target_count.max(1) as i64;
    for width in LADDER_MS {
        if span <= width * target {
            return width;
        }
    }
    let largest = LADDER_MS[LADDER_MS.len() - 1];
    let multiple = ((span + largest * target - 1) / (largest * target)).max(1);
    largest * multiple
}

pub fn bucket_count_for_span(span_ms: i64, bucket_width_ms: i64) -> usize {
    ((span_ms + bucket_width_ms - 1) / bucket_width_ms).max(1) as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_a_round_interval_for_a_ten_minute_span() {
        assert!([5000, 10000].contains(&pick_bucket_width_ms(10 * MINUTE, TARGET_BUCKET_COUNT)));
    }

    #[test]
    fn never_exceeds_the_target_bucket_count() {
        for span in [
            SECOND,
            MINUTE,
            HOUR,
            DAY,
            30 * DAY,
            400 * DAY,
            1_000_000 * DAY,
        ] {
            let width = pick_bucket_width_ms(span, TARGET_BUCKET_COUNT);
            assert!(
                bucket_count_for_span(span, width) <= TARGET_BUCKET_COUNT,
                "{span}"
            );
        }
    }

    #[test]
    fn picks_the_smallest_rung_that_fits() {
        assert_eq!(pick_bucket_width_ms(120_000, 120), 1000);
    }
}
