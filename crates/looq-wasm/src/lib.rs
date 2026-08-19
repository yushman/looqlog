//! Thin `wasm-bindgen` adapter over `looq-core` (ADR-0005, `wasm-bridge` spec).
//!
//! `ParserHandle` wraps `looq_core::Parser` one-to-one: construct per file (design.md
//! D4/D7 — a fresh instance means no state leaks between files, and cancelling a
//! parse is just dropping the handle), feed it byte chunks, read typed results back
//! through `serde-wasm-bindgen`. All parsing/detection/diagnostics logic stays in
//! `looq-core`; this crate only translates types across the JS boundary (`dto.rs`)
//! and never runs on the main thread — the worker in `web/src/worker.ts` is the only
//! caller.
//!
//! The `bootstrap-cli-and-wasm-skeleton` stub (`parse_json_lines_count`) and the
//! `log-parsing-core` benchmarking export (`parse_auto_detect_count`) are retired by
//! this change (`browser-file-loading` spec, "provisional single-format entry point
//! ... is removed") — `ParserHandle` replaces both.

mod dto;

use wasm_bindgen::prelude::*;

use dto::{
    format_from_str, DetectionResultDto, DiagnosticsSummaryDto, EntryDto, FieldInventoryDto,
};

fn to_js<T: serde::Serialize + ?Sized>(value: &T) -> Result<JsValue, JsValue> {
    // `serialize_maps_as_objects(true)`: BTreeMap<String, _> fields (Entry::fields,
    // FieldInventory::fields, FieldStats::values, the diagnostics counts map) cross
    // as plain JS objects (`Record<string, T>` on the TS side) rather than `Map`,
    // which is what `web/src/wasm-types.ts` and every consumer assume.
    // `serialize_missing_as_null(true)`: `Option::None` fields (`Entry::level`,
    // `Entry::timestamp`) must cross as JS `null`, matching the `string | null`
    // contract in `wasm-types.ts` — the default `undefined` reaches consumers that
    // check `=== null` (entry-index.ts) or feed straight into `escapeHtml`
    // (looq-filter-bar.ts's chip rendering), crashing the tab on any entry with no
    // level extracted.
    let serializer = serde_wasm_bindgen::Serializer::new()
        .serialize_maps_as_objects(true)
        .serialize_missing_as_null(true)
        .serialize_large_number_types_as_bigints(false);
    value
        .serialize(&serializer)
        .map_err(|err| JsValue::from_str(&format!("serialization failed: {err}")))
}

/// One parser instance bound to one file/stream. Construct a fresh `ParserHandle`
/// per file (`wasm-bridge` spec, "Each file gets a fresh parser instance") — the
/// worker drops the old handle and constructs a new one rather than reusing it.
#[wasm_bindgen]
pub struct ParserHandle {
    inner: looq_core::Parser,
}

#[wasm_bindgen]
impl ParserHandle {
    /// `format_override`: `"json" | "logfmt" | "plain"`, or `undefined` to
    /// auto-detect. `tz_offset_minutes`: a fixed UTC offset (minutes east) applied to
    /// timestamps with no explicit offset; `undefined` defaults to UTC (named IANA
    /// zones are out of scope — see `crates/looq-core/src/timestamp.rs`).
    /// `reference_ms`: the caller's "now" as epoch milliseconds (`Date.now()` in
    /// `web/src/worker.ts`), used to infer a year for the shapes that carry none —
    /// syslog RFC 3164, klog (`prefix-and-payload-parsing` design.md D4). It crosses
    /// the boundary rather than being read inside `looq-core` because ADR-0005
    /// requires that crate to stay target-agnostic and clock-free. Without it those
    /// shapes are not recognised at all, so a syslog or klog file gets an empty
    /// timeline.
    #[wasm_bindgen(constructor)]
    pub fn new(
        format_override: Option<String>,
        tz_offset_minutes: Option<i32>,
        reference_ms: Option<f64>,
    ) -> Result<ParserHandle, JsValue> {
        let format = match format_override {
            Some(raw) => Some(
                format_from_str(&raw)
                    .ok_or_else(|| JsValue::from_str(&format!("unknown format override: {raw}")))?,
            ),
            None => None,
        };
        let tz = match tz_offset_minutes {
            Some(minutes) => {
                let offset = chrono::FixedOffset::east_opt(minutes * 60)
                    .ok_or_else(|| JsValue::from_str("tz_offset_minutes out of range"))?;
                looq_core::TimeZonePolicy::fixed_offset(offset)
            }
            None => looq_core::TimeZonePolicy::utc(),
        };
        let mut ctx = looq_core::ParseContext::new(tz);
        if let Some(ms) = reference_ms {
            let reference = chrono::DateTime::from_timestamp_millis(ms as i64)
                .ok_or_else(|| JsValue::from_str("reference_ms out of range"))?;
            ctx = ctx.with_reference(reference);
        }
        Ok(ParserHandle {
            inner: looq_core::Parser::with_context(format, ctx),
        })
    }

    /// Feed the next chunk of bytes (a `Blob.slice` result read on the JS side).
    /// Returns entries completed by this call as `EntryDto[]`.
    pub fn feed(&mut self, chunk: &[u8]) -> Result<JsValue, JsValue> {
        let entries: Vec<EntryDto> = self.inner.feed(chunk).iter().map(EntryDto::from).collect();
        to_js(&entries)
    }

    /// Signal end of input: flushes any trailing partial line and finalizes
    /// detection if it never reached its sample size. Returns the final batch of
    /// entries as `EntryDto[]`.
    pub fn finish(&mut self) -> Result<JsValue, JsValue> {
        let entries: Vec<EntryDto> = self.inner.finish().iter().map(EntryDto::from).collect();
        to_js(&entries)
    }

    /// The detection result once known, or `null` while still sampling (or when a
    /// format override skipped detection entirely).
    pub fn detection(&self) -> Result<JsValue, JsValue> {
        match self.inner.detection() {
            Some(result) => to_js(&DetectionResultDto::from(result)),
            None => Ok(JsValue::NULL),
        }
    }

    /// The active format's string key (`"json" | "logfmt" | "plain"`), or `null`
    /// while still sampling.
    pub fn format(&self) -> Option<String> {
        self.inner.format().map(|f| f.as_str().to_string())
    }

    #[wasm_bindgen(js_name = diagnosticsSummary)]
    pub fn diagnostics_summary(&self) -> Result<JsValue, JsValue> {
        to_js(&DiagnosticsSummaryDto::from(self.inner.diagnostics()))
    }

    #[wasm_bindgen(js_name = fieldInventory)]
    pub fn field_inventory(&self) -> Result<JsValue, JsValue> {
        to_js(&FieldInventoryDto::from(self.inner.field_inventory()))
    }

    #[wasm_bindgen(js_name = entriesEmitted)]
    pub fn entries_emitted(&self) -> usize {
        self.inner.entries_emitted()
    }

    #[wasm_bindgen(js_name = blankLines)]
    pub fn blank_lines(&self) -> usize {
        self.inner.blank_lines()
    }

    #[wasm_bindgen(js_name = totalLines)]
    pub fn total_lines(&self) -> usize {
        self.inner.total_lines()
    }
}
