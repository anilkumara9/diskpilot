# DiskPilot architecture

DiskPilot is a Tauri 2 desktop app with a strict split:

- **`crates/core`** (`diskpilot-core`) — the Rust engine. All analysis,
  every safety decision, and the only code that can remove files.
- **`src-tauri`** — the desktop shell: window, tray, settings
  persistence, and typed commands that bridge to the core.
- **`src/`** — the React/TypeScript frontend. It renders and asks; it
  cannot touch the filesystem. Its only channel to the machine is the
  typed command set, and its Tauri capability set grants nothing else
  (no fs, no shell — just window dragging, the folder-picker dialog and
  opening external links).

The split is the security story: a compromised or buggy UI cannot
delete anything, because deletion exists only behind the core's safety
gate, and the UI can only *request* plans by recommendation ID.

## The safety pipeline

```text
┌──────────┐  ┌────────────┐  ┌─────────────────┐  ┌─────────┐
│ Scanner  │─▶│ Classifier │─▶│ Recommendations │─▶│ Planner │
│ (read)   │  │ (pure)     │  │ (explain only)  │  │(preview)│
└──────────┘  └────────────┘  └─────────────────┘  └────┬────┘
                                                        │
┌──────────┐  ┌────────────┐                            │
│ History  │◀─│ Executor   │◀────── Safety gate ────────┘
│ (SQLite) │  │ (act)      │        (validate)
└──────────┘  └────────────┘
```

Only the executor removes files, and it only ever executes a plan that
the safety gate has validated — twice (at plan time, and again
immediately before execution, so a path swapped for a link after
planning is caught).

### Scanner (`scanner`)

