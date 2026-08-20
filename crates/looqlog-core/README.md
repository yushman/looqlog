# looqlog-core

The log parsing core of [looqlog](https://crates.io/crates/looqlog): byte chunks in,
structured entries and diagnostics out.

Target-agnostic by design — no `wasm-bindgen`, no `web-sys`, no `std::fs`, and no clock
— so the same crate backs both the browser (through a `wasm-bindgen` adapter) and native
callers. CI enforces this with a dedicated job rather than a convention.

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
- **No `regex` dependency.** Hand-written byte scanners, because `regex` alone cost
  roughly three times the whole WebAssembly size budget.

Full documentation lives in the
[repository](https://github.com/yushman/looqlog#readme).

MIT licensed.
