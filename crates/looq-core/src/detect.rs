//! Format auto-detection (format-detection spec, design.md D3). Samples at most the
//! first 100 non-empty lines and evaluates candidates in TDR §8 priority order —
//! JSON, then logfmt — selecting the first to cross an 80% match threshold; plain
//! text is the fallback, never rejected.

use crate::format::Format;
use crate::parsers::{json, logfmt};

/// Lines sampled for detection.
pub const SAMPLE_SIZE: usize = 100;

/// Fraction of sampled lines that must match a candidate format for it to win
/// (design.md D3: a judgement call, reported alongside every result so a wrong
/// threshold is visible in practice rather than inferred).
pub const THRESHOLD: f64 = 0.8;

/// Whether the chosen format crossed the threshold outright, or plain text was
/// selected because nothing did.
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

/// Run detection over a sample of non-empty lines (already filtered by the caller;
/// at most `SAMPLE_SIZE` of them per the spec, though this function itself does not
/// enforce the cap — the incremental parser stops collecting once it has enough).
pub fn detect(sample: &[&str]) -> DetectionResult {
    if sample.is_empty() {
        return DetectionResult {
            format: Format::Plain,
            match_fraction: 0.0,
            outcome: DetectionOutcome::Fallback,
        };
    }

    let json_fraction = fraction_matching(Format::Json, sample);
    if json_fraction >= THRESHOLD {
        return DetectionResult {
            format: Format::Json,
            match_fraction: json_fraction,
            outcome: DetectionOutcome::Threshold,
        };
    }

    let logfmt_fraction = fraction_matching(Format::Logfmt, sample);
    if logfmt_fraction >= THRESHOLD {
        return DetectionResult {
            format: Format::Logfmt,
            match_fraction: logfmt_fraction,
            outcome: DetectionOutcome::Threshold,
        };
    }

    DetectionResult {
        format: Format::Plain,
        match_fraction: json_fraction.max(logfmt_fraction),
        outcome: DetectionOutcome::Fallback,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mostly_json_with_noise_selects_json() {
        let mut lines: Vec<&str> = vec![r#"{"a":1}"#; 95];
        let banners = vec!["starting up"; 5];
        lines.extend(banners);
        let result = detect(&lines);
        assert_eq!(result.format, Format::Json);
        assert_eq!(result.outcome, DetectionOutcome::Threshold);
    }

    #[test]
    fn ambiguous_input_falls_back_to_plain() {
        let mut lines: Vec<&str> = vec!["level=info msg=x"; 50];
        lines.extend(vec!["just some free text here"; 50]);
        let result = detect(&lines);
        assert_eq!(result.format, Format::Plain);
        assert_eq!(result.outcome, DetectionOutcome::Fallback);
    }

    #[test]
    fn no_candidate_reaches_threshold_selects_plain_without_failing() {
        let lines: Vec<&str> = vec!["free text one", "free text two"];
        let result = detect(&lines);
        assert_eq!(result.format, Format::Plain);
        assert_eq!(result.outcome, DetectionOutcome::Fallback);
    }

    #[test]
    fn result_carries_its_evidence() {
        let mut lines: Vec<&str> = vec!["level=info msg=x service=y"; 88];
        lines.extend(vec!["free text with no pairs"; 12]);
        let result = detect(&lines);
        assert_eq!(result.format, Format::Logfmt);
        assert!((result.match_fraction - 0.88).abs() < 1e-9);
        assert_eq!(result.outcome, DetectionOutcome::Threshold);
    }
}