A parallel walker built on rayon: `read_dir` + `symlink_metadata`,
never following symlinks, junctions or other reparse points, never
crossing onto another volume (checked by volume serial). On Windows it
records *allocated* size (`GetCompressedFileSizeW`), counts hard links
once (volume serial + file index), counts OneDrive placeholders at
their local size without opening them (which would trigger a download),
and uses extended-length `\\?\` paths so trees deeper than 260
characters still scan. Progress is reported through a callback roughly
every 120 ms; cancellation is a shared atomic flag. The walk produces a
tree of directory nodes plus scan facts: totals per category, the 500
largest files, heavy folders, and lists of errors and skipped folders
(nothing is silently dropped — unmeasurable folders surface on the
Overview).

**Byte accounting.** Every user-facing byte figure in the product is
*allocated* bytes on disk — the space a file actually occupies and the
space a cleanup actually frees. The scanner's `allocated_size` helpers
are the single measuring point, and the index, search hits, cleanup
cards, download provenance, duplicate per-file sizes and wasted totals,
and cleanup results all report allocated bytes, so the same file shows
the same size on every page. Logical file length is used only where it
is a correctness input (duplicate size-grouping and hash staging) and
is never displayed.

#### Fast path: reading the Master File Table (`mft`, `mft_volume`)

When the scan root is a whole drive (`C:\`) and the process can open
the raw volume — which Windows only allows for administrators — the
scanner skips the directory walk entirely and reads the NTFS Master
File Table directly, turning a full path-by-path enumeration into a
single sequential read. There is no settings toggle and no elevation
prompt: the app never asks for rights it was not started with; the
fast path simply engages when access already exists and is otherwise
invisible.

The design is three layers with a hard fallback between each:

1. **Parser (`mft`)** — pure functions over bytes, no IO: the boot
   sector (record size from the clusters-per-file-record byte, both
   the power-of-two and clusters branches), runlists (signed LCN
   deltas, sparse runs), and file records (Update Sequence Array
   fixup validated per sector — a mismatch marks the record corrupt
   and it is skipped, never fatal), extracting `$STANDARD_INFORMATION`,
   `$FILE_NAME` (all namespaces), the unnamed `$DATA` stream
   (resident content or a non-resident runlist) and
   `$ATTRIBUTE_LIST`. Attribute lists are followed into extension
   records, so a fragmented file's data and names merge back into one
   logical file.
2. **Assembly (`mft`)** — records become flat entries: deleted
   records skipped, one entry per hardlink name (Win32 namespace
   preferred, then Win32&DOS, POSIX, DOS; 8.3 aliases never
   double-count a file that also has a long name), full paths built
   by walking parent references to the root record with cycle and
   orphan guards, and reparse points — directories and files —
   skipped exactly as the walker skips them, so junctions and
   symlinks are never followed on either path.
3. **Volume IO (`mft_volume`)** — Windows-only: opens `\\.\X:` for
   reading, bootstraps the $MFT's own run map (its later extents
   live in extension records inside the MFT itself), then streams
   the table in ≥ 4 MB chunks. Any failure anywhere — access denied,
   non-NTFS, an implausible boot sector, an unparseable $MFT, or the
   sanity check (a volume read that produced zero files) — returns a
   typed "unavailable" and the caller runs the standard walk, with
   no user-visible error.

The assembled entries feed a virtual walk in the scanner that mirrors
the real one step for step — same classifier calls with the same
inheritance, same exclusions, same aggregation — and both paths
finish through the same summary code, so snapshots, recommendations,
history and every downstream page are computed identically whichever
engine ran. The summary records which one it was (`scanEngine`:
`"mft"` or `"walker"`).

**Allocated-size convention.** The byte rule above still holds, with
one documented approximation: a non-resident file's allocated size
is the sum of its data runs × the cluster size — its true on-disk
allocation. A resident file has no runs (its content lives inside
its MFT record), so it is counted at one record size (typically
1 KB). The walker instead asks the OS per file
(`GetCompressedFileSizeW`), which can differ for compressed or
sparse files; the divergence is bounded by the compression actually
applied and never affects logical sizes, grouping or hashing.

### Classifier (`classifier`)

A pure function from paths (and tree position) to a `Classification`:
one of eleven categories — Cache, Build artifacts, Developer tools,
Docker storage, Application data, Logs, Downloads, User data,
Binary/blob data, System files, Unknown — plus the rule ID that
matched, a human explanation, a confidence, and `inheritedFrom` when
the classification came from an ancestor.

Rules are a plain data table; first match wins. **Container** rules set
the category for a subtree while allowing deeper rules to refine it;
**Leaf** rules claim a whole subtree outright. Files inherit their
folder's classification except inside containers, where extension rules
refine them (`.gguf`, `.safetensors`, `.iso`, `.zip`, … → Binary data).
Matching is case-insensitive on Windows. Classification never looks at
size and never decides deletion — it only explains.

### Recommendations (`recommend`)

Policies turn classifications and analyzer findings into
recommendations. Each carries: title, paths, bytes, tier
(*Likely safe* / *Review first* / *Keep*), the methods allowed,
`whyDetected`, `whySafe`, `risks`, confidence, an optional
`alternativeCommand` (the tool's own hygiene command, when one exists —
shown to the user instead of deleting behind the tool's back), and a
source tag. Recommendations are pure data; building them changes
nothing on disk.

One recommendation is *guided only*: a `Windows.old` folder at the
scan root (the previous Windows installation) is flagged with
`guidedOnly` and a `guidance` text naming the removal route through
Windows' own storage tools. The planner rejects guided-only items
outright, and the safety gate treats the location as report-only, so
no plan can ever contain the folder.

### Planner & safety gate (`cleanup`, `safety`)

The frontend sends recommendation **IDs only**. The planner resolves
IDs against the engine's own recommendation set, so the UI cannot
inject paths. The safety gate then validates every resolved path:

- absolute, with no `.` / `..` components or NUL bytes;
- inside the user's home folder **and** inside the scanned root;
- outside the protected list (see [WINDOWS.md](WINDOWS.md));
- no symlink, junction or reparse point in the path or any parent.

The resulting `CleanupPlan` lists exact paths and bytes, warnings, and
any rejected items (with reasons). Each item's bytes are the scan
index's measured allocated bytes for that exact path — never a share
of a recommendation total; a path the scan cannot verify is dropped
from the plan rather than estimated. The plan records whether
permanent deletion is allowed — only when *every* item is
regenerable. Plans are single-use and expire 10 minutes after
creation, and asking to execute a plan in a mode it does not allow is
refused *before* the plan is consumed, so the same plan can still be
executed the allowed way.

### Executor (`cleanup`)

Execution re-runs the safety gate on every path immediately before
acting. Default action: move to the Recycle Bin (the `trash` crate,
which uses the Windows Recycle Bin APIs). Permanent deletion uses a
link-safe routine that never follows symlinks. Outcomes — paths, bytes
reclaimed, per-item errors — are returned to the UI and written to
History.

## Analysis modules

- **Duplicates (`duplicates`)** — two-phase: group by identical size,
  then SHA-256 the survivors. Hidden folders, AppData, dependency
  folders and package caches are skipped; hard links count once; NTFS
  clones/deduped files are flagged as "removing frees little". Before
  any copy moves, it is re-verified byte-for-byte against the group
  hash; the keeper copy is never touched.
- **Docker (`docker`)** — locates the `docker` CLI in known install
  locations (GUI apps don't inherit a shell PATH), invokes it with an
  argument vector (never a shell string), a minimal PATH and a timeout.
  Parses `docker system df -v` text into images, containers, volumes
  and build cache. Actions are a closed enum — remove stopped
  container, remove unused image, prune unused build cache — each
  producing a preview (exact command + reclaimable bytes) and
  re-validated at execution. There is no code path that removes a
  volume or passes `--force` to `rm`. The WSL2 vhdx diagnostic compares
  the virtual disk's file size with Docker's own reclaimable figure.
- **Developer tools (`devtools`)** — one analyzer per tool family
  (editors, AI agents, Node, Rust, Python, .NET, Visual Studio /
  JetBrains, local model stores). Analyzers locate known storage
  locations under the profile, measure them with the same allocated-size
  rules as the scanner, and classify each item with a state: current,
  stale, orphaned, superseded or informational. Other applications'
  databases (e.g. editor state DBs, chat DBs) are opened read-only;
  analyzers never modify anything.
- **Applications (`apps`)** — installed-app inventory (registry
  uninstall keys + package locations) joined with measured AppData
  Local/Roaming footprints. Data folders whose owning app is gone
  become *leftovers*: reported, never auto-removed.
- **Secrets (`secrets`)** — offline pattern matching for known
  credential formats over AI-assistant transcript files. Findings carry
  a masked value only (prefix/suffix characters), the file, and a
  count. The raw transcript text is never stored or transmitted.
- **Disk (`disk`)** — volume information (total / used / available /
  name) for the Overview figures and the tray ring.
- **Drives (`drives`)** — enumeration of every mounted volume
  (drive letters on Windows, `/` elsewhere) with total and free
  space, probed via `fs2`; volumes that do not answer or report zero
  total are dropped.
- **Cloud (`cloud`)** — OneDrive awareness. The local folder is
  found from the `OneDrive` / `OneDriveConsumer` /
  `OneDriveCommercial` environment variables; its on-disk footprint
  is computed from a scan index (files under the root occupying
  allocated bytes). "Free up space" marks paths unpinned
  (`attrib +U -P`) so OneDrive dehydrates them to online-only —
  nothing is deleted, and any path outside the detected root refuses
  the whole call.
- **Snapshot (`snapshot`)** — the last scan summary, persisted as JSON
  in the app data folder so the Overview is populated on launch before
  a fresh scan. Each scan also records the scanned drive's free space
  (`driveFreeBytes`) at scan end.
- **Forecast (`forecast`)** — a pure least-squares fit over the scan
  history: daily growth in bytes, and — when the drive is growing and
  the latest scan knows the free space — the projected date it fills.
  Needs at least two points spanning 12 hours inside the last 30 days.
- **Snapshot diff (`snapshot_diff`)** — the directory-level diff
  between the current and previous snapshots behind "biggest
  changes": grown / shrunk / added / removed directories, from the
  snapshots' stored directory sizes (or, when a snapshot carries
  none, prefix aggregation over its file list). All bytes are the
  scanner's allocated bytes.
- **Media (`media`)** — photos / videos / screenshots totals over a
  scan index, classified with the search module's kind rules so the
  Media page and file search always agree; screenshots are images
  that also match the screenshot rule, so the screenshots bucket is
  a subset of photos. Carries the 15 biggest photos and videos.
- **Games (`games`)** — installed-game inventory read from the
  launchers' own manifests, no scan involved. Steam: the install
  root found under the Program Files candidates, every library from
  `steamapps/libraryfolders.vdf` (parsed by a tolerant KeyValues
  mini-parser), each game's name and `SizeOnDisk` from its
  `appmanifest_*.acf` — a manifest without a recorded size falls
  back to measuring the install folder (allocated sizes, entry
  budget capped). Epic: the JSON `.item` manifests under
  `%ProgramData%\Epic\EpicGamesLauncher\Data\Manifests`, grouped by
  the folder containing each install location. Malformed manifests
  are skipped; no launcher installed means an empty report.
- **Storage extensions (`extensions`)** — per-extension aggregation
  over a scan index (allocated bytes and file counts, extensionless
  files grouped, case-folded) behind the Storage Types view, plus
  the largest files of any one type. Pure index math — the figures
  can never disagree with the other views of the same scan.
- **App junk (`appjunk`)** — a curated rule table of known,
  regenerable per-app cache locations (Discord, Slack, Teams,
  Chrome, Edge, Firefox, VS Code, NVIDIA, the DirectX shader cache,
  Windows Update downloads, Explorer thumbnail/icon caches),
  expressed as templates over the standard Windows roots (one
  single-segment wildcard for browser-style profile folders, one
  filename glob for cache databases). `measure_app_junk` resolves
  the rules against caller-supplied roots, walks each location with
  the scanner's allocated-size rule — bounded, never following
  symlinks — and returns one group per app. Claims are collapsed
  before measuring: a location nested inside another rule's location
  (or the same directory reached twice) is counted once, and a group
  whose walk hit the depth/entry bounds is flagged `truncated`
  rather than presented as exact. Profiles, history, cookies and
  offline/downloaded data are never in the table by construction.
  Cleaning is a shell command (below), not a planner flow: it
  re-measures server-side, acts only on the fresh measurement's
  paths, and moves contents to the Recycle Bin item by item.
- **Drive health (`smart`)** — pure parsers for the two SMART wire
  formats: the 512-byte ATA attribute table (30 records, raw values
  as 6-byte little-endian) and the NVMe SMART/Health log page, read
  at the specification's offsets. Constructors turn parsed data into
  a `DriveHealth` (life left, temperature, power-on hours, power
  cycles, reallocated sectors, media errors) and `classify` reduces
  it to Healthy / Caution / Critical / Unknown. Two honesty rules
  are encoded, not documented around: ATA attribute 231 is dual-use
  (SSD life on some drives, temperature on others), so life is taken
  from it only when attribute 194 independently secures the
  temperature — otherwise 231's byte is the temperature fallback and
  no life is reported; and an all-zero NVMe log (a short or zeroed
  read) yields Unknown vitals, never a fabricated healthy drive.
  Values a drive does not report stay `None` end to end.
- **Winget (`winget`)** — a pure parser for `winget upgrade` table
  output: column spans come from the header line (winget pads names,
  so rows are sliced by offsets, never split on whitespace), spinner
  garbage and CRLF redraws before the header are skipped, and the
  pinned-packages section and summary footer terminate the table.
  Rows whose package ID fails `is_valid_package_id` — including IDs
  winget itself truncated with an ellipsis — are dropped inside the
  parser, and when the (localized) header words are not English the
  parser falls back to the separator line's dash geometry, so
  non-English Windows still parses. The shell re-validates every ID
  against a fresh listing before running anything (below).
- **Relocation validation (`relocate`)** — the pure half of folder
  moves: `validate_relocation` rejects a source with no file name,
  anything on the shared protected/report-only lists, a destination
  parent at or inside the source (compared on normalized,
  case-folded spellings — the identity check included, so a
  case-variant spelling of the source's own parent cannot name the
  source as its own destination), and a destination without the
  source's bytes plus headroom (`max(64 MiB, 5%)`, overflow-checked).
  Volume comparison and filesystem existence are the shell's half.
- **Maintenance policy (`maintenance`)** — pure scheduling and
  selection for automatic maintenance: `maintenance_due` (daily ≥
  20 h, weekly ≥ 6.5 days, saturating arithmetic so a future
  last-run is never due) and `select_bin_purge`, which selects
  Recycle Bin items strictly older than the age cutoff (an item
  exactly at the cutoff is kept; the age is floored at one day).
  The shell runner adds the execution discipline (below).
- **Export (`export`)** — CSV renderings of a scan (largest files;
  category totals), RFC-4180 quoting with two hardening rules: both
  outputs begin with a UTF-8 byte-order mark so Excel detects the
  encoding, and any field starting with `=`, `+`, `-`, `@`, a tab
  or a carriage return is prefixed with a single quote, so a crafted
  file name on disk cannot become a spreadsheet formula in the
  user's own export.

## Assistant (`assistant`)

Optional and off by default. The core defines a fixed, read-only tool
set over already-computed scan data (category totals, top items,
recommendations, Docker overview, dev-tools summaries) and a provider
client (Anthropic / OpenAI / OpenRouter) that talks directly to the
provider's host over HTTPS with redirects disabled. API keys are kept
behind a keyring abstraction backed by Windows Credential Manager; keys
never enter settings files, logs or the frontend (the shell can store,
check presence of, and delete a key — it can never read one back out
through a command). The assistant has no tools that act: its only
effect on the UI is returning recommendation IDs to highlight.

## History (`history`)

A rusqlite (bundled SQLite) database at
`%APPDATA%\dev.diskpilot.windows\diskpilot.sqlite3`. Every executor
action and every scan appends an entry: timestamp, operation, paths,
bytes reclaimed, errors. History is append-only from the UI's point of
view — it is the audit trail for "what did DiskPilot ever do?".

Each removed item is also recorded with its mode — Trash or
permanent — which powers cleanup undo: when entries are listed, the
shell computes `restorableCount` by matching a run's Trash-mode items
against the live Recycle Bin listing and checking each original path
is currently free, and `restore_cleanup_run` restores the whole run
through the same single-item restore the Recycle Bin page uses,
reports per-item failures, counts permanent items as skipped, and
records the undo as its own history entry. Runs recorded before item
modes existed carry no list: they report zero restorable and decline
the group restore with an explanation.

## OCR text layer (`ocr`)

After a scan completes, the shell runs a background pass over the
fresh index's images and screenshots (when `ocrEnabled`): the core's
`ocr` module recognizes text on-device with Windows' built-in OCR
(WinRT; off Windows it returns a typed Unsupported error, never
synthetic text), capped at 400 files, newest first, two images in
flight at most, stopped cooperatively when a new scan bumps the
generation counter. Results are cached in the app-data
`ocr_cache.json` keyed by path + mtime + size and written into the
index entries' `ocrText`, which the core search matches terms
against alongside file names — a hit that matched only inside the
picture's text is flagged `matchedInText`.

## Live index (`usn`, `usn_volume`, `live_apply`, shell `live`)

Between scans, the shell keeps the in-memory index fresh by
following the NTFS USN change journal of the scanned volume — the
same "what changed since X" stream Windows itself maintains.

- **Checkpoints.** After every completed scan, the shell queries
  the journal (`FSCTL_QUERY_USN_JOURNAL`) and stores
  `{journalId, nextUsn}` per volume root in the app-data
  `usn_checkpoints.json`. The checkpoint is taken at scan end, so
  it matches the index's freshness. Loading or saving the file is
  best-effort, exactly like the scan history's.
- **Polling.** A thread wakes ~15 s after startup and then every
  60 s. For the current scan root's volume it reads the changes
  since the checkpoint (`FSCTL_READ_USN_JOURNAL`, reason-masked,
  capped at 200k records), resolving each record's full path by
  walking its parent chain through the MFT (`mft_volume`'s record
  reader). Only changes under the scan root are applicable.
- **Sound application.** Changes go through the core's
  `live_apply`: deletes and rename-sources remove the entry;
  creates, modifies and rename-targets re-stat the file through
  the walker's own metadata path and re-classify it against its
  parent directory's stored classification; ancestor totals are
  adjusted by the exact byte delta; the summary is then rebuilt
  from the mutated index through the same `finish_summary` a real
  scan uses (with the original scan's provenance carried over).
  Case-insensitive lookups bridge the journal's volume-anchored
  spelling and the index's scan-root spelling.
- **Stale, never approximate.** Anything that cannot be applied
  exactly marks the index stale instead: a journal reset or wrap
  (typed `JournalReset`), a batch of more than 20,000 applicable
  changes, an unresolvable path, any directory change (a
  directory's journal record says nothing about its subtree), a
  hard-link ambiguity, or a file appearing in a directory the
  index does not know. A stale index heals only through a fresh
  scan — triggered automatically through the scheduled-scan path
  when the Watch scheduler is enabled, otherwise left for the
  user's next scan. The served summary reports both states
  (`liveIndexActive`, `indexStale`); the persisted snapshot never
  carries them.
- **Limits.** The live index updates only while DiskPilot is
  running (tray-resident counts); changes made while it is closed
  are reconciled by the next scan. It requires the same
  administrator volume access as the MFT fast path — without it,
  the journal query fails, no checkpoint is kept, and polling
  quietly stays off. Freshness is within about a minute (the poll
  interval), never real-time. The `liveIndex` setting (default on)
  disables polling and checkpointing entirely.

## Shell (`src-tauri`)

- **Commands (`commands.rs`)** — one typed Tauri command per frontend
  API call. Slow commands run on blocking worker threads
  (`spawn_blocking`); scan lifecycle events stream to the frontend as
  `scan://progress`, `scan://partial` and `scan://finished`. Cleanup
  and Recycle Bin operations stream their own progress events.
  `reveal_in_explorer` first validates that the target is the scanned
  root or inside it, then spawns `explorer.exe /select,` with the path
  as a separate argument.
