## ADDED Requirements

### Requirement: A process holds many sources with stable ids
The backend SHALL keep a registry of sources. Each source SHALL get an id of 8 lowercase base32
characters from the OS random generator when it is created, and an id SHALL never be reused within
the process. A source SHALL be one of: an argv file, the stdin stream (at most one per process), or
an upload.

#### Scenario: Three sources at once
- **WHEN** the user runs `printf 'a\n' | looqlog x.log y.log`
- **THEN** the registry holds three sources with three distinct ids: two files and one stdin

#### Scenario: Ids are not sequential
- **WHEN** two sources are created one after the other
- **THEN** their ids do not reveal their creation order

### Requirement: Process-owned sources live until exit
Argv files and stdin SHALL be owned by the process: they SHALL NOT be unloaded by GC or by budget
eviction. An argv file SHALL be closable by the user; the stdin source SHALL NOT be closable, because
stopping reading would block or kill the producer.

#### Scenario: Argv file survives no tabs
- **WHEN** an argv file has had no tab open for an hour
- **THEN** it is still listed and its URL still shows the log

#### Scenario: Stdin cannot be closed
- **WHEN** a close request names the stdin source
- **THEN** it is refused with a message saying a stream cannot be closed while it is being read

### Requirement: Uploads are owned by tabs
An upload SHALL be unloaded when it is closed explicitly, or when no tab has been connected to it
for 10 minutes. A tab reconnecting within that time (including a reload) SHALL keep the source.

#### Scenario: Reload keeps an upload
- **WHEN** a tab on an upload is reloaded
- **THEN** the upload is still there and the tab shows it

#### Scenario: Orphaned upload is unloaded
- **WHEN** the last tab on an upload closes and 10 minutes pass with no tab reconnecting
- **THEN** the upload is removed from the registry and its memory is released

### Requirement: A process-wide memory budget
The registry SHALL enforce a fixed 4 GiB budget over the retained bytes of all sources, measured
from what each session actually retains. When an ingest would exceed it, the backend SHALL unload
tabless uploads, longest without a tab first, never one still being received; if it still does not
fit, an upload SHALL be aborted with `507` and an error naming the budget, leaving every other
source intact, and an argv file SHALL stop being read and
be marked with that error. Process-owned sources SHALL never be evicted.

#### Scenario: Orphan makes room
- **WHEN** a new upload would exceed the budget and a tabless upload exists
- **THEN** the tabless upload is unloaded and the new upload continues

#### Scenario: Nothing to evict
- **WHEN** a new upload would exceed the budget and only process-owned or tab-held sources exist
- **THEN** the upload is aborted, its partial data is dropped, the uploader sees the budget error,
  and every other source is unchanged

#### Scenario: Argv file over budget
- **WHEN** an argv file does not fit in the budget
- **THEN** the process still starts and that source shows a memory-budget error

### Requirement: The landing lists sources live
The landing page SHALL list every source with its name, kind, size, state (parsing, live, ended,
error — a fully read file is labelled "loaded"), number of connected tabs, a link to its tab, and a close control for every source except
stdin. The list SHALL update without reload when sources are added, change state, or are removed.

#### Scenario: New upload appears in another landing tab
- **WHEN** two landing tabs are open and one uploads a file
- **THEN** the other tab's list shows the new source without reloading

#### Scenario: Closing from the landing
- **WHEN** the user closes an argv file from the landing
- **THEN** it disappears from the list and its open tabs show the closed state
