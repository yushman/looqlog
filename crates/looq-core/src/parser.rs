//! The incremental parser (design.md D1): byte chunks in, entries out, one code path
//! shared by file mode (large chunks) and stdin mode (one line at a time).

use crate::detect::{self, DetectionResult};
use crate::diagnostics::{DiagnosticReason, Diagnostics};
use crate::entry::Entry;
use crate::fields::FieldInventory;
use crate::format::Format;
use crate::parsers::{json, logfmt, plain, Extracted};
use crate::timestamp::{ParseContext, TimeZonePolicy};

/// Default cap on individually-retained diagnostics. Measured at ~126 bytes/item
/// uncapped (`examples/mem_probe.rs`, task 6.1 — see `docs/devlog.md` for the exact
/// command and numbers), so 5,000 retained examples costs ~615KB — bounded well
/// inside the WASM-heap constraint TDR §14 flags, while still keeping far more
/// examples than a human would scroll through for one bad file.
pub const DEFAULT_DIAGNOSTIC_CAP: usize = 5_000;

/// Default cap on distinct values tracked per field. Measured at ~81 bytes/item
/// uncapped (task 6.1), so 10,000 distinct values costs ~788KB per field — chosen
/// higher than the diagnostic cap because a legitimately low-but-not-trivial
/// cardinality field (e.g. `status_code` unioned with a few hundred `service`
/// values) should not get flagged high-cardinality prematurely, while a
/// `request_id`-shaped field (task 6.5: 50,000 distinct values) still gets capped
/// well before it could grow unbounded.
pub const DEFAULT_FIELD_VALUE_CAP: usize = 10_000;

enum State {
    /// Buffering lines until either `detect::SAMPLE_SIZE` non-empty lines have been
    /// seen or `finish()` is called with fewer. `buffered_all` keeps every line
    /// (including blanks, in order) so it can be replayed once a format is chosen;
    /// `sample` keeps only the non-empty ones detection actually looks at.
    Sampling {
        buffered_all: Vec<(usize, String)>,
        sample: Vec<(usize, String)>,
    },
    Active {
        format: Format,
        detection: Option<DetectionResult>,
    },
}

/// Parses successive byte chunks into `Entry` values plus diagnostics, holding state
/// (active format, decoder tail, field inventory, diagnostic counters) across calls.
/// Construct a fresh instance per input — reusing one across two different files/
/// streams mixes their format detection and field inventories.
pub struct Parser {
    ctx: ParseContext,
    state: State,
    diagnostics: Diagnostics,
    inventory: FieldInventory,
    pending_bytes: Vec<u8>,
    line_no: usize,
    blank_lines: usize,
    entries_emitted: usize,
}

impl Parser {
    /// `format_override`: when `Some`, detection is skipped entirely and every line
    /// is parsed under that format from the start (format-detection spec, "Explicit
    /// override wins over detection").
    ///
    /// Year-less timestamp shapes (syslog RFC 3164, klog) need a reference instant to
    /// be dated at all — supply one with [`Parser::with_context`], because ADR-0005
    /// forbids this crate from reading the system clock itself.
    pub fn new(format_override: Option<Format>, tz: TimeZonePolicy) -> Self {
        Self::with_context(format_override, ParseContext::new(tz))
    }

    /// As [`Parser::new`], with the full parse context — the timezone policy plus the
    /// caller-supplied reference instant for year inference (design.md D4).
    pub fn with_context(format_override: Option<Format>, ctx: ParseContext) -> Self {
        let state = match format_override {
            Some(format) => State::Active {
                format,
                detection: None,
            },
            None => State::Sampling {
                buffered_all: Vec::new(),
                sample: Vec::new(),
            },
        };
        Self {
            ctx,
            state,
            diagnostics: Diagnostics::new(DEFAULT_DIAGNOSTIC_CAP),
            inventory: FieldInventory::new(DEFAULT_FIELD_VALUE_CAP),
            pending_bytes: Vec::new(),
            line_no: 0,
            blank_lines: 0,
            entries_emitted: 0,
        }
    }

