## Context

Every P0 feature works on the happy path. This change is about the other paths, and about the
two numbers the documents have been deferring since the beginning: the hard file-size cap that
TDR §14 marks "TBD by benchmark", and whether the TDR §11 latency targets are actually met.

It is also the last change before feature freeze. After it, per CLAUDE.md's scope discipline, a
good idea goes to the devlog's `## Ideas for later`, not into the code.

## Goals / Non-Goals

**Goals:**
- Failures that say what happened, in the UI, and stay visible.
- The TDR §13 security measures implemented with their threat model written down, so nobody
  mistakes them for authentication.
- Numbers: latency, throughput, binary size, and the file-size cap — measured, recorded, and
  either met or explicitly downgraded.
- Proof that the binary works somewhere other than the machine that built it.
- Both READMEs complete and in sync.

**Non-Goals:**
- New features. This change freezes scope; anything it discovers that is not a defect goes to the
  devlog.
- Authentication for `--host` bindings. ADR-0003 accepted the advisory warning; changing that
  needs a new ADR, not a task here.
- Windows and macOS release builds beyond best effort — TDR commits to Linux x86_64 at minimum.

## Decisions

### D1 — Token in the handshake payload, not the URL

A token in a query string ends up in shell history, in proxy logs, and in whatever the user
pastes when asking for help. So the page presents it as the first message on the WebSocket, and
the server closes any connection that has not authenticated within a short timeout. The token is
per process: it lives as long as the CLI does, a reload fetches the page and therefore the token
again, and multiple tabs work because each fetched the same document.

What this actually buys, stated so the implementation is not mistaken for more: a page on another
origin cannot read the served HTML, so it cannot learn the token, so it cannot open the socket —
which closes the cross-tab hijacking risk named in TDR §13. It does nothing about another process
on the same machine, which can fetch the page itself, and nothing about `--host 0.0.0.0`, where
anyone who can reach the port can also fetch the page. ADR-0003 already accepted that; this
change writes it where a user will see it.

### D2 — CSP verified per browser, not assumed

`default-src 'self'` is the target, but WASM compilation and worker creation have historically
needed extra directives in particular browser versions, which TDR §13 flags. So the policy is
tested in each browser PRD §11 supports, and any additional directive is added deliberately with a
note on why. A CSP that silently breaks WASM in Safari is worse than a slightly looser one that
works everywhere, because the failure looks like a broken product rather than a policy.

### D3 — The file-size cap comes from measurement, and refuses rather than degrades

Two numbers: a warning threshold where parsing will be slow and memory heavy, and a hard cap
where wasm32 linear memory cannot hold the entries and index (TDR §14 puts the overhead at 3–10x
the raw file). Above the cap the application refuses and explains, rather than starting a parse
that will die partway with an out-of-memory error the user cannot interpret. Both numbers cite the
measurements they came from, taken in earlier changes.

The alternative — chunked or windowed indexing so any size can be opened — is the right long-term
answer and is explicitly not MVP scope; refusing loudly is the honest interim.

### D4 — Binary detection by NUL bytes, with an override

A NUL byte in the first chunk means the file is almost certainly not text. The application says
so and offers to proceed anyway, because a log with one stray NUL byte exists and refusing it
outright would be wrong. The default matters more than the override: rendering a screen of
mojibake and letting the user work out why is the failure mode being designed away.

### D5 — Errors persist, toasts do not

Error messages stay until dismissed or superseded. A toast that fades after four seconds is
invisible to a user who was reading a stack trace when it appeared, and this application's errors
— a refused file, a failed module, a wrong format — are precisely the ones a user needs to still
be able to read a minute later.

### D6 — Theme follows the system, override persists locally

System preference by default, explicit toggle stored in the browser. This is the one piece of
persisted user state in the product; keeping it in the browser rather than in a config file
preserves the zero-config principle (PRD §4) and the promise that the CLI writes nothing.

### D7 — The fresh-machine run is a checklist, not a CI job

Automating it well means an environment that is not the dev machine but is also not a user's
machine, which tends to pass while the real thing fails. So it is a documented manual checklist
run against a container or VM before release, with its result recorded in the devlog. Cheap, and
it catches the class of bug that only exists outside the build environment.

## Risks / Trade-offs

- **CSP breaks WASM in a supported browser** → Test in all of them before the release build; add
  the minimum extra directive with a note, rather than dropping the policy.
- **The token is mistaken for authentication** → Its limits are documented next to it, in the
  README and in the security notes.
- **The hard cap is set too low and users hit it on legitimate files** → Derived from measurement
  and documented with its reasoning, so raising it is an informed change rather than a guess
  replacing another guess.
- **The performance pass finds a miss too late to fix** → mvp-plan already says the decision is
  made that day: fix, or downgrade and document. Silence is not an option.
- **Feature freeze pressure** → Anything found here that is not a defect goes to the devlog's
  ideas list, which is the mechanism that exists for exactly this moment.
- **README drift between the two languages** → Same commit, checked during the release pass.

## Open Questions

- Should the binary print the token or a full URL containing a one-time link, for the SSH
  port-forward case where the user opens the page manually on another machine?
- Does the theme preference belong in `localStorage` or in the URL hash? The hash makes a shared
  link carry an appearance the recipient did not choose.
- Which additional platforms get release binaries at 0.1.0, and does that decision belong to this
  change or to the release itself?
