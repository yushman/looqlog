# web

TypeScript thin client: the landing and per-source views over the `/ws` query API. The browser
holds only the rows on screen; parsing, retention and querying are the backend's (ADR-0010).

## Build
- Vite, strict TypeScript, Web Components. CI runs `tsc --noEmit` and fails on any type error. The
  served HTML has no inline application script: it loads the bundle only.
- `scripts/build-frontend.sh` vendors the build (fixed, unhashed file names) into
  `crates/looqlog/assets/`.
- No external fonts, stylesheets or images: everything ships in the bundle, so the app works offline
  under CSP `default-src 'self'`. No `style` attributes are written; dynamic geometry (row offsets,
  spacer height, column widths) goes through stylesheet rules mutated via the CSSOM, because the CSP
  forbids inline styles.

## Routing and boot
- `/` mounts `looqlog-landing`; `/l/<id>` mounts a source. The `#mode-template` JSON picks the file
  shell (`looqlog-app`) or the stdin shell (`looqlog-live-tail`); its unsubstituted placeholder
  (`vite dev`) means file mode. `token.ts` reads `#token-template`; the placeholder means no token.
- A new tab at `/` starts on the landing and inherits no source, filters or stream from other tabs.

## Query client
- `QueryClient` opens one WebSocket per tab and sends `{type: "auth", token, source}` first (`source`
  is null on the landing). Replies resolve by id; `error` rejects with `RequestError`; a closed socket
  rejects pending requests with `ConnectionLostError`.
- States: `connecting`, `live`, `ended`, `disconnected`. It reconnects with backoff from 500 ms
  doubling to 10 s; after every authentication `onConnected` re-issues the current query and window,
  so each retained entry shows exactly once. A `closed` first frame skips the re-query.
- `ViewSession` keeps the applied state and its generation and drops `window`/`buckets` replies of an
  older generation. After a compile error the applied state keeps the previous search; extracted
  `field=value` tokens merge into the applied filters.
- `query-types.ts` mirrors the server's DTOs and protocol by hand.

## Workspace
- One layout serves file and stdin mode: a full-width timeline on top, the filter rail, the entry
  table, and the detail pane. It is bound to the viewport; panes scroll independently and the document
  never scrolls.
- Rail and detail collapse from a control on the pane itself, leaving a strip with that control and the
  pane's name in vertical text. Collapsed content is `visibility: hidden`, so only the strip's control
  is in the tab order.
- Below 1100 px the panes stack. The width is defined once, in `style.css`'s media query, and published
  as `--narrow-layout` for `isNarrowLayout()`. Collapsing hides the stacked block and serialises the
  same way in both layouts.
- Selecting a row expands a collapsed detail pane; collapsing it keeps the selection.
- Rail surfaces (open another file, detection, diagnostics, privacy, share) are `<details>` that may
  start collapsed, with their status in the summary. A surface that needs attention dispatches
  `rail-attention`: the rail's control or strip shows an indicator, and `autoExpand` opens both the
  surface and the rail.

## File shell (`looqlog-app`)
- Owns source-level state (source info, stats, progress, errors, gone reason) and the URL hash.
  `ViewController` owns filters, search and range and feeds the table, timeline, detail pane and rail;
  components render and dispatch events (`filters-change`, `range-change`, `selection-change`) and
  never wire to each other.
- While a file is read, redraws are throttled: table 300 ms, timeline 500 ms, stats 1 s.
- The status line distinguishes parsing (with %), an empty file, lines read but no entries (suggesting
  `#format=`), and normal counts.
- Errors stay in a dismissable banner until dismissed or superseded, never only a toast. The size
  warning shows as a note; a lost connection shows a retrying note.
- A source that failed with no entries, or is gone, replaces the table with a prompt: the reason
  (`goneMessage` per `unknown`, `closed`, `unloaded`, `budget`), an uploader and a link to the source
  list — never an empty table.
- `reset` reloads stats and re-queries with the tab's filters kept.

