//! Format auto-detection (format-detection spec, design.md D3). Samples at most the
//! first 100 non-empty lines and evaluates candidates in TDR §8 priority order —
//! JSON, then logfmt — selecting the first to cross an 80% match threshold; plain
//! text is the fallback, never rejected.

use crate::format::Format;
use crate::parsers::{json, logfmt};
use crate::timestamp::{self, ParseContext, TimestampShape};

/// Lines sampled for detection.
pub const SAMPLE_SIZE: usize = 100;

/// Fraction of sampled lines that must match a candidate format for it to win
/// (design.md D3: a judgement call, reported alongside every result so a wrong
/// threshold is visible in practice rather than inferred).
pub const THRESHOLD: f64 = 0.8;

/// Whether the chosen format crossed the threshold outright, or plain text was
/// selected because nothing did.
///
/// Plain text reaches `Threshold` too, when the prefix scanner recognises a timestamp
/// in at least `THRESHOLD` of the sampled lines (`prefix-and-payload-parsing` design.md
/// D9). Before that change every plain-text input was a `Fallback`, and the UI renders
/// a fallback as a warning — which would now fire on syslog, klog and access logs that
/// parsed perfectly well.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetectionOutcome {
    Threshold,
    Fallback,
}

/// The result of running detection: which format, how well it matched, and whether
/// that was a real threshold win or the plain-text fallback (format-detection spec,
/// "The detection result is reported, not hidden").
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DetectionResult {
    pub format: Format,
    pub match_fraction: f64,
    pub outcome: DetectionOutcome,
    /// The timestamp shape that matched most of the sample, and the head offset it
    /// matched at (format-detection spec, "Detection fixes the prefix shape for the
    /// input"). `None` for structured formats and for input with no recognised
    /// prefixes at all. The parser tries this choice first per line (design.md D3);
    /// it is an ordering hint only and never changes which entries are produced.
    pub timestamp_shape: Option<TimestampShape>,
    pub timestamp_offset: Option<usize>,
}

fn fraction_matching(format: Format, lines: &[&str]) -> f64 {
    let matches = lines
        .iter()
        .filter(|line| match format {
            Format::Json => json::matches(line),
            Format::Logfmt => logfmt::matches(line),
            Format::Plain => true,
        })
        .count();
    matches as f64 / lines.len() as f64
}

/// How many sampled lines carry a recognisable timestamp prefix, and which
/// shape/offset the most of them used. The winner is the modal choice rather than the
/// first one seen, so one odd line at the top of a file cannot fix the hint for the
/// whole input.
fn prefix_evidence(lines: &[&str], ctx: &ParseContext) -> (f64, Option<(TimestampShape, usize)>) {
    let mut matched = 0usize;
    let mut counts: Vec<((TimestampShape, usize), usize)> = Vec::new();
    for line in lines {
        if let Some(choice) = timestamp::prefix_shape(line, ctx) {
            matched += 1;
            match counts.iter_mut().find(|(seen, _)| *seen == choice) {
                Some((_, count)) => *count += 1,
                None => counts.push((choice, 1)),
            }
        }
    }
    let winner = counts
        .into_iter()
        .max_by_key(|(_, count)| *count)
        .map(|(choice, _)| choice);
    (matched as f64 / lines.len() as f64, winner)
}

