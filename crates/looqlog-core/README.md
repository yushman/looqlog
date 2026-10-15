# looqlog-core

The log parsing and query engine of [looqlog](https://crates.io/crates/looqlog): byte
chunks in, structured entries and diagnostics out, plus a `Session` that indexes them and
answers filter, search, window and timeline-bucket queries.

Target-agnostic by design — no `wasm-bindgen`, no `web-sys`, no `std::fs`, no async and
no clock — so the same crate backs the `looqlog` server and any other native caller. CI
enforces this with a dedicated job rather than a convention.

```rust
use looqlog_core::{Parser, TimeZonePolicy};

let mut parser = Parser::new(None, TimeZonePolicy::utc());
let mut entries = parser.feed(b"2026-08-08T17:42:01Z ERROR connection refused\n");
entries.extend(parser.finish());
```

- **Incremental.** One code path for a file read in large chunks and for stdin arriving
  one line at a time; an incomplete trailing line is held until the next chunk.
- **Auto-detecting.** JSON Lines, logfmt and plain text, with syslog, klog, Apache/CLF,
  Docker and Android logcat prefixes recognised inside the plain-text path.
- **Loud about failure.** A malformed line is skipped with a reported diagnostic, never
  dropped silently; entries emitted plus lines skipped plus blank lines always equals
  the total line count.
- **Native query engine.** A time-ordered index, chain-aware filters, substring and
  `re:` search (Rust `regex` syntax) and timeline buckets, with per-client views.

Full documentation lives in the
[repository](https://github.com/yushman/looqlog#readme).

MIT licensed.