- **State** — the shell holds the current scan summary, derived
  recommendations, the single pending plan, cancellation handles, the
  lazily-opened History database and the settings struct, persisted to
  `settings.json` in the app data folder.
- **Tray (`tray.rs`, `tray_icon.rs`)** — the tray icon is rendered in
  code: an RGBA ring rasterised from the used fraction of the system
  volume, with a tooltip showing available space and a menu that emits
  `tray://navigate` events. Closing the window hides it to the tray
  instead of quitting. Once a scan exists, the menu is headed by a
  disabled digest line — "{reclaimable} can be cleaned", summed from
  the last scan's recoverable category totals — refreshed whenever
  the menu is rebuilt after a scan.
- **Recycle Bin (`recycle_bin.rs`)** — `$Recycle.Bin` inventory from
  the on-disk `$I…` metadata: listing, item-by-item emptying,
  restoring one item to its recorded original path (refused when the
  destination exists again). "Empty bin" is also per-item: every ID
  is re-validated against the machine's real `$Recycle.Bin` roots
  and exactly the listed payload + metadata pairs are removed one
  by one — there is no whole-bin API call, so items that arrive
  after the listing are never touched. Report export is a command too: the
  frontend's HTML is written through the dialog plugin's Rust-side
  save dialog, so it needs no capability permission.
