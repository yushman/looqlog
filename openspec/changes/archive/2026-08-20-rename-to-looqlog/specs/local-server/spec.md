## MODIFIED Requirements

### Requirement: Loopback-by-default HTTP listener
The server SHALL bind to `127.0.0.1` on port `7891` unless overridden by `--host` or
`--port`. `--port 0` SHALL allocate a free port from the operating system and the CLI SHALL
report the allocated port. A port already in use SHALL fail with a clear message naming the
port, not a panic or a backtrace.

#### Scenario: Default bind
- **WHEN** `looqlog app.log` is running with no overrides
- **THEN** `curl http://127.0.0.1:7891` returns HTTP 200

#### Scenario: Random port allocation
- **WHEN** the user runs `looqlog --port 0` twice in a row
- **THEN** each run reports a port that is open and, with high probability, different

#### Scenario: Occupied port fails cleanly
- **WHEN** port 7891 is already bound by another process and the user runs `looqlog app.log`
- **THEN** the process exits non-zero with a message naming port 7891 and suggesting
  `--port 0`, with no panic output
