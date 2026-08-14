//! Vendored frontend artifacts (ADR-0008), embedded at compile time via
//! `include_str!`/`include_bytes!`. `scripts/build-frontend.sh` regenerates
//! `assets/` from `web/` sources and `crates/looq-wasm`; nothing here reads the
//! filesystem at runtime (`local-server` spec, "Static assets are served from the
//! binary").
//!
//! Layout mirrors the Vite build output (`browser-app-shell`, task 1.1/1.3):
//! `index.html`, a JS/CSS pair under `assets/` with fixed (unhashed) filenames so
//! the CI staleness check diffs cleanly, and `core.js`/`core.wasm` under `wasm/`
//! (wasm-pack `--target web` output, copied through Vite's `public/` dir unchanged).

const INDEX_HTML_TEMPLATE: &str = include_str!("../assets/index.html");
pub const APP_JS: &[u8] = include_bytes!("../assets/assets/index.js");
pub const APP_CSS: &[u8] = include_bytes!("../assets/assets/index.css");
pub const WORKER_JS: &[u8] = include_bytes!("../assets/assets/worker.js");
pub const CORE_JS: &[u8] = include_bytes!("../assets/wasm/core.js");
pub const CORE_WASM: &[u8] = include_bytes!("../assets/wasm/core.wasm");

/// Fill in the `__LOOQ_HINT__`, `__LOOQ_MODE__` and `__LOOQ_TOKEN__` placeholders in
/// the vendored `index.html`. `hint_html` must already be fully-formed, escaped HTML
/// — callers build it with [`html_escape`] applied to any untrusted input (e.g. the
/// CLI-supplied path). `mode_json` is a small trusted JSON object (`{"mode":"file"}`
/// or `{"mode":"stdin","maxLines":N}`, built in `main.rs` from typed values, never
/// from untrusted input) that `looq-app.ts` reads to decide which UI to mount.
/// `token` is the per-process random hex string (`security` spec, design.md D1) —
/// generated once in `main.rs`, never from untrusted input, read by
/// `web/src/token.ts` and presented as the first `/ws` message.
pub fn render_index(hint_html: &str, mode_json: &str, token: &str) -> String {
    INDEX_HTML_TEMPLATE
        .replace("__LOOQ_HINT__", hint_html)
        .replace("__LOOQ_MODE__", mode_json)
        .replace("__LOOQ_TOKEN__", token)
}

/// Minimal HTML-escaping for untrusted text (the CLI positional path) interpolated
/// into the served page.
pub fn html_escape(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_html_special_characters() {
        assert_eq!(
            html_escape("<script>alert('x')</script>"),
            "&lt;script&gt;alert(&#39;x&#39;)&lt;/script&gt;"
        );
    }

    #[test]
    fn render_index_substitutes_hint_and_mode() {
        let rendered = render_index("hello", r#"{"mode":"file"}"#, "tok123");
        assert!(rendered.contains("hello"));
        assert!(!rendered.contains("__LOOQ_HINT__"));
        assert!(rendered.contains(r#"{"mode":"file"}"#));
        assert!(!rendered.contains("__LOOQ_MODE__"));
        assert!(rendered.contains("tok123"));
        assert!(!rendered.contains("__LOOQ_TOKEN__"));
    }
}
