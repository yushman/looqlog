## MODIFIED Requirements

### Requirement: The WASM module reports a parse result to the page
The page SHALL parse the selected file through the real multi-format parser running in the
worker, and SHALL receive typed entries, the detection result, the field inventory and the
diagnostics — not a bare count. The provisional single-format entry point from the skeleton is
removed; format detection and multi-format parsing are the parser's own responsibility, and
the page SHALL NOT assume any particular format.

#### Scenario: Entry count matches the fixture
- **WHEN** the user opens `tests/fixtures/sample.jsonl`, which has a known number of lines
- **THEN** the page reports an entry count equal to that number, now derived from the real
  parser rather than the hardcoded one

#### Scenario: Any of the three formats works without configuration
- **WHEN** the user opens a JSON Lines, a logfmt or a plain-text fixture
- **THEN** each is detected and parsed correctly with no format hint from the CLI or the page

#### Scenario: Parse throughput is measured, not assumed
- **WHEN** a ~1MB JSON Lines fixture is parsed through the worker
- **THEN** the wall-clock duration is measured in the browser and recorded in
  `docs/devlog.md` alongside the <200ms/MB target from TDR §11, superseding the skeleton's
  stub measurement and including the cost of the worker boundary
