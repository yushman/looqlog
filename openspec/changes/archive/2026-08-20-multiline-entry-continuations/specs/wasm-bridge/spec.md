## MODIFIED Requirements

### Requirement: Typed values cross the boundary
Entries, the detection result, the field inventory and the diagnostics SHALL cross the
JS↔WASM boundary as typed structures via `serde-wasm-bindgen`, and the TypeScript types
describing them SHALL be checked by `tsc --noEmit` in CI so a shape change in Rust cannot land
as a silent runtime mismatch. An entry's continuation link SHALL cross as a nullable number,
never as `undefined`, so consumers that test for absence with an explicit null check behave
correctly.

#### Scenario: Type check catches a shape change
- **WHEN** a field is renamed in the Rust structure without updating the TypeScript type
- **THEN** CI fails on the type check

#### Scenario: Entries arrive with their fields intact
- **WHEN** a logfmt fixture carrying `service=api` is parsed
- **THEN** the JS side receives entries whose field map contains `service` with value `api`

#### Scenario: The continuation link crosses as a number or null
- **WHEN** a fixture containing a stack trace is parsed
- **THEN** the JS side receives the chain root's entry with a null continuation link and
  each member's entry with the root's ordinal as a number
