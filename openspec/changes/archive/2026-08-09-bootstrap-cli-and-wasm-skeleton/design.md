## Context

Nothing is built yet: no `Cargo.toml`, no specs, an empty devlog. Six accepted ADRs already
fix the shape of the system — web UI over a local server (0001), browser-side WASM parsing
with no backend file reads (0002), loopback bind by default (0003), ring-buffered stdin
(0004), a target-agnostic core crate (0005), the name `looq` (0006).

This change is the walking skeleton: one thin slice through every layer, plus the day-4
throughput measurement that decides whether ADR-0002 survives contact with reality. It also
forces two decisions that the existing documents assume without stating, and that get more
expensive with every day of code built on top of them.

## Goals / Non-Goals

**Goals:**
- A binary that boots, serves an embedded page, streams stdin over `/ws`, and parses a
  user-picked file in WASM — end to end, however crudely.
- A measured parse number for a ~1MB JSON Lines fixture, written down against the TDR §11
  target with the command that produced it.
- ADR-0007 and ADR-0008 written and accepted before the code that depends on them.
- A build that a Rust-only machine can run.

**Non-Goals:**
- Any real parser. One hardcoded JSON Lines entry point is the whole parsing surface here;
  logfmt, plain text, auto-detect, `Entry`, field extraction all belong to `log-parsing`.
- Any real buffering. `/ws` is line-in-line-out; ADR-0004's ring buffer, snapshot and gap
  indicator belong to `live-tail`.
- Any real UI. No Vite, no Web Components, no uPlot, no table — a single hand-written page
  is enough to observe the plumbing.
- Any real interop. `serde-wasm-bindgen` and `comlink` belong to `browser-app-shell`; here a
  number crossing the boundary is sufficient.
- Security hardening beyond the `--host` warning. CSP, `/ws` origin check and token are
  `release-hardening`, though the token's threat model is sketched below so that change
  does not have to invent it from nothing.

## Decisions

### D1 — The argv path is a hint, not an auto-loaded file (→ ADR-0007)

PRD US-1 reads "`looq app.log` opens the browser, **the file is parsed**", and mvp-plan day 3
expects `looq tests/fixtures/sample.jsonl` to show an entry count. TDR §7 says the opposite:
the UI opens the file through `<input type="file">`. Only one of these can be true. No
browser API lets a page open a path chosen by a server: `showOpenFilePicker` can suggest a
starting directory at best, and is Chromium-only; `file://` is unreachable from an `http://`
origin. Under ADR-0002, where the backend must not read the file, the user has to pick or
drop it.

Alternatives considered:

