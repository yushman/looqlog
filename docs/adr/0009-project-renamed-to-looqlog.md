# 0009. Project renamed to `looqlog`

- **Status:** Accepted
- **Date:** 2026-08-20

## Context

ADR-0006 (2026-08-08) named the project `looq` before any code existed, and closed
with an unusually explicit exit condition:

> **What would make us revisit:** nothing short of a trademark conflict. Once the crate
> is published the name is permanent, so the practical window for reversal closes at
> first publish — which is also why this decision was made before writing code rather
> than after.

That condition is now met. [Looq for macOS](https://www.producthunt.com/products/looq-3)
is a Quick Look extension for developers that previews files, explicitly handles **log
files** — it advertises finding timestamps inside zipped log archives — and markets
itself on **"Zero Data Collection", running entirely locally**. That is this project's
positioning almost word for word, aimed at the same audience. Separately, [Looq
AI](https://looq.ai/) is a funded spatial-AI company holding `looq.ai`; a different
industry, and the weaker of the two problems, but a second holder of the name
nonetheless.

The window ADR-0006 described is still open: `looq`, `looq-core` and `looq-wasm` were
all unregistered on crates.io (verified: HTTP 404) at the time of this decision. Once
`cargo publish` runs, the name is permanent — this had to land before any crates.io
publication.

## Decision

The project, all three crates, the binary, the product name and the GitHub repository
become `looqlog` / `looqlog-core` / `looqlog-wasm`. `looqlog` is free on crates.io and
returns no competing product as a search term. One name is used everywhere — crate,
binary, product and repository — rather than a `git-delta`-style split between a
registry-safe crate name and a different command name; the conflict is with a tool
people search for and type the name of, so the string that collides is the string that
had to change.

## Alternatives considered

### `lq`

The strongest short candidate, and already ADR-0006's runner-up. Still unavailable, and
more entrenched than in 2026-08:

```
lq  0.16.0   created 2024-10-28   updated 2026-04-22   6,391 downloads
    github.com/clux/lq — "low overhead yq/tq/jq cli"
```

Not a judgement call this time either — the crate is actively maintained and has only
grown.

### `loglens`

Free on crates.io, and the worst possible choice in the wild. Six existing products
carry the name, all in this exact category:

| What | Overlap |
|---|---|
| [getloglens.com](https://getloglens.com/) | "Blazing Fast Structured Log Analysis CLI Tool", JSON, interactive TUI — this project's pitch |
| [`loglens-core`](https://users.rust-lang.org/t/loglens-core-a-zero-config-structured-log-parsing-engine-json-logfmt/136406) | a **Rust** crate, "zero-config structured log parsing engine (JSON/Logfmt)" — `looq-core`'s description, same `-core` suffix |
| LogLens (linuxlinks.com) | Python TUI log viewer |
| VS Code extension | Android logcat viewer |
| Chrome extension | "Simple Android Logcat Viewer" |
| Academic paper | "LogLens: A Real-time Log Analysis System" |

Choosing it would reproduce, worse, the exact failure ADR-0006 rejected "Open Log
Viewer" for: launching directly into an established brand's search results.

### `looqlog`

Keeps the deliberate misspelling ADR-0006 already priced as a known cost, and at 7
characters sits close to the 8 that got `logscope` rejected for the pipe position
(`myapp | looqlog`). What it buys over the alternatives above: it says what the tool
is, it returns no competing product, and it stays recognisably continuous with the
already-released `v0.1.0` rather than arriving as an unrelated third name.

## Consequences

**Good:** the conflict this ADR exists to resolve is closed while it is still cheap —
before any `cargo publish`. One name still covers crate, binary, product and
repository, preserving the arrangement ADR-0006 counted as its main win.

**Bad / accepted cost:** the pipe form gets three characters longer
(`myapp | looq` → `myapp | looqlog`), the exact axis `logscope` was rejected on in
ADR-0006, now paid here too. Anyone holding a `v0.1.0` binary — the command they
installed is named `looq` — has no in-place upgrade path; the next release installs a
differently-named command and the old one is left on disk untouched. The population is
small: the release was days old and never announced.

**What would make us revisit:** nothing short of another in-category conflict
discovered before the next irreversible publish step. There is no such step left after
this one — `looqlog` is the name that ships to crates.io.
