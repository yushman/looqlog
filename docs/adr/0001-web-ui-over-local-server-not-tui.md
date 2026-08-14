# 0001. Serve a web UI over a local HTTP server, not a TUI

- **Status:** Accepted
- **Date:** 2026-08-08

## Context

PRD §2 explicitly positions looq against `lnav`, a mature TUI log viewer:
powerful, but with unclear UX, no privacy mode, and no shareable URL for a time range.
The primary user (backend dev / SRE, PRD §3) wants timeline + filters + search fast
during an incident; the tertiary user (privacy engineer, PRD §3) needs a verifiable
privacy guarantee (US-6: empty Network tab, file never leaves the browser process).
A known risk (PRD §12) is that SRE incident response often happens over SSH to a
headless remote box, where a browser-based tool is inconvenient without port-forwarding.

## Decision

Ship as a local HTTP + WebSocket server (axum) rendering a UI in the user's own
browser, opened automatically only when `--open` is passed (default off). No TUI mode
in MVP (TDR §2, Non-Goal).

## Alternatives considered

### Terminal UI (ratatui-style, like lnav)

Works over SSH without port-forwarding, no browser dependency, fits the remote-SRE
workflow directly. Rejected for MVP: cannot reuse the File API isolation that the
privacy guarantee depends on (US-6), has no natural drag-to-select timeline widget,
and would need URL-hash-equivalent state sharing reinvented from scratch. Would also
require a second, native implementation of the parsing/index logic instead of the one
already planned for the browser (see ADR-0005).

### Desktop app (Electron / Tauri)

Native file dialogs, no port to bind at all. Rejected: Electron ships a bundled
Chromium, directly violating the "single binary, zero runtime deps" principle (PRD §4);
Tauri is lighter but still adds per-OS packaging complexity that a "browser you already
have" avoids entirely, which is the core of the "open it like a PDF" pitch (PRD §1).

## Consequences

**Good:** reuses a browser the user already has and keeps updated; enables File API
privacy isolation (ADR-0002) and cheap rich charting (uPlot) instead of building both
from scratch; URL hash state (F-15) is trivial to share on the same machine.

**Bad / accepted cost:** degrades on headless/WSL/SSH-only remote machines — the exact
environment where SRE incident response often happens (PRD §12). Mitigated, not solved,
by `--no-browser` + printed manual URL + an `ssh -L` port-forward hint, but there is no
terminal fallback in MVP.

**What would make us revisit:** if issue reports show remote-SSH-without-GUI is the
dominant real usage pattern, reconsider a TUI companion mode (already slotted as P3 in
PRD §6 roadmap).