    pub fn with_caps(
        format_override: Option<Format>,
        tz: TimeZonePolicy,
        diagnostic_cap: usize,
        field_value_cap: usize,
    ) -> Self {
        let mut parser = Self::new(format_override, tz);
        parser.diagnostics = Diagnostics::new(diagnostic_cap);
        parser.inventory = FieldInventory::new(field_value_cap);
        parser
    }

    /// Feed the next chunk of bytes. Returns entries completed by this call; an
    /// incomplete trailing line is held until the next chunk or `finish()`.
    pub fn feed(&mut self, chunk: &[u8]) -> Vec<Entry> {
        self.pending_bytes.extend_from_slice(chunk);
        let mut out = Vec::new();
        while let Some(pos) = self.pending_bytes.iter().position(|&b| b == b'\n') {
            let line_bytes: Vec<u8> = self.pending_bytes.drain(..=pos).collect();
            let line_bytes = &line_bytes[..line_bytes.len() - 1];
            self.consume_line(line_bytes, &mut out);
        }
        out
    }

    /// Signal end of input: flushes a trailing line with no newline, and — if
    /// detection never reached its sample size — finalizes detection on whatever
    /// was collected (format-detection spec, "Short input").
    pub fn finish(&mut self) -> Vec<Entry> {
        let mut out = Vec::new();
        if !self.pending_bytes.is_empty() {
            let remaining = std::mem::take(&mut self.pending_bytes);
            self.consume_line(&remaining, &mut out);
        }
        if matches!(self.state, State::Sampling { .. }) {
            self.finalize_detection(&mut out);
        }
        out
    }

    fn consume_line(&mut self, raw_bytes: &[u8], out: &mut Vec<Entry>) {
        let raw_bytes = strip_trailing_cr(raw_bytes);
        self.line_no += 1;
        let (text, used_fallback) = decode_line(raw_bytes);
        if used_fallback {
            self.diagnostics.record(
                self.line_no,
                DiagnosticReason::EncodingFallback,
                "line was not valid UTF-8; decoded with the latin-1 fallback",
            );
        }

        if text.trim().is_empty() {
            self.blank_lines += 1;
            return;
        }

        match &mut self.state {
            State::Sampling {
                buffered_all,
                sample,
            } => {
                buffered_all.push((self.line_no, text.clone()));
                sample.push((self.line_no, text));
                if sample.len() >= detect::SAMPLE_SIZE {
                    self.finalize_detection(out);
                }
            }
            State::Active { format, .. } => {
                let format = *format;
                self.process_and_emit(format, self.line_no, &text, out);
            }
        }
    }

    fn finalize_detection(&mut self, out: &mut Vec<Entry>) {
        let State::Sampling {
            buffered_all,
            sample,
        } = std::mem::replace(
            &mut self.state,
            State::Active {
                format: Format::Plain,
                detection: None,
            },
        )
        else {
            return;
        };

        let sample_refs: Vec<&str> = sample.iter().map(|(_, s)| s.as_str()).collect();
        let result = detect::detect(&sample_refs, &self.ctx);
        let format = result.format;

        // Sticky prefix choice (design.md D3): the sample that picked the format also
        // picks the timestamp shape, so the common case costs one match attempt per
        // line instead of sweeping every shape. Purely an ordering hint — every line
        // still falls back to the full sweep when the hint misses.
        if let (Some(shape), Some(offset)) = (result.timestamp_shape, result.timestamp_offset) {
            self.ctx.set_prefix_hint(shape, offset);
        }

        for (line_no, text) in &buffered_all {
            self.process_and_emit(format, *line_no, text, out);
        }

        self.state = State::Active {
            format,
            detection: Some(result),
        };
    }

