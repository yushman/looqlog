## MODIFIED Requirements

### Requirement: Upload is authenticated and origin-checked
`POST /upload` and `POST /sources/<id>/close` SHALL require the per-process token in the
`X-Looqlog-Token` header, an `Origin` matching the served origin, and a `Host` naming the bound
address. A request failing any check SHALL be rejected with 403 before any of its body is parsed
and before any source is created or closed.

#### Scenario: Cross-site upload
- **WHEN** a page on another origin posts a body to `/upload`
- **THEN** the response is 403 and no source is created

#### Scenario: Missing token
- **WHEN** a same-origin request to `/upload` omits the token header
- **THEN** the response is 403 and nothing is parsed

#### Scenario: Close of an unknown id
- **WHEN** an authenticated same-origin request closes an id that does not exist
- **THEN** the response is 404

#### Scenario: Cross-site close
- **WHEN** a page on another origin posts to `/sources/<id>/close`
- **THEN** the response is 403 and the source stays open
