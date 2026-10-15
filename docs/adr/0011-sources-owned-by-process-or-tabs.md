# 0011. Sources are owned by the process or by tabs

- **Status:** Accepted
- **Date:** 2026-10-15

## Context

After ADR-0010 the backend owns parsed data, but a process held exactly one source: an upload
replaced the current file, every tab showed the same log, and file and stdin were exclusive
modes. Binding a browser tab to a specific log means a process holds several sources at once,
and then something has to decide when each one goes away and how much memory all of them may
take together.

## Decision

A process keeps a registry of sources. Each source gets an id of 8 lowercase base32 characters
from the OS random generator (40 bits), never reused within the process, so a stale `/l/<id>`
link from a previous run cannot silently open a different log. A tab is bound to one source at
`/l/<id>`; `/` is a landing page that lists the sources and accepts uploads.

| Kind | Owner | Closable | Unloaded when | Evicted by the budget |
|---|---|---|---|---|
| argv file | process | yes | closed | never |
| stdin (at most one) | process | no | never | never (bounded by `--max-lines`) |
| upload | tabs | yes | closed, or 10 minutes after its last tab disconnects | yes, oldest orphan first |

- **Argv files and stdin are owned by the process.** The user named them on the command line,
  so they stay until exit even with no tab open. An argv file can be closed from the landing.
  Stdin cannot: stopping reading would block or `SIGPIPE` the producer, and draining while
  discarding would make "closed" a statement about memory, not about data.
- **Uploads are owned by tabs.** Nothing outside the browser refers to them, so once no tab has
  been connected for 10 minutes they are unloaded. A reload reconnects well inside that grace.
- **One process-wide memory budget of 4 GiB**, counted from what each session actually retains
  (`Session::retained_bytes()`, calibrated against `mem_probe`). An ingest that would exceed it
  first unloads tabless uploads oldest-first; if it still does not fit, an upload is aborted
  with an error naming the budget and its partial source is dropped, and an argv file stops
  being read and shows the budget error. Stdin never trips the check by itself; its retained
  lines count toward the total that other ingests see.
- **Several stdin streams are several processes.** A process reads one stdin; a second stream
  runs a second `looqlog`, which takes the next free port in 7891–7900 when `--port` is not
  given. There is no shared server across processes.

The budget and the grace period are fixed, not flags.

## Alternatives considered

### Sequential ids

Rejected: `/l/2` bookmarked from a previous run would open whatever the new process happened to
load second, which is exactly the silent wrong answer this project avoids.

### Garbage-collect every source without a tab

Rejected for argv files and stdin: closing the last tab on a file the user passed on the command
line would lose it, and stdin cannot be re-read at all.

### A shared hub process for several stdin streams

Rejected in explore: cross-process discovery, ownership and shutdown for a case that two
processes on two ports already cover.

## Consequences

**Good:** several logs side by side, a tab that keeps its log across reloads, bounded memory
with a predictable eviction order, and a stale link that says the source is gone instead of
showing an empty table.

**Bad / accepted cost:** an upload tab left open across a laptop sleep of more than 10 minutes
wakes up to "source unloaded, upload again". The retained-bytes estimate is a guard, not an
accounting of RSS. A link only works against the process that created its source.

**What would make us revisit:** RSS exceeding the budget by more than 25% in the probe, or users
routinely losing uploads to the grace period.