- **Stream the file's bytes through the stdin path** (`looq app.log` behaves as
  `cat app.log | looq`). Delivers the zero-click golden path exactly as PRD Flow 1 promises.
  Rejected: it moves file mode into the weaker stdin privacy tier (TDR §12 — "does not leave
  the machine" instead of "does not leave the browser") and makes US-6's empty-Network-tab
  test fail, which is the single claim that separates looq from every server-side viewer.
- **Opt-in `--serve-file`** — the picker by default, byte-streaming on request. A real
  option, and TDR §12 already reserves `--enable-server-side-parse` as its neighbour.
  Rejected for now as a flag with no demand behind it yet; it stays available if users
  actually complain about the click.

Consequence to be honest about in both READMEs: `looq app.log` differs from bare `looq` only
by showing which file to open. The pitch shrinks from "opens your log" to "opens a viewer
for your log". Better to say that now than to discover it in a launch thread.

### D2 — Vendored frontend artifacts (→ ADR-0008)

`include_bytes!` needs the JS bundle and `core.wasm` to exist before `cargo build` runs. TDR
§5 promises `cargo install looq`; PRD §11 calls Node "dev only". Both hold only if built
artifacts are committed and shipped inside the crate.

Alternatives considered:

- **`build.rs` invoking `wasm-pack`/npm.** Always fresh, nothing to commit. Rejected: every
  `cargo install` would then need Node, a network fetch and several minutes, on a machine
  the user expects to need only Rust.
- **Skip crates.io, ship prebuilt binaries only.** Rejected: contradicts TDR §5 and cuts off
  the audience that installs everything through `cargo install`.

The cost is a repository containing build output, which is only safe with a CI check that
the committed artifacts match their sources — otherwise a stale bundle ships silently, which
is exactly the failure class this project's testing rules single out.

### D3 — Crate layout fixed now, per ADR-0005

`crates/looq-core` (no `wasm-bindgen`, no `web-sys`, no `std::fs`), `crates/looq-wasm` (the
browser adapter), `crates/looq` (the binary). Even with one hardcoded parse function, the
split is written now because ADR-0005's whole point is avoiding a later rewrite; retrofitting
it after logfmt, plain text and auto-detect exist is when it stops being free.

### D4 — `/ws` protocol shape is provisional and named as such

Lines cross as plain text WebSocket messages, one message per line. TDR §16 leaves
text-versus-binary framing open; that question belongs to `live-tail`, where the snapshot
message and the gap event give it actual content. The provisional shape is recorded here so
`live-tail` knows it is replacing a placeholder rather than breaking a contract.

### D5 — `/ws` token threat model, recorded but not implemented

TDR §13 asks for an `Origin` check plus a one-time token without saying what they buy. To be
written down now so `release-hardening` implements something meaningful: a token embedded in
`index.html` defends against a malicious page in another tab opening `ws://127.0.0.1:PORT/ws`,
because same-origin policy stops that page from reading `index.html` to learn the token. It
does not defend against another local process, and it does not defend the `--host 0.0.0.0`
case — ADR-0003 already accepts that the warning there is advisory, not a control. Open
sub-questions for that change: per-process or per-connection token, survival across reload,
and behaviour with several tabs.

### D6 — The day-4 measurement is a deliverable, not a step

The benchmark's output is a line in `docs/devlog.md` containing the number, the target, the
fixture, the machine and the command. If the number misses <200ms/MB by a wide margin, the
correct response is recorded in mvp-plan: one timeboxed optimisation pass, then either an
explicitly downgraded target or a reopened ADR-0002 — not silent continuation.

## Risks / Trade-offs

- **The golden path gets weaker than the PRD promises (D1)** → State it in both READMEs and
  in the page's own prompt text; keep `--serve-file` as a documented escape hatch rather than
  quietly re-adding server-side reads later.
- **WASM throughput misses the target and invalidates ADR-0002** → Measured on the walking
  skeleton, before parsers, UI or interop exist; the sunk cost at the decision point is a few
  days, not a few weeks.
- **Committed build artifacts drift from source (D2)** → CI check with a named rebuild
  command; a stale bundle must fail loudly, since a silently old UI is indistinguishable from
  a bug in the new one.
- **Non-deterministic bundler output makes the CI check flap** → Pin the toolchain versions
  and verify byte-identical output across two runs before wiring the check in; if that proves
  impossible, compare a normalised hash rather than raw bytes.
- **`--port 0` plus `--open` race**: the browser may be launched before the listener is
  ready → Launch only after the listener is bound and the actual port is known.
- **Skeleton code calcifies into the real implementation** → The specs explicitly mark the
  hardcoded parser and the buffer-less `/ws` as provisional and name the changes that
  replace them.

## Open Questions — resolved

- **Reference browser:** Chromium, driven via Playwright, version recorded alongside the
  measured number in `docs/devlog.md` (read from `navigator.userAgent` at benchmark time).
  Chosen over Firefox because it is the automatable, reproducible option available in this
  environment; the number is explicitly attached to that one browser/version, not claimed
  as cross-browser.
- **Fixture generator ships in-repo, the ~1MB file does not.** `scripts/gen-bench-fixture.py`
  (or equivalent) deterministically generates the ~1MB JSON-Lines benchmark fixture from a
  fixed seed/pattern; the small `tests/fixtures/sample.jsonl` (used for the entry-count
  correctness check, task 5.5) is committed directly since it is small enough to review as
  text.
- **`--max-lines` in file mode:** parsed and reported, never rejected — but if the user
  explicitly passes a non-default value while in file mode, the CLI prints a one-line note
  that `--max-lines` has no effect outside stdin mode, so the flag being a no-op is visible
  rather than a silent surprise.
