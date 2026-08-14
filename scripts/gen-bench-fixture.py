#!/usr/bin/env python3
"""Deterministically generate the ~1MB JSON-Lines fixture used for the day-4 WASM
parse benchmark (mvp-plan day 4, `browser-file-loading` spec "Parse throughput is
measured, not assumed").

Not committed to the repository (a ~1MB data file is not something worth reviewing as
text); regenerate it on demand. Deterministic (no timestamps, no randomness) so the
same command always produces a byte-identical fixture — that determinism is what makes
the recorded benchmark number reproducible.

Usage:
    python3 scripts/gen-bench-fixture.py > /tmp/bench-1mb.jsonl
"""
import sys

LEVELS = ["INFO", "DEBUG", "WARN", "ERROR"]
SERVICES = ["api", "db", "worker", "auth", "cache"]
MESSAGES = [
    "request completed",
    "cache miss",
    "connection retried",
    "job processed",
    "slow query detected",
    "config reloaded",
    "health check passed",
]

TARGET_BYTES = 1_000_000


def gen_line(i: int) -> str:
    level = LEVELS[i % len(LEVELS)]
    service = SERVICES[i % len(SERVICES)]
    message = MESSAGES[i % len(MESSAGES)]
    # Fixed epoch base + deterministic offset instead of wall-clock time, so the
    # file is byte-identical across runs and machines.
    ts_seconds = 1_754_668_800 + i
    return (
        '{"timestamp":"%d","level":"%s","service":"%s","message":"%s",'
        '"seq":%d,"request_id":"req-%06d"}\n'
        % (ts_seconds, level, service, message, i, i)
    )


def main() -> None:
    out = sys.stdout
    written = 0
    i = 0
    while written < TARGET_BYTES:
        line = gen_line(i)
        out.write(line)
        written += len(line)
        i += 1
    sys.stderr.write(f"generated {i} lines, {written} bytes\n")


if __name__ == "__main__":
    main()
