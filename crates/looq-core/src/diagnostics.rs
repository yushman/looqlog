//! Diagnostics for lines that did not become entries, plus encoding and timestamp
//! notices. Bounded and aggregated (design.md D4): a badly broken input must yield a
//! usable summary, not one allocation per bad line.

use std::collections::BTreeMap;

/// Why a line did not become an entry, or a non-fatal notice about one that did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DiagnosticReason {
    /// JSON format active, line does not parse as JSON at all.
    InvalidJson,
    /// JSON format active, line parses but its top level is not an object.
    NonObjectJson,
    /// A recognised timestamp field was present but its value could not be parsed.
    UnparsableTimestamp,
    /// The line was not valid UTF-8 and was decoded with the latin-1 fallback.
    EncodingFallback,
}

impl DiagnosticReason {
    pub fn label(&self) -> &'static str {
        match self {
            DiagnosticReason::InvalidJson => "invalid JSON",
            DiagnosticReason::NonObjectJson => "JSON value is not an object",
            DiagnosticReason::UnparsableTimestamp => "unparsable timestamp value",
            DiagnosticReason::EncodingFallback => "decoded with latin-1 fallback",
        }
    }
}

/// A single retained diagnostic: which line, why, and a human-readable detail.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub line: usize,
    pub reason: DiagnosticReason,
    pub detail: String,
}

/// Aggregated diagnostics: exact per-reason counts always kept, individual examples
/// retained only up to `cap` so a one-million-bad-lines input stays bounded in memory
/// (spec: "Diagnostics are bounded and aggregated").
#[derive(Debug, Clone)]
pub struct Diagnostics {
    cap: usize,
    retained: Vec<Diagnostic>,
    counts: BTreeMap<DiagnosticReason, usize>,
}

impl Diagnostics {
    pub fn new(cap: usize) -> Self {
        Self {
            cap,
            retained: Vec::new(),
            counts: BTreeMap::new(),
        }
    }

    pub fn record(&mut self, line: usize, reason: DiagnosticReason, detail: impl Into<String>) {
        *self.counts.entry(reason).or_insert(0) += 1;
        if self.retained.len() < self.cap {
            self.retained.push(Diagnostic {
                line,
                reason,
                detail: detail.into(),
            });
        }
    }

    /// Individual examples, capped. Use `count_for`/`total` for exact totals.
    pub fn retained(&self) -> &[Diagnostic] {
        &self.retained
    }

    pub fn count_for(&self, reason: DiagnosticReason) -> usize {
        self.counts.get(&reason).copied().unwrap_or(0)
    }

    pub fn total(&self) -> usize {
        self.counts.values().sum()
    }

    pub fn cap(&self) -> usize {
        self.cap
    }
}
