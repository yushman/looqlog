## Why

By this point every P0 feature works on a good day with a good file. What is missing is what
happens on a bad day: a binary file dropped by mistake, a file too large for wasm32 memory, a
malicious page in another tab opening the WebSocket, a machine that has never seen the dev
environment. TDR §13's security measures are still unimplemented, TDR §14's hard file-size cap is
still "TBD by benchmark", and the only proof the binary works is that it works here.

This change covers days 24 to 30 of `docs/mvp-plan.md` — the performance pass, the feature
freeze, error states, the security pass, docs, the release build and the fresh-machine check.
They are one change because they share a single acceptance question: would this be safe and
comprehensible in the hands of someone who did not build it.

## What Changes

- CSP `default-src 'self'` on all responses, verified across the browsers PRD §11 targets, since
  WASM compilation under CSP has historically needed extra directives in some of them.
- Origin check on `/ws` plus a per-process token embedded in the served page and presented during
  the WebSocket handshake, closing the cross-tab hijacking risk in TDR §13 — with its actual
  threat model written down, including what it does not protect.
- A hard cap on file size, chosen from the measurements taken in earlier changes rather than
  guessed, with a warning threshold below it and a refusal above it that explains why.
- User-facing error states for the cases that currently produce a blank screen or a console-only
  message: unreadable file, empty file, binary file, a format that could not be parsed at all,
  and a WASM module that failed to load.
- Light and dark theme following the system preference by default, with an explicit toggle that
  persists.
- A performance pass measuring filter latency, live-tail end-to-end latency, parse throughput and
  binary cold start against TDR §11, with every miss either fixed or explicitly downgraded and
  documented.
- A P0 and P1 feature sweep against the golden path, after which MVP scope is frozen.
- `README.md` and `README.ru.md` completed and in sync: install, the three PRD flows, the privacy
  asymmetry between file and stdin modes, flags, and known limitations.
- Release build with the binary size recorded against the TDR §5 budget.
- A fresh-machine verification run of Flows 1, 2 and 3 from a binary that never touched the
  development environment.

Not in this change: MCP mode (P2, ADR-0005), syslog and other P1 formats, export, gzip, and
anything else in PRD §8 Out of Scope. Nothing new enters MVP scope after the freeze in this
change.

## Capabilities

### New Capabilities

- `security`: CSP, WebSocket origin and token handshake, and the stated limits of both.
- `error-states`: how the application behaves and what it says when input or environment is bad,
  including the file-size cap.
- `theming`: light and dark appearance, system preference, explicit override and persistence.

### Modified Capabilities

- `packaging`: adds the release build, the binary size budget check and the fresh-machine
  verification to the build-and-distribution contract established by
  `bootstrap-cli-and-wasm-skeleton`.

### Ordering note

Assumes every earlier change is archived. The file-size cap in particular depends on numbers
measured in `log-parsing-core`, `browser-app-shell` and `timeline-and-table`.

## Impact

- `crates/looq`: CSP headers, origin check, token generation and handshake.
- `web/`: error state surfaces, theme, size-cap enforcement before parsing begins.
- Both READMEs, the release workflow, and a documented manual verification checklist for the
  fresh-machine run.
- Closes TDR §13 (security), TDR §14's open file-size cap, and PRD §14 Q4 on auto-open defaults if
  the performance pass changes anything about startup.
- After the freeze in this change, further work is post-MVP by definition: new ideas go to the
  devlog's `## Ideas for later`, per the scope discipline in CLAUDE.md.
