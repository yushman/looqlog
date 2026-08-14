## ADDED Requirements

### Requirement: Content Security Policy on every response
The server SHALL send a Content Security Policy of `default-src 'self'` with any additional
directives strictly required for same-origin WASM compilation and worker creation, on every
response. The policy SHALL be verified in each browser PRD §11 lists as supported.

#### Scenario: Policy is present
- **WHEN** any response is inspected
- **THEN** it carries the CSP header

#### Scenario: Application still works under the policy
- **WHEN** the page is loaded in each supported browser
- **THEN** the WASM module compiles, the worker starts, and no CSP violation is reported in the
  console

#### Scenario: External resources are refused
- **WHEN** the page attempts to load any resource from a different origin
- **THEN** the browser blocks it

### Requirement: WebSocket origin check
The server SHALL reject WebSocket upgrade requests whose `Origin` header does not match the
origin it is serving, before any stdin data is sent.

#### Scenario: Foreign origin is rejected
- **WHEN** a page on another origin attempts to connect to `/ws`
- **THEN** the upgrade is refused and no stdin line is transmitted

#### Scenario: The application's own page connects
- **WHEN** the served page connects to `/ws`
- **THEN** the upgrade succeeds

### Requirement: WebSocket token handshake
The server SHALL generate a random token per process, embed it in the served page, and require it
during the WebSocket handshake before streaming any data. The token SHALL NOT be placed in the
request URL, and a connection that does not present it within a short timeout SHALL be closed.

#### Scenario: Connection without a token
- **WHEN** a client connects to `/ws` and sends no token
- **THEN** the connection is closed without any stdin line being delivered

#### Scenario: Reload obtains a fresh page and a valid token
- **WHEN** the user reloads the page during an active stream
- **THEN** the new page fetches the token with the document and reconnects successfully

#### Scenario: Token is not in the URL
- **WHEN** the WebSocket connection is established
- **THEN** the token appears in the handshake payload rather than in the request URL, so it does
  not land in proxy or shell history

### Requirement: The limits of these measures are documented
The project SHALL state, where a user can find it, that the origin check and token protect
against another page in the same browser and not against another process on the machine, and that
neither protects a non-loopback `--host` binding (ADR-0003).

#### Scenario: Documented threat model
- **WHEN** a reader consults the README or the security notes
- **THEN** the protection and its two stated gaps are described together, not implied to be
  general authentication