- **Scan history (`scan_history.rs`)** — every completed scan
  (manual or scheduled) appends one entry to `scan_history.json` in
  the app data folder: time, root, totals, drive free space and
  per-category bytes, capped at the last 120 scans. It feeds the
  history chart, the `forecast` fit and the growth alert. Writes are
  best-effort and never fail a scan.
- **Live index (`live.rs`)** — the USN journal poller described in
  the Live index section above: per-volume checkpoints, a 60-second
  poll, sound application into the in-memory index, the stale flag,
  and the Watch-gated heal rescan.
- **Watch (`watch.rs`)** — the daily-driver automation. A background
  scheduler checks every 30 minutes (and once a minute after
  startup): when the settings' `autoScan` is `daily` or `weekly` and
  the last scan of the configured root is older than the interval
  (20 h / 6.5 days), it runs a scan through the same routine as the
  manual command. After a *scheduled* scan only, it can raise two
  notifications — free space below `lowSpaceAlertGb`, or growth above
  `growthAlertGb` since the previous scan. The module also owns
  launch-at-login (the per-user Run key on Windows, re-applied at
  startup so it self-heals) and the Quick Search popup window,
  toggled globally by Ctrl+Shift+Space and hidden on focus loss.
- **Drive health IO (`smart_io.rs`)** — Windows-only device reads
  behind `get_drive_health`: physical drives are probed in order,
  the storage descriptor gives model and bus type (the serial number
  is never read), capacity and failure prediction come from their
  IOCTLs, and the health bytes come from an ATA pass-through
  (SMART READ DATA, with the mandatory signature cylinders) or, for
  NVMe, an `IOCTL_STORAGE_PROTOCOL_COMMAND` Get Log Page request
  built by a pure, unit-tested constructor whose field offsets match
  the platform header exactly (84-byte header, command at offset 80,
  adapter-request flag `0x80000000`); a response counts as read only
  when the driver's status field reports success (`1`). Handles are
  opened read/write with a zero-access fallback, every per-drive
  failure degrades that drive to Unknown vitals or skips it, and the
  raw bytes are interpreted only by the core's `smart` parsers. Off
  Windows the command returns an empty list.
