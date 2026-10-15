//! Vendored frontend artifacts (ADR-0008), embedded at compile time via
//! `include_str!`/`include_bytes!`. `scripts/build-frontend.sh` regenerates `assets/`
//! from `web/` sources; nothing here reads the filesystem at runtime (`local-server`
//! spec, "Static assets are served from the binary").
//!
//! Layout mirrors the Vite build output: `index.html` and a JS/CSS pair under `assets/`
//! with fixed (unhashed) filenames so the CI staleness check diffs cleanly.

const INDEX_HTML_TEMPLATE: &str = include_str!("../assets/index.html");
pub const APP_JS: &[u8] = include_bytes!("../assets/assets/index.js");
pub const APP_CSS: &[u8] = include_bytes!("../assets/assets/index.css");

/// Fill in the `__LOOQLOG_MODE__` and `__LOOQLOG_TOKEN__` placeholders in the vendored
/// `index.html`. `mode_json` is a small trusted JSON object built in `server.rs` from typed
/// values (`{"mode":"landing"}`, `{"mode":"file"}` or `{"mode":"stdin","maxLines":N}`);
/// `token` is the
/// per-process random hex string (`security` spec), read by `web/src/token.ts` and
/// presented as the first `/ws` message.
pub fn render_index(mode_json: &str, token: &str) -> String {
    INDEX_HTML_TEMPLATE
        .replace("__LOOQLOG_MODE__", mode_json)
        .replace("__LOOQLOG_TOKEN__", token)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_index_substitutes_mode_and_token() {
        let rendered = render_index(r#"{"mode":"file"}"#, "tok123");
        assert!(rendered.contains(r#"{"mode":"file"}"#));
        assert!(!rendered.contains("__LOOQLOG_MODE__"));
        assert!(rendered.contains("tok123"));
        assert!(!rendered.contains("__LOOQLOG_TOKEN__"));
    }
}