## Landing
- Lists sources from the pushed `sources` snapshots, live: name, kind (file, stdin, upload), size,
  state (a finished file reads "loaded"), tab count, a link, and a close control for every source but
  stdin.
- Offers the uploader (file picker and drop target).

## Upload
- Picking and dropping take the same path. Preflight: an empty file is an error; above 400 MiB it is
  refused with the reason; above 50 MiB the user confirms or cancels; a NUL byte in the first 8 KiB
  asks whether to proceed with what does not look like a text log. The thresholds in `limits.ts`
  mirror the backend's.
- The file streams as the `POST /upload` body with `X-Looqlog-Token` and a percent-encoded
  `X-Looqlog-Filename`. Success navigates the tab to `/l/<id>`; failure shows the backend's `error`
  text in a persistent banner. The page reads nothing of the file beyond the 8 KiB sniff.
- A tab showing a source never swaps it out; another file opens from the landing or a new tab.

## Entry table
- Virtual scrolling with fixed 24 px rows (`ROW_HEIGHT`; a test asserts `style.css` agrees) and 8 rows
  of overscan. It keeps only a cache of rows around the viewport, fetched with `window` requests.
- Columns: `#`, timestamp (header says UTC; the backend sends UTC instants), level, message. The level
  is a color-coded one-letter badge with the full name in `aria-label` and `title`. A missing timestamp
  or level is an em-dash absence marker with an explanatory label, inside its own column.
- Messages render on one line, truncated at 300 characters with `…`; the full text is in the detail pane.
- Rows are reused, keyed by ordinal, selection, match ranges and chain state; unchanged rows are not
  recreated, so a click or focus survives live updates.
- A chain root carries an expand/collapse `<button>` and "N lines"; members render indented. Chains
  start collapsed; collapse state lives in the backend view, so scrolling never resets it; a search
  match on a member expands it. An orphaned member is an ordinary row.
- On refresh the top visible row's ordinal is re-found with an anchored window, so a paused view does
  not jump; if that entry was evicted, a banner says earlier entries are gone and the view jumps to the
  oldest retained one.
- An eviction marker above the first row names the evicted line count (`--max-lines`).
- The summary states shown against total, says when filters exclude every entry, mentions an active
  range, and counts timestampless entries a range excludes.
- Column resize: drag handles exist only in the header (pointer capture). `#`, timestamp and level are
  resizable in rem, defaults 3 / 12.5 / 2.5, floors 2 / 3 / 2, ceiling 60, clamped while dragging; the
  message column takes the rest. Double-clicking a handle resets that column; "Reset columns" resets all.

## Detail pane
- Always present; says "Select an entry to inspect" with no selection. Follows the selected ordinal and
  fetches `entry`: the message with highlights, every field (nested JSON readable) and the raw line. An
  evicted selection says the entry is no longer retained rather than showing another one.

## Filter rail
- Sections: Search, Time range (typed from/to), `level`, then one collapsible section per inventory
  field, the first 3 open. A collapsed section still names its field and its value count.
- Values are listed with counts. A high-cardinality field, or one with more than 50 values
  (`CHIP_LIST_MAX_VALUES`), gets a typed-value input instead.
- Controls update in place as counts change and are added or removed only when the set of fields or
  values changes, so clicks, focus and half-typed text survive a live stream.
- The UI text states the combination rule. The count reads `matched / total entries`, or
  "0 of N — filters exclude every entry". Every filter is removable; "Clear all" clears them.
- Search is debounced; Escape clears only the query, at once. A compile error shows inline
  (`role="alert"`) and the previous results stay. `field=value` tokens the backend extracted become
  chips.

## Timeline
- A uPlot chart of backend buckets: filtered counts over a subordinate unfiltered series, so a filter
  that matches nothing still shows the data's shape.
- Dragging selects a range (`setSelect` → `range-change`), which narrows the table at once; "Clear
  range" removes it. The range is absolute and does not follow live data.
- Outliers outside the default span are reported with a "Show full range" button.
- With no usable timestamps it says so instead of drawing an empty chart; the timestampless count is
  shown.

