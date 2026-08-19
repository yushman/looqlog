## MODIFIED Requirements

### Requirement: Content Security Policy on every response
The server SHALL send a Content Security Policy of `default-src 'self'` with any additional
directives strictly required for same-origin WASM compilation and worker creation, on every
response. The policy SHALL NOT permit inline styles: `style-src` SHALL remain as strict as
`default-src`, since the virtual scroller no longer writes `style` attributes. The policy SHALL
be verified in each browser PRD §11 lists as supported.

#### Scenario: Policy is present
- **WHEN** any response is inspected
- **THEN** it carries the CSP header

#### Scenario: Inline styles are not permitted
- **WHEN** the CSP header is inspected
- **THEN** `style-src` does not contain `'unsafe-inline'`

#### Scenario: Application still works under the policy
- **WHEN** the page is loaded in each supported browser
- **THEN** the WASM module compiles, the worker starts, and no CSP violation is reported in the
  console
