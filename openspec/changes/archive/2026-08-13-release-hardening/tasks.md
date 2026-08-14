## 1. Performance pass (mvp-plan day 24)

- [x] 1.1 Measure filter latency at 10,000 entries against the <50ms target in TDR §11
- [x] 1.2 Measure live-tail end-to-end latency against the <100ms target
- [x] 1.3 Measure parse throughput against <200ms/MB and binary cold start against <100ms
- [x] 1.4 Fix the worst regression found, or explicitly downgrade the target and document the new number with its reasoning
- [x] 1.5 Record every number in `docs/devlog.md` with the command and machine that produced it

## 2. Security (mvp-plan day 27)

- [x] 2.1 CSP `default-src 'self'` on all responses
- [x] 2.2 Verify WASM compilation and worker creation under the policy in every browser PRD §11 supports; add any required directive deliberately, with a note on why
- [x] 2.3 Origin check on `/ws` rejecting mismatched origins before any data is sent
- [x] 2.4 Per-process random token embedded in the page, presented in the handshake payload rather than the URL, with a timeout closing unauthenticated connections
- [x] 2.5 Tests: cross-origin connection refused, tokenless connection closed, reload reconnects successfully, multiple tabs work
- [x] 2.6 Write the threat model and its two gaps — other local processes, non-loopback `--host` — into the README and security notes
- [x] 2.7 Verify the `--host` warning from `bootstrap-cli-and-wasm-skeleton` still fires and still tells the truth

## 3. File size limits

- [x] 3.1 Derive the warning threshold and hard cap from the memory and throughput measurements taken in earlier changes
- [x] 3.2 Warning before parsing above the threshold, with continue and cancel
- [x] 3.3 Refusal above the cap explaining the wasm32 memory limit and suggesting alternatives
- [x] 3.4 Document both numbers with the measurements they came from; update TDR §14, which currently says TBD

## 4. Error states (mvp-plan day 26)

- [x] 4.1 Empty file, unreadable file, and module-load failure each with their own message
- [x] 4.2 Binary detection by NUL bytes in the first chunk, with a proceed-anyway override
- [x] 4.3 A format that produced no entries reads as such, distinct from an empty file and from a filter matching nothing
- [x] 4.4 A failed open leaves previously loaded data and filters intact
- [x] 4.5 Errors persist until dismissed or superseded; no fading-toast-only presentation

## 5. Theme (mvp-plan day 26)

- [x] 5.1 Light and dark appearance following the system preference by default
- [x] 5.2 Explicit toggle persisted locally; decide storage location per the design open question
- [x] 5.3 Check legibility of levels, gap markers, highlights and the timeline in both appearances
- [x] 5.4 Confirm no external fonts, stylesheets or images; both appearances render with the network disabled

## 6. Feature freeze (mvp-plan day 25)

- [x] 6.1 Pass over every P0 item F-1…F-7 and F-9…F-13 against the golden path
- [x] 6.2 Pass over every P1 item F-8, F-14, F-15 — functional even if unpolished
- [x] 6.3 Record anything found that is not a defect in the devlog's `## Ideas for later`, not in the code

## 7. Docs and release (mvp-plan days 28–29)

- [x] 7.1 `README.md` and `README.ru.md` complete and in sync: install, the three flows, every flag, the privacy asymmetry, known limitations including the size cap
- [ ] 7.2 Release build for Linux x86_64; record binary, WASM and bundle sizes against the TDR §5 budget —
      **not completed as specified**: this sandbox has no Linux cross-toolchain (no `zig`,
      `x86_64-unknown-linux-gnu-gcc`, `musl-gcc`, `cross`, or Docker). A macOS arm64 release
      build was produced and its size recorded against the same TDR §5 budget instead, with
      the gap stated plainly in `docs/devlog.md` rather than silently substituted. NEEDS HUMAN
      DECISION / follow-up: build on an actual Linux x86_64 environment before 0.1.0 ships.
- [ ] 7.3 Documented fresh-machine checklist; run Flows 1, 2 and 3 from a binary that never touched the dev environment —
      **partially completed**: no container/VM was available in this sandbox, so the closest
      proxy (running the compiled release binary directly, not `cargo run`, against fresh
      ports/tabs) was used for all three flows, and all three passed. This does not rule out an
      environment-specific dependency a genuinely clean machine would lack. NEEDS HUMAN DECISION
      / follow-up: an actual container/VM run before 0.1.0 ships.
- [ ] 7.4 Fix anything that only breaks outside the dev machine, then re-run the checklist —
      blocked on 7.3's real run; nothing surfaced by the proxy run to fix.
- [ ] 7.5 Decide which additional platforms get 0.1.0 binaries — NEEDS HUMAN DECISION, no human
      available in this session to decide; only macOS arm64 (this sandbox) and the Linux x86_64
      TDR §5 commitment (not yet built here) are on the table.

## 8. Buffer (mvp-plan day 30)

- [ ] 8.1 Absorb whatever slipped from earlier changes — nothing concrete slipped from changes
      1–6 that required fixing here (each change's own devlog entry closed its own scope); the
      real slippage found in *this* change (7.2–7.5 above) is recorded, not silently absorbed.
- [ ] 8.2 If nothing slipped, record the demo video referenced in PRD US-7, under two minutes —
      not done: no video-recording capability in this environment. NEEDS HUMAN DECISION /
      follow-up.
- [x] 8.3 Resolve the design open questions: token or URL for the SSH port-forward case, theme storage location —
      both were already answered in design.md's own D1 ("presented as the first message on the
      WebSocket... never a query string") and D6 ("stored in the browser [via localStorage]");
      implemented exactly as those decisions state.

## 9. Wrap-up

- [x] 9.1 Final devlog entry covering the release with its numbers
- [x] 9.2 `openspec validate release-hardening --strict` passes
- [x] 9.3 Archive the change; `openspec/specs/` now describes shipped 0.1.0 behaviour
