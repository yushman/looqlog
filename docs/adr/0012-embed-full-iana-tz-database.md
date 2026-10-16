# 0012. Embed the full IANA time zone database in looqlog-core

- **Status:** Accepted
- **Date:** 2026-10-16

## Context

Timestamps without an offset were read as UTC or as a fixed `#tz=<minutes>` offset. A log
written in `Europe/Belgrade` local time needs a different offset in summer and winter, so a
fixed number is wrong for half the year. The earlier scope cut (no named zones) was about
`core.wasm` size and stopped applying with ADR-0010. `looqlog-core` does no I/O (ADR-0005),
so it cannot read the system zoneinfo.

## Decision

`looqlog-core` depends on `chrono-tz` with the full database and no regex filter. `#tz=` accepts
an IANA name (`Europe/Berlin`) next to the minutes form; the offset is resolved per timestamp.
An ambiguous local time (clocks going back) takes the earlier instant; a nonexistent one (clocks
going forward) uses the offset in effect before the transition. An unknown name is a visible
error from the backend, not a silent drop.

Measured on macOS aarch64, release build: 3386288 B before, 4575568 B after (+1.19 MB, +35%).
The 10 MiB release gate applies to the Linux x86_64 binary and is still checked in CI. Parse
benchmarks of the fixed-offset and UTC paths did not change significantly.

## Alternatives considered

### `jiff`

Rejected: a second date/time type stack beside `chrono`, which the parser already uses.

### `chrono-tz` with `filter-by-regex` (a subset of zones)

Rejected: some zones would silently stop working, which is the failure this change removes.

### System zoneinfo

Rejected: I/O inside core, and the result depends on the platform and its installed tzdata.

## Consequences

**Good:** correct offsets across DST for any zone, no I/O in core, one binary on every platform.

**Bad / accepted cost:** about 1.2 MB of binary growth. The tzdb version is frozen per release,
so a zone rule changed by a government after the release is wrong until the next one. Times are
still displayed in UTC. `Session::set_format` takes a `TzOverride` instead of minutes, which
is semver-breaking for `looqlog-core` 0.x.

**What would make us revisit:** the binary nearing the 10 MiB gate, or a need for tzdb updates
without a release.
