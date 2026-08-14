# 0006. Project and binary are named `looq`

- **Status:** Accepted
- **Date:** 2026-08-08

## Context

The project was drafted as "Open Log Viewer" with the binary `logview`. Two problems
surfaced before any code existed, at the only moment they are cheap to fix.

"Open Log Viewer" already names an unrelated, established tool — a viewer for
automotive ECU logs — so the product would launch competing for its own name in search
results, against PRD §9's stated goal of 500 GitHub stars. `logview` as a command is
generic enough to collide with existing utilities and library classes across several
ecosystems.

Naming is also unusually irreversible here. crates.io never releases a name once
published, even for abandoned crates: `lq`, `logq`, `sift`, `loupe` and `scry` are all
permanently unavailable despite most being dead since 2019–2021. A name chosen after
first publish cannot be undone, only abandoned.

## Decision

The project, the binary and the crate are all named `looq` — verified free on
crates.io and npm, with no collision against installed commands. The MCP tools become
`looq_open` / `looq_query` / `looq_summarize` / `looq_list_files`, the core crate is
`looq-core`, and the env var is `LOOQ_ALLOWED_DIRS`.

The name reads aloud as "look" — the verb for what the tool does — while the `q`
signals the query/filter surface (PRD F-6, F-7) and places it in the same mental family
as `jq`, the tool this audience already reaches for on JSON logs.

## Alternatives considered

### `look`

Rejected on a hard blocker: `/usr/bin/look` is an existing Unix utility shipping its
own man page. A binary cannot claim that name.

### `lq`

The strongest short candidate, and the one that motivated `looq`. Rejected because the
crate is taken by an actively maintained project (v0.16, updated 2024), and because a
two-letter name is effectively unsearchable — a real cost for a project whose success
metric is public discovery. `jq` gets away with it on a decade of reputation that a new
tool has not yet earned.

### `logscope`

Free everywhere, professional, searchable, zero risk. Rejected as forgettable: it
describes the category rather than naming the product, and at eight characters it is
noticeably worse to type in the pipe position (`myapp | logscope`) that PRD US-2 puts at
the center of the product.

### Keeping `logview` (crate name is in fact free)

Rejected despite availability. The blocker was never registration — it was launching
into an existing brand's search results with a name that describes any tool in the
category.

## Consequences

**Good:** crate, binary, npm name and product name are all the same word, so
`cargo install looq` needs no explanation and no `git-delta`-style split between crate
name and command name. Four characters keeps the pipe form short. The name is
distinctive enough to own its search results.

**Bad / accepted cost:** `looq` is a deliberate misspelling, which some readers will
find affected, and it must be spelled out in speech ("looq with a q"). It carries no
meaning to someone who has not seen it used — the README's first line has to do the
work of explaining what it is, where `logscope` would have explained itself.

**What would make us revisit:** nothing short of a trademark conflict. Once the crate is
published the name is permanent, so the practical window for reversal closes at first
publish — which is also why this decision was made before writing code rather than
after.
