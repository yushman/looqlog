#!/usr/bin/env python3
"""Generates a deterministic JSON-Lines fixture with real ISO 8601 timestamps, for
exercising the `timeline-and-table` change's index/timeline/table at a target line
count (as opposed to `gen-bench-fixture.py`'s byte-target fixture with raw epoch
integers, which the timestamp extractor doesn't recognise).

Not committed — regenerate on demand, same convention as `gen-bench-fixture.py`.

Usage:
    python3 scripts/gen-timeline-fixture.py <line-count> [--jitter N] [--timestampless-every N] > out.jsonl

--jitter N: each line's timestamp is offset by up to N seconds earlier than a
strictly increasing baseline, producing a "badly out of order" fixture when N is
large relative to the 1-second base spacing.
--timestampless-every N: every Nth line omits the timestamp field entirely (0 disables).
"""
import sys
import random

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

BASE_EPOCH = 1_754_668_800  # 2025-08-08T17:20:00Z-ish, fixed so output is deterministic


def iso(ts: int) -> str:
    import datetime

    return datetime.datetime.fromtimestamp(ts, datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def main() -> None:
    if len(sys.argv) < 2:
        sys.stderr.write("usage: gen-timeline-fixture.py <line-count> [--jitter N] [--timestampless-every N]\n")
        sys.exit(1)
    n = int(sys.argv[1])
    jitter = 0
    timestampless_every = 0
    args = sys.argv[2:]
    i = 0
    while i < len(args):
        if args[i] == "--jitter":
            jitter = int(args[i + 1])
            i += 2
        elif args[i] == "--timestampless-every":
            timestampless_every = int(args[i + 1])
            i += 2
        else:
            i += 1

    rng = random.Random(42)  # fixed seed: deterministic output
    out = sys.stdout
    for idx in range(n):
        level = LEVELS[idx % len(LEVELS)]
        service = SERVICES[idx % len(SERVICES)]
        message = MESSAGES[idx % len(MESSAGES)]
        ts = BASE_EPOCH + idx - (rng.randint(0, jitter) if jitter else 0)
        if timestampless_every and (idx + 1) % timestampless_every == 0:
            out.write(
                '{"level":"%s","service":"%s","message":"%s","seq":%d}\n'
                % (level, service, message, idx)
            )
        else:
            out.write(
                '{"timestamp":"%s","level":"%s","service":"%s","message":"%s","seq":%d,"request_id":"req-%06d"}\n'
                % (iso(ts), level, service, message, idx, idx)
            )
    sys.stderr.write(f"generated {n} lines\n")


if __name__ == "__main__":
    main()
