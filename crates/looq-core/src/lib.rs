//! Target-agnostic log parsing core (ADR-0005).
//!
//! This crate has no `wasm-bindgen`, no `web-sys`, and no `std::fs` dependency, so it
//! can be reused unmodified by a native adapter (future MCP mode) as well as the wasm
//! adapter (`looq-wasm`).
//!
//! The real parsing surface — the incremental `Parser`, the JSON Lines / logfmt /
//! plain-text format parsers, auto-detection, and `Entry`/field extraction — lives in
//! the modules below (`log-parsing-core` change). `parse_json_lines` at the bottom of
//! this file is the `bootstrap-cli-and-wasm-skeleton` walking-skeleton stub; it is
//! kept as-is (not modified by this change — see that change's proposal.md "Modified
//! Capabilities: None") because the current `browser-file-loading` spec still exercises
//! it. `browser-app-shell` is expected to rewire the page onto the real `Parser` and
//! retire this stub.

pub mod detect;
pub mod diagnostics;
pub mod entry;
pub mod fields;
pub mod format;
pub mod level;
pub mod parser;
pub mod parsers;
pub mod timestamp;

pub use detect::{DetectionOutcome, DetectionResult};
pub use diagnostics::{Diagnostic, DiagnosticReason, Diagnostics};
pub use entry::{Entry, FieldValue};
pub use fields::{FieldInventory, FieldStats};
pub use format::Format;
pub use level::Level;
pub use parser::Parser;
pub use timestamp::TimeZonePolicy;

/// Result of parsing a JSON-Lines input: how many lines parsed as valid JSON, and a
/// warning for every line that did not. Malformed lines are never silently dropped —
/// per this project's testing rules (CLAUDE.md), a bug here must be loud, not quiet.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ParseOutcome {
    pub entry_count: usize,
    pub warnings: Vec<String>,
}

/// Parse `input` as JSON-Lines: one JSON value per non-empty line. Blank lines are
/// skipped silently (they carry no data); lines that fail to parse as JSON are
/// skipped but recorded as a warning rather than dropped without a trace.
///
/// Walking-skeleton stub (see module doc comment above) — superseded by
/// [`Parser`] for real use.
pub fn parse_json_lines(input: &str) -> ParseOutcome {
    let mut outcome = ParseOutcome::default();
    for (idx, line) in input.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        match serde_json::from_str::<serde_json::Value>(trimmed) {
            Ok(_) => outcome.entry_count += 1,
            Err(err) => outcome
                .warnings
                .push(format!("line {}: malformed JSON ({err})", idx + 1)),
        }
    }
    outcome
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_valid_json_lines() {
        let input = "{\"a\":1}\n{\"b\":2}\n{\"c\":3}\n";
        let outcome = parse_json_lines(input);
        assert_eq!(outcome.entry_count, 3);
        assert!(outcome.warnings.is_empty());
    }

    #[test]
    fn skips_blank_lines_without_counting_or_warning() {
        let input = "{\"a\":1}\n\n   \n{\"b\":2}\n";
        let outcome = parse_json_lines(input);
        assert_eq!(outcome.entry_count, 2);
        assert!(outcome.warnings.is_empty());
    }

    #[test]
    fn malformed_line_is_warned_not_silently_dropped() {
        let input = "{\"a\":1}\nnot json\n{\"b\":2}\n";
        let outcome = parse_json_lines(input);
        assert_eq!(outcome.entry_count, 2);
        assert_eq!(outcome.warnings.len(), 1);
        assert!(outcome.warnings[0].contains("line 2"));
    }

    #[test]
    fn empty_input_has_zero_entries_and_no_warnings() {
        let outcome = parse_json_lines("");
        assert_eq!(outcome.entry_count, 0);
        assert!(outcome.warnings.is_empty());
    }
}