    fn process_and_emit(
        &mut self,
        format: Format,
        line_no: usize,
        text: &str,
        out: &mut Vec<Entry>,
    ) {
        match format {
            Format::Json => match json::parse_line(text, &self.ctx) {
                Ok(extracted) => self.finish_entry(line_no, extracted, out),
                Err(reason) => {
                    let (diag_reason, detail) = match reason {
                        json::MalformedReason::InvalidJson(detail) => {
                            (DiagnosticReason::InvalidJson, detail)
                        }
                        json::MalformedReason::NonObjectJson => (
                            DiagnosticReason::NonObjectJson,
                            "JSON value is not an object".to_string(),
                        ),
                    };
                    self.diagnostics.record(line_no, diag_reason, detail);
                }
            },
            Format::Logfmt => {
                let extracted = logfmt::parse_line(text, &self.ctx);
                self.finish_entry(line_no, extracted, out);
            }
            Format::Plain => {
                let extracted = plain::parse_line(text, &self.ctx);
                self.finish_entry(line_no, extracted, out);
            }
        }
    }

    fn finish_entry(&mut self, line_no: usize, extracted: Extracted, out: &mut Vec<Entry>) {
        if let Some(detail) = extracted.timestamp_diagnostic {
            self.diagnostics
                .record(line_no, DiagnosticReason::UnparsableTimestamp, detail);
        }
        for (name, value) in &extracted.fields {
            self.inventory.record(name, &value.display());
        }
        self.entries_emitted += 1;
        out.push(Entry {
            ordinal: line_no,
            timestamp: extracted.timestamp,
            timestamp_used_default_tz: extracted.timestamp_used_default_tz,
            timestamp_year_inferred: extracted.timestamp_year_inferred,
            level: extracted.level,
            message: extracted.message.unwrap_or_default(),
            fields: extracted.fields,
        });
    }

    pub fn diagnostics(&self) -> &Diagnostics {
        &self.diagnostics
    }

    pub fn field_inventory(&self) -> &FieldInventory {
        &self.inventory
    }

    /// `None` while still sampling (or when a format override skipped detection
    /// entirely), `Some` once a format has been chosen by detection.
    pub fn detection(&self) -> Option<&DetectionResult> {
        match &self.state {
            State::Active { detection, .. } => detection.as_ref(),
            State::Sampling { .. } => None,
        }
    }

    /// The active format, once known (after detection finalizes, or immediately for
    /// a forced format). `None` while still sampling.
    pub fn format(&self) -> Option<Format> {
        match &self.state {
            State::Active { format, .. } => Some(*format),
            State::Sampling { .. } => None,
        }
    }

    pub fn entries_emitted(&self) -> usize {
        self.entries_emitted
    }

    pub fn blank_lines(&self) -> usize {
        self.blank_lines
    }

    /// Total physical lines consumed so far, including blanks. Together with
    /// `entries_emitted`, `blank_lines` and `diagnostics().total()` this satisfies
    /// the "every skipped line is accounted for" invariant (log-parsing spec).
    pub fn total_lines(&self) -> usize {
        self.line_no
    }
}

fn strip_trailing_cr(bytes: &[u8]) -> &[u8] {
    if bytes.last() == Some(&b'\r') {
        &bytes[..bytes.len() - 1]
    } else {
        bytes
    }
}

/// Decode one line's bytes as UTF-8, falling back to latin-1 (each byte maps
/// directly to the Unicode scalar of the same value) when the bytes are not valid
/// UTF-8. Because decoding only happens once a complete line (up to `\n`) has been
/// assembled across chunks, a multi-byte UTF-8 character split at a chunk boundary
/// is never misdecoded — it is simply not attempted until the whole line has
/// arrived (log-parsing spec, "Multi-byte character split across chunks").
fn decode_line(bytes: &[u8]) -> (String, bool) {
    match std::str::from_utf8(bytes) {
        Ok(s) => (s.to_string(), false),
        Err(_) => {
            let s: String = bytes.iter().map(|&b| b as char).collect();
            (s, true)
        }
    }
}