/// Run detection over a sample of non-empty lines (already filtered by the caller;
/// at most `SAMPLE_SIZE` of them per the spec, though this function itself does not
/// enforce the cap — the incremental parser stops collecting once it has enough).
pub fn detect(sample: &[&str], ctx: &ParseContext) -> DetectionResult {
    if sample.is_empty() {
        return DetectionResult {
            format: Format::Plain,
            match_fraction: 0.0,
            outcome: DetectionOutcome::Fallback,
            timestamp_shape: None,
            timestamp_offset: None,
        };
    }

    let json_fraction = fraction_matching(Format::Json, sample);
    if json_fraction >= THRESHOLD {
        return DetectionResult {
            format: Format::Json,
            match_fraction: json_fraction,
            outcome: DetectionOutcome::Threshold,
            timestamp_shape: None,
            timestamp_offset: None,
        };
    }

    let logfmt_fraction = fraction_matching(Format::Logfmt, sample);
    if logfmt_fraction >= THRESHOLD {
        return DetectionResult {
            format: Format::Logfmt,
            match_fraction: logfmt_fraction,
            outcome: DetectionOutcome::Threshold,
            timestamp_shape: None,
            timestamp_offset: None,
        };
    }

    // Plain text is never rejected, but it is no longer automatically a *fallback*:
    // a file of syslog or access-log lines whose prefixes all parse is a match on its
    // own terms (design.md D9).
    let (prefix_fraction, choice) = prefix_evidence(sample, ctx);
    let outcome = if prefix_fraction >= THRESHOLD {
        DetectionOutcome::Threshold
    } else {
        DetectionOutcome::Fallback
    };
    DetectionResult {
        format: Format::Plain,
        // Evidence for the best candidate seen, whichever it was — with plain text
        // winning on the threshold, that is `prefix_fraction` by construction.
        match_fraction: json_fraction.max(logfmt_fraction).max(prefix_fraction),
        outcome,
        timestamp_shape: choice.map(|(shape, _)| shape),
        timestamp_offset: choice.map(|(_, offset)| offset),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, Utc};

    fn ctx() -> ParseContext {
        ParseContext::utc().with_reference(
            DateTime::parse_from_rfc3339("2026-08-20T00:00:00Z")
                .unwrap()
                .with_timezone(&Utc),
        )
    }

    #[test]
    fn mostly_json_with_noise_selects_json() {
        let mut lines: Vec<&str> = vec![r#"{"a":1}"#; 95];
        let banners = vec!["starting up"; 5];
        lines.extend(banners);
        let result = detect(&lines, &ctx());
        assert_eq!(result.format, Format::Json);
        assert_eq!(result.outcome, DetectionOutcome::Threshold);
    }

    #[test]
    fn ambiguous_input_falls_back_to_plain() {
        let mut lines: Vec<&str> = vec!["level=info msg=x"; 50];
        lines.extend(vec!["just some free text here"; 50]);
        let result = detect(&lines, &ctx());
        assert_eq!(result.format, Format::Plain);
        assert_eq!(result.outcome, DetectionOutcome::Fallback);
    }

    #[test]
    fn no_candidate_reaches_threshold_selects_plain_without_failing() {
        let lines: Vec<&str> = vec!["free text one", "free text two"];
        let result = detect(&lines, &ctx());
        assert_eq!(result.format, Format::Plain);
        assert_eq!(result.outcome, DetectionOutcome::Fallback);
    }

    #[test]
    fn result_carries_its_evidence() {
        let mut lines: Vec<&str> = vec!["level=info msg=x service=y"; 88];
        lines.extend(vec!["free text with no pairs"; 12]);
        let result = detect(&lines, &ctx());
        assert_eq!(result.format, Format::Logfmt);
        assert!((result.match_fraction - 0.88).abs() < 1e-9);
        assert_eq!(result.outcome, DetectionOutcome::Threshold);
    }

    // --- prefixed plain text (task 7.1, 7.2) ---------------------------------

    #[test]
    fn recognised_prefixes_are_a_match_not_a_fallback() {
        let mut lines: Vec<&str> = vec!["Aug  8 17:42:01 host app[123]: connection refused"; 95];
        lines.extend(vec!["startup banner with no timestamp"; 5]);
        let result = detect(&lines, &ctx());
        assert_eq!(result.format, Format::Plain);
        assert_eq!(result.outcome, DetectionOutcome::Threshold);
        assert!((result.match_fraction - 0.95).abs() < 1e-9);
        assert_eq!(result.timestamp_shape, Some(TimestampShape::Syslog3164));
        assert_eq!(result.timestamp_offset, Some(0));
    }

    #[test]
    fn access_log_records_its_head_offset() {
        let lines: Vec<&str> =
            vec![r#"1.2.3.4 - - [08/Aug/2026:17:42:01 +0000] "GET /x" 200 12"#; 10];
        let result = detect(&lines, &ctx());
        assert_eq!(result.outcome, DetectionOutcome::Threshold);
        assert_eq!(result.timestamp_shape, Some(TimestampShape::Clf));
        assert_eq!(result.timestamp_offset, Some(13));
    }

    #[test]
    fn genuinely_unstructured_input_still_reports_fallback() {
        let lines: Vec<&str> = vec!["something happened", "and then something else did"];
        let result = detect(&lines, &ctx());
        assert_eq!(result.format, Format::Plain);
        assert_eq!(result.outcome, DetectionOutcome::Fallback);
        assert_eq!(result.timestamp_shape, None);
    }

    // --- logcat (`logcat-and-payload-precision` tasks 6.1, 6.2) --------------

    /// The measured bugreport's logcat sections reported `fell back to plain text
    /// (12%)`, because none of the six shapes could see `04-21 13:07:53.198`. With the
    /// shape in the sweep they are a threshold match on their own terms — no
    /// structural change to detection was needed (design D6).
    #[test]
    fn logcat_majority_input_is_a_threshold_match_not_a_fallback() {
        let mut lines: Vec<&str> =
            vec!["04-18 19:21:16.151  1000   806   995 D ActivityManager: freezing 2521 com.x"; 90];
        lines.extend(vec!["------ DUMPSYS ------"; 10]);
        let result = detect(&lines, &ctx());
        assert_eq!(result.format, Format::Plain);
        assert_eq!(result.outcome, DetectionOutcome::Threshold);
        assert!((result.match_fraction - 0.90).abs() < 1e-9);
        assert_eq!(result.timestamp_shape, Some(TimestampShape::Logcat));
        assert_eq!(result.timestamp_offset, Some(0));
    }

    /// The sticky choice records logcat like any other shape, and the modal rule still
    /// applies — one logcat line in an ISO file does not fix the hint.
    #[test]
    fn the_sticky_choice_records_logcat_like_any_other_shape() {
        let mut lines: Vec<&str> = vec!["2026-08-08T17:42:01Z the rest of the file"; 20];
        lines.push("04-21 13:07:51.985   806 29149 W UsbDescriptorParser: len 58");
        assert_eq!(
            detect(&lines, &ctx()).timestamp_shape,
            Some(TimestampShape::Iso)
        );
    }

    #[test]
    fn the_recorded_shape_is_the_modal_one_not_the_first_seen() {
        let mut lines: Vec<&str> = vec!["Aug  8 17:42:01 host app: one odd line"];
        lines.extend(vec!["2026-08-08T17:42:01Z the rest of the file"; 20]);
        let result = detect(&lines, &ctx());
        assert_eq!(result.timestamp_shape, Some(TimestampShape::Iso));
    }
}
