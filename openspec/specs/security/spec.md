## Purpose

The `security` capability covers the CSP, the origin/host checks and per-process token on `/ws`
and `/upload` that close the cross-tab WebSocket hijacking risk named in TDR §13, and the stated
limits of those measures.
## Requirements
### Requirement: Content Security Policy on every response
The server SHALL send a Content Security Policy of `default-src 'self'` on every response, with
no `'wasm-unsafe-eval'` and no additional directive beyond what same-origin scripts, styles and
the `/ws` connection require. The policy SHALL NOT permit inline styles: `style-src` SHALL remain
as strict as `default-src`. The policy SHALL be verified in each browser PRD §11 lists as
supported.

#### Scenario: Policy is present
- **WHEN** any response is inspected
- **THEN** it carries the CSP header

#### Scenario: Inline styles are not permitted
- **WHEN** the CSP header is inspected
- **THEN** `style-src` does not contain `'unsafe-inline'`

#### Scenario: No WASM allowance
- **WHEN** the CSP header is inspected
- **THEN** it does not contain `'wasm-unsafe-eval'`

#### Scenario: Application still works under the policy
- **WHEN** the page is loaded in each supported browser
- **THEN** the page connects, shows a log, and no CSP violation is reported in the console

### Requirement: WebSocket origin check
The server SHALL reject WebSocket upgrade requests whose `Origin` header does not match the
origin it is serving, or whose `Host` header does not name the address it is bound to, before any
log data is sent.

#### Scenario: Foreign origin is rejected
- **WHEN** a page on another origin attempts to connect to `/ws`
- **THEN** the upgrade is refused and no log data is transmitted

#### Scenario: Rebound hostname is rejected
- **WHEN** an upgrade request arrives with `Host: attacker.example:7891`
- **THEN** the upgrade is refused

#### Scenario: The application's own page connects
- **WHEN** the served page connects to `/ws`
- **THEN** the upgrade succeeds

### Requirement: WebSocket token handshake
The server SHALL generate a random token per process, embed it in the served page, and require it
during the WebSocket handshake before answering any query or sending any notification. The token
SHALL NOT be placed in the request URL, and a connection that does not present it within a short
timeout SHALL be closed.

#### Scenario: Connection without a token
- **WHEN** a client connects to `/ws` and sends no token
- **THEN** the connection is closed without any log data being delivered

#### Scenario: Reload obtains a fresh page and a valid token
- **WHEN** the user reloads the page during an active stream
- **THEN** the new page fetches the token with the document and reconnects successfully

#### Scenario: Token is not in the URL
- **WHEN** the WebSocket connection is established
- **THEN** the token appears in the handshake payload rather than in the request URL, so it does
  not land in proxy or shell history

### Requirement: The limits of these measures are documented
The project SHALL state, where a user can find it, that the origin, host and token checks protect
against another page in the same browser and not against another process on the machine, and that
none of them protects a non-loopback `--host` binding (ADR-0003). It SHALL state that log
contents — files and stdin alike — cross a local process boundary and never leave the machine on
the default bind, and SHALL NOT claim that any log stays inside the browser.

#### Scenario: Documented threat model
- **WHEN** a reader consults the README or the security notes
- **THEN** the protection and its two stated gaps are described together, not implied to be
  general authentication

#### Scenario: No browser-only claim remains
- **WHEN** both READMEs and the UI copy are searched for "never leaves your browser" or its
  Russian equivalent
- **THEN** there is no match

### Requirement: Upload is authenticated and origin-checked
`POST /upload` SHALL require the per-process token in the `X-Looqlog-Token` header, an `Origin`
matching the served origin, and a `Host` naming the bound address. A request failing any check
SHALL be rejected with 403 before any of its body is parsed.

#### Scenario: Cross-site upload
- **WHEN** a page on another origin posts a body to `/upload`
- **THEN** the response is 403 and the current source is unchanged

#### Scenario: Missing token
- **WHEN** a same-origin request to `/upload` omits the token header
- **THEN** the response is 403 and nothing is parsed

