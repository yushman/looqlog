## MODIFIED Requirements

### Requirement: Filtering performance
Applying or changing a filter over 10,000 entries SHALL complete within the 50ms target in
TDR §11, measured from the query request to the reply on the backend and recorded rather than
assumed. The native evaluation cost SHALL additionally be benchmarked with `cargo bench` on
`looqlog-core` at 10,000 and 1,000,000 entries.

#### Scenario: Measured filter latency
- **WHEN** a filter is toggled on a 10,000-entry dataset
- **THEN** the measured time to the query reply is recorded against the target, and a miss is
  either fixed or explicitly documented

#### Scenario: Large dataset is benchmarked
- **WHEN** the benchmark runs at 1,000,000 entries
- **THEN** the result is recorded in `docs/devlog.md`