## Detection and diagnostics
- Detection shows the format, the match fraction and the timestamp shape. A fallback, or a threshold
  win below 0.9, is flagged and opens the section. The override is `#format=` / `#tz=`; there is no
  picker.
- Diagnostics show the skip count and severity in the summary, and per-reason counts and retained
  examples when expanded. A skip ratio above 0.2 is severe and opens the section and the rail.

## Live tail (`looqlog-live-tail`)
- The indicator shows Connecting…, LIVE, Stream ended, or that the connection was lost and is retrying.
  Lines/sec comes from cumulative total plus evicted, every 1 s.
- Renders are batched: table 80 ms, timeline 500 ms, stats 1 s.
- The view follows the tail while within 24 px of the bottom; scrolling away pauses it and shows
  "Resume following"; the button or returning to the bottom resumes.
- While paused, the button reads `Resume following · <n> new` (thousands separated, `12,408`), plain
  `Resume following` at zero. n is the growth of cumulative total plus evicted since the pause,
  clamped at 0, so it stays correct under eviction and a reset or reconnect cannot make it negative.
  It counts all arrived lines, not only filter-matched ones. It refreshes on the 80 ms table tick
  and restarts from 0 on each pause.
- The page retains no entries beyond the rows it renders; the count shows the backend's retained
  total and eviction shows as the marker.
- The stream's detected format can be overridden the same way as in file mode: `#format=` and `#tz=`
  in the URL hash apply in stdin mode. The override is sent as `set_format` once, on the first connect,
  before stats and the first query, and never re-sent on reconnect: the session already holds it. Other
  tabs on the source get the `reset` notification.
- An override the backend rejects (e.g. an unknown format or time zone) shows its error in the visible
  error note; the stream keeps loading under detection. `format` and zone names are not validated
  client-side.
- The applied override is written back into the hash, so it survives state changes and copy-link.

## URL hash
- The fragment is `URLSearchParams`: `range=<startMs>,<endMs>`, repeated `filter=<field>=<value>`,
  `q=<text>`, `format=json|logfmt|plain`, `tz=<minutes east | IANA name>`,
  `cols=<#>,<timestamp>,<level>` (rem, only when not default), `panes=rail|detail|rail+detail` (the
  collapsed panes, absent when none; `+`, `,` and spaces all separate).
- `tz` is an integer with absolute value below 1440 (sent as `tzOffsetMinutes`), or a token matching
  `^[A-Za-z][A-Za-z0-9_+\-/]*$` (sent as `tzName`, e.g. `Europe/Moscow`); anything else (`1440`,
  `1.5`, `+03:00`) is dropped without a notice. Spaces in the value turn back into `+` first, so a
  hand-typed `Etc/GMT+3` survives `URLSearchParams`. Names are not validated client-side: an unknown
  one shows the backend's `could not apply the format override: unknown time zone: <name>` in the
  visible error note. At most one of the two is set, and it is written back verbatim.
- Written with `history.replaceState`, debounced, so typing does not grow history.
- On load in file mode: panes apply at once; a format or timezone override is sent as `set_format`
  before the first query; range, filters (checked against the field inventory) and query form the first
  query. Unknown keys, a bad range, malformed filters, unknown fields, bad widths (default or clamped)
  and unknown pane names are named in a visible notice while the valid parts apply; the log always loads.
- On load in stdin mode only `format` and `tz` are read; `range`, `filter`, `q`, `cols` and `panes` are
  written, never read — an absolute range is meaningless for a live stream and filter checks would race
  the detection hold.
- The copy-link affordance shows `SHARE_CAVEAT` when copying: the link carries search text and field
  values, which are log fragments, and names a source of this running process, so it fails elsewhere or
  after exit.

## Theme
- Follows `prefers-color-scheme`. The toggle cycles Auto → Light → Dark → Auto and stores the choice in
  `localStorage` (`looqlog-theme`, applied as `data-theme`). The six levels have six distinct colors in
  both themes.

## Privacy copy
- The privacy surface in every mode says the data is read by the local `looqlog` process and never
  leaves the machine on the default bind.
