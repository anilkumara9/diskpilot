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

### Planner & safety gate (`cleanup`, `safety`)

The frontend sends recommendation **IDs only**. The planner resolves
IDs against the engine's own recommendation set, so the UI cannot
inject paths. The safety gate then validates every resolved path:

- absolute, with no `.` / `..` components or NUL bytes;
- inside the user's home folder **and** inside the scanned root;
- outside the protected list (see [WINDOWS.md](WINDOWS.md));
- no symlink, junction or reparse point in the path or any parent.

The resulting `CleanupPlan` lists exact paths and bytes, warnings, and
any rejected items (with reasons). It records whether permanent
deletion is allowed — only when *every* item is regenerable. Plans are
single-use and expire 10 minutes after creation.

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
- **Snapshot (`snapshot`)** — the last scan summary, persisted as JSON
  in the app data folder so the Overview is populated on launch before
  a fresh scan.

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
  instead of quitting.

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
platform-checked even where Windows APIs are stubbed behind `cfg`.
