## Why

ADR-0006 chose `looq` on the premise that the name was "distinctive enough to own its
search results", and named the one condition that would justify revisiting it: a conflict
serious enough to matter, decided **before the crate is published**, because crates.io
never releases a name.

That conflict exists and the ADR did not know about it. [Looq for
macOS](https://www.producthunt.com/products/looq-3) is a Quick Look extension for
developers that previews files, **reads log files** — it advertises finding timestamps
inside zipped log archives — and markets itself on **"Zero Data Collection", running
entirely locally**. That is this project's positioning almost word for word, aimed at the
same audience. A second holder, [Looq AI](https://looq.ai/), is a funded spatial-AI
company in a different industry and is harmless by comparison; the macOS tool is not.

The window ADR-0006 described is still open: `looq`, `looq-core` and `looq-wasm` are all
unregistered on crates.io (verified: HTTP 404). Once `cargo publish` runs the name is
permanent. This change must therefore land before any crates.io publication.

## What Changes

- The project, all three crates, the binary, the product name and the GitHub repository
  become `looqlog` / `looqlog-core` / `looqlog-wasm`. `looqlog` is free on crates.io and
  returns no competing product as a search term.
- The three crate directories are renamed, and `looq`'s path dependency on `looq-core`
  follows.
- The twelve custom element tag names (`looq-app`, `looq-timeline`, `looq-workspace`, …)
  become `looqlog-*`.
- The three server-side template placeholders `__LOOQ_HINT__`, `__LOOQ_MODE__` and
  `__LOOQ_TOKEN__` become `__LOOQLOG_*__`.
- The theme `localStorage` key changes from `looq-theme`, so an existing user's saved
  light/dark choice resets once. Small, but it is a user-visible change and is recorded
  rather than left to be discovered.
- The GitHub repository is renamed from `openlogviewer` to `looqlog`, and the
  `repository` field every crate inherits from the workspace follows it. GitHub keeps a
  permanent redirect from the old URL, so existing clones and links keep working.
- ADR-0006 is marked `Superseded by ADR-0009`; its body is left untouched. ADR-0009
  records the macOS conflict as the trigger ADR-0006 itself specified.
- Historical records are **not** rewritten: `docs/devlog.md`, the bodies of ADR-0001…0008,
  and the 15 archived changes under `openspec/changes/archive/` keep saying `looq`,
  because that is what the project was called when they were written.

Not **BREAKING** in the API sense — nothing has been published or depended on. It is a
breaking change for anyone holding a v0.1.0 binary: the command they installed is named
`looq` and the next release ships `looqlog`, with no in-place upgrade path.

## Capabilities

### New Capabilities
None. No behavior changes; the tool does exactly what it did under a different name.

### Modified Capabilities
Sixteen requirements across seven capabilities name the binary or a crate in their text or
scenarios. No requirement's *meaning* changes — only the identifier a reader would type.

- `cli` (5 requirements): every scenario invokes the binary by name — `looq --help`,
  `echo hi | looq`, `looq --host 0.0.0.0`.
- `stdin-stream` (4): scenarios drive the producer through `looq --stdin`.
- `live-tail-ui` (2): the process is named, including in the weaker-privacy-guarantee
  requirement.
- `packaging` (2): `cargo install looq` and `looq --version`.
- `browser-file-loading` (1), `local-server` (1), `log-parsing` (1): the process, the
  command, and `looq-core`.

`format-detection` mentions `looq-core` only in its Purpose paragraph, which is not a
requirement and therefore carries no delta; it is updated as a direct file edit, along with
the Purpose sections of `cli` and `log-parsing`.

## Impact

- `crates/looq/` → `crates/looqlog/`, `crates/looq-core/` → `crates/looqlog-core/`,
  `crates/looq-wasm/` → `crates/looqlog-wasm/`; workspace members and all four
  `Cargo.toml` files.
- `crates/looqlog/src/assets.rs` — the three `__LOOQ_*__` placeholders and the
  `include_bytes!` paths.
- `web/src/**` — twelve custom element registrations and every `querySelector` using
  them; `web/src/theme.ts`'s storage key.
- `crates/looqlog/assets/` — the vendored bundle has element names minified into it and
  **must be regenerated** by `./scripts/build-frontend.sh`, never text-replaced.
- `.github/workflows/release.yml` — release asset names (`looq-${VERSION}-${target}`),
  build target and release title; `.github/workflows/ci.yml` — the `cargo package`,
  `cargo tree` and ADR-0005 guard steps that name the crates.
- `scripts/build-frontend.sh`, `scripts/smoke-release-binary.sh`,
  `scripts/gen-*-fixture.py`.
- `README.md` and `README.ru.md`, in the same commit and in sync.
- `docs/PRD.md`, `docs/TDR.md`, `docs/mvp-plan.md` — current documents, updated.
- `docs/adr/0006-project-named-looq.md` — status line only; new
  `docs/adr/0009-project-renamed-to-looqlog.md`.
- `openspec/specs/` — the eight capability specs listed above.
- Deliberately untouched: `docs/devlog.md`, ADR-0001…0008 bodies,
  `openspec/changes/archive/` (56 files total).
