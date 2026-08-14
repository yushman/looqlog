# 0003. Bind to 127.0.0.1 by default; `--host` is opt-in with a mandatory warning

- **Status:** Accepted
- **Date:** 2026-08-08

## Context

Live tail (stdin mode) puts log lines — potentially containing credentials or PII — on
an unauthenticated WebSocket (TDR §13). The secondary persona (DevOps/platform
engineer, PRD §3) sometimes runs tools like this on a shared or containerized box where
binding to `0.0.0.0` would be the convenient choice for quick team access. One
misconfigured `docker run -p` or one forgotten firewall rule turns that convenience into
a network-wide credential leak.

## Decision

Default bind address is `127.0.0.1`. Changing it requires an explicit `--host` flag, and
any value other than `127.0.0.1` triggers a mandatory stdout warning at startup stating
that live logs become network-reachable without authentication (TDR §13, PRD §12).

## Alternatives considered

### Default to 0.0.0.0 for container/shared-box convenience

Rejected outright: makes "privacy first" false by default in the single most common
deployment mistake, exactly the scenario PRD §12 calls Critical impact.

### Require real authentication (token/password) whenever host != 127.0.0.1

More correct than a warning, but adds real scope — session or token lifecycle
management — that TDR §13 only sketches as a possible future
`--require-cors-origin` flag. Deferred deliberately: MVP non-goal, revisited below.

## Consequences

**Good:** the unsafe path requires a deliberate, named opt-in (`--host <ip>`) rather than
being one copy-pasted Docker flag away from exposure; matches "zero config, safe by
default" (PRD §4).

**Bad / accepted cost:** the warning is advisory, not a control. Once `--host` is
changed, there is no actual authentication — a same-network attacker can still connect
to the WebSocket. The Origin-check + one-time token planned for `/ws` (TDR §13) mitigates
same-machine cross-tab hijacking but does **not** protect the `--host 0.0.0.0` case.

**What would make us revisit:** real-world reports of `--host 0.0.0.0` being used in
shared or CI environments — would justify pulling minimal auth into MVP rather than P2.