- **Software updates IO (`winget_io.rs`)** — locates `winget` (PATH,
  then the WindowsApps alias), runs it with an argument vector and
  no window, captures output with timeouts (120 s to list, 600 s
  per update), and parses it with the core's `winget` parser.
  `update_software` validates every requested ID twice —
  `is_valid_package_id` *and* membership in a fresh listing made
  inside the same call — and refuses non-members without running
  anything; an empty ID list means "everything in that fresh list",
  processed through the same per-ID loop (never a shell-out to
  `winget upgrade --all`). Outcomes carry the process's real exit
  status and output tail per package, and the run is recorded in
  History.
- **Relocation IO (`relocate_io.rs`)** — the executing half of
  folder moves. Order is the safety property: judge link-ness on
  the user's spelling, then canonicalize for the protection checks
  and the core's `validate_relocation`; measure the source (files
  only, links skipped); require a different destination volume;
  copy with `robocopy` (`/XJ`, so junctions inside the tree are
  skipped exactly as the measuring walk skips them); verify the
  copy's file count *and* allocated bytes against the source; only
  then delete the original with the link-safe delete and create the
  junction (`FSCTL_SET_REPARSE_POINT`, mount-point tag, `\??\`
  substitute name). A junction failure rolls back by copying back —
  and the rollback is verified to the same count-and-bytes standard
  before the working copy is removed; a partial delete of the
  original records a `relocate-failed` History entry naming both
  paths. Undo re-validates the History entry, confirms the source
  is still a junction pointing at the recorded target, and reverses
  the same way; an interrupted undo (source already a partial real
  directory, target intact) can be resumed under the same entry's
  authorization. A move whose History record fails still completes,
  with a `warning` on the result saying Undo is unavailable for it.
- **Maintenance runner (`maintenance.rs`)** — `run_if_due` is hooked
  into the Watch scheduler's loop and a startup catch-up; it owns
  no timer of its own and can never propagate an error into the
  scheduler. When the core policy says a run is due it (a) purges
  Recycle Bin items the policy selected — re-reading each item's
  `$I` record at delete time and skipping the item entirely if the
  record is gone, unparseable, or no longer past the cutoff — and
  (b) clears old temporary files *only* through the planner:
  recommendations are filtered to the Safe temp classification,
  built into a plan, taken with `take_plan_for_mode(Permanent)`,
  and executed by the executor; a gate refusal is a silent skip.
  There is no direct delete call for temp files. Every run —
  partial failures included — is recorded in History and stamps
  `last_maintenance_ms`, so a broken item retries at most once per
  window.
- **Startup manager (`startup.rs`)** — enumerates the per-user and
  machine-wide `Run` / `RunOnce` values and the Startup folders
  (including entries DiskPilot itself disabled, shown as disabled).
  Toggling re-enumerates and matches by opaque ID — the UI's paths
  are never trusted — and moves the *raw* registry value (type and
  data preserved, with rollback) to a `…-DisabledByDiskPilot`
  sibling key, or renames the folder item into a `Disabled by
  DiskPilot` subfolder without overwriting. Machine-wide changes
  fail with a plain "run as administrator" message. Changes are
  recorded in History with empty paths (real startup paths would
  corrupt the History restore accounting).
- **Explorer integration & launch intent (`commands.rs`, `lib.rs`)**
  — an optional per-user registration under
  `HKCU\Software\Classes\{Directory,Drive}\shell\DiskPilot` whose
  command is `"<exe>" --scan "%1"`; disabling deletes both keys.
  The shell parses `--scan` from argv at startup into a take-once
  `launch_intent` slot the frontend claims via `get_launch_intent`;
  the single-instance plugin (registered first) forwards a second
  launch's path into the same slot, emits `launch-scan`, and focuses
  the main window. Received paths are normalized before use — a
  drive root arrives mangled by the trailing-backslash quote escape
  (`C:"`) and is repaired to `C:\`. The frontend dedupes per
  delivery, not per path, so re-analyzing the same folder an hour
  later scans again.
- **Updater (`lib.rs`)** — the Tauri updater plugin with signed
  artifacts: releases publish a `latest.json` (see
  [RELEASING.md](RELEASING.md)), the app checks once per launch and
  on demand from Settings, and a package that fails signature
  verification is refused, never installed. The endpoint is baked
  into `tauri.conf.json` and assumes the repository is named
  `diskpilot`.

## Frontend state & the sidebar (`src/`)

The Zustand store is the single source of UI truth; pages hold no
scan state of their own, and load-once surfaces refetch when a scan
finishes. The sidebar is state-driven in three ways worth recording:

- **Drive-health dot.** The footer shows the worst SMART status
  across physical drives, fed by a single `get_drive_health` read
  per launch (silent on failure — an unreadable system simply shows
  no dot). Statuses map to the tier colours; the tooltip carries the
  per-drive summary.
- **Badges are information, never alarms.** Cleanup's pill shows the
  last scan's actionable (reclaimable) bytes — the Cleanup page's
  own headline figure; Applications' pill shows the update count
  from the last *completed* winget check, persisted (store +
  versioned localStorage) so it survives restarts. Nothing ever
  checks in the background to feed a badge.
- **Collapsed mode.** The sidebar collapses from 220 px to a 64 px
  icon rail (accessible names and tooltips retained; badges become
  icon dots) via `settings.sidebarCollapsed`. The shell settings
  struct predates the field, so the preference is mirrored to
  localStorage and the mirror wins at startup. The Quick Search
  window is a separate root with no sidebar and is unaffected.

## Data flow, end to end

```text
User clicks Scan
  └─ frontend invoke("start_scan")
       └─ shell: scanner::scan ── progress cb ──▶ emit scan://progress
                                └─ partial summaries ─▶ emit scan://partial
       └─ classifier runs inside the scan; summary stored + snapshotted
       └─ recommend::recommendations(summary) stored in shell state
       └─ emit scan://finished ─▶ Overview / Cleanup render from state
User selects recommendations, clicks Review cleanup
  └─ invoke("build_cleanup_plan", ids) ─▶ planner + safety gate
       └─ plan (paths, warnings, rejected) rendered for confirmation
User confirms
  └─ invoke("execute_cleanup_plan", planId) ─▶ executor re-validates,
       moves to Recycle Bin, records History, streams progress
```

## Testing

The core is tested on real temporary folder trees: scanner totals equal
category totals, hard-link dedup, symlinks not followed, container vs.
leaf classifier rules, safety-gate rejections (protected, outside root,
link in path), plan expiry and single-use, duplicate hashing, parsing
of a captured `docker system df -v` fixture, and secret masking. Core
tests run on Linux in CI as well as Windows, so the engine's logic is
platform-checked even where Windows APIs are platform-gated behind `cfg`.
