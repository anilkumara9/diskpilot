# Changelog

All notable changes to DiskPilot are documented here. The format is
based on Keep a Changelog, and the project follows semantic versioning.

## [0.6.0] — Unreleased

### Added

- **Scan history** — every completed scan (manual or scheduled) now
  appends to a persisted time series (`scan_history.json`, last 120
  scans): totals, file count, per-category bytes and the scanned
  drive's free space, which the scanner now records on every summary.
- **Space forecast** — a least-squares fit over the last 30 days of
  scan history reports the drive's daily growth and, while it is
  growing, the projected date the drive fills up.
- **Biggest changes** — a directory-level diff between the current
  and previous scans (grown / shrunk / added / removed), so "what
  ate my disk?" has a direct answer after every scan.
- **Scheduled scans with alerts** — optional daily or weekly
  background scans of the configured root; after a scheduled scan,
  DiskPilot notifies when free space drops below a threshold
  (default 10 GB) or usage grows past a threshold (default 5 GB).
  Manual scans never notify.
- **Launch at login** — an optional autostart setting backed by the
  per-user Run key, re-applied at startup so it self-heals.
- **Quick Search window** — a global Ctrl+Shift+Space shortcut
  toggles a compact, always-on-top search popup over the current
  scan index; it hides when it loses focus.
- **Drives** — every mounted drive is enumerated with its total and
  free space, so multi-drive PCs get a per-volume picture.
- **Previous Windows installation** — a `Windows.old` folder at the
  scan root is reported as a guided-only recommendation carrying the
  removal route through Windows' own storage tools; DiskPilot never
  deletes it (the planner rejects it and the safety gate treats the
  location as report-only).
- **Big files, long forgotten** — a fifth Clean Up card: any file of
  at least 500 MB untouched for over 180 days that no other card
  claims.
- **OneDrive awareness** — the local OneDrive folder is detected and
  its on-disk footprint read from the scan; selected files and
  folders can be marked online-only ("free up space"), asking
  OneDrive to evict the local copies without deleting anything.
- **Recycle Bin restore** — a recycled item can be restored to its
  original location (refused when something already sits there),
  alongside the existing item-by-item emptying.
- **Tray digest** — once a scan exists, the tray menu is headed by a
  "{reclaimable} can be cleaned" readout from the last scan.
- **Report export** — a standalone HTML storage report can be saved
  through the native save dialog.
- **Health page** — every drive judged by the room it has left
  (Healthy / Getting low / Low space), the fill-up forecast with its
  growth chart (scanned size and free space across the scan history),
  and the watch configuration at a glance.
- **Media summary** — photos, videos and screenshots are totalled
  from the scan index (allocated bytes and file counts, classified
  with the same kind rules as file search; screenshots are a subset
  of photos), with the 15 biggest photos and videos listed.
- **Games** — installed Steam and Epic Games libraries are read from
  the launchers' own manifest files (Steam `libraryfolders.vdf` +
  `appmanifest_*.acf` sizes, Epic `.item` manifests), reporting every
  game's on-disk size per library; games whose Steam manifest has no
  recorded size are measured directly.
- **Search reads your screenshots** — after a scan, an on-device OCR
  pass (Windows' built-in recognizer — nothing leaves the machine)
  reads the text inside images and screenshots, and file search matches
  query terms against that text too. A hit found only in a picture's
  text is flagged as such. Recognitions are cached per file (path +
  mtime + size), so unchanged images are never re-read; the pass runs
  in the background, at most two images at a time, and can be turned
  off in Settings.
- **Undo a cleanup from History** — every cleanup run now records where
  each item went (Recycle Bin or permanent). History shows how many of
  a run's items can still be brought back, and one action restores the
  whole run: items already gone from the Recycle Bin are reported,
  permanently deleted items are counted as skipped, and the restore
  itself is recorded in History. Runs recorded before this version
  explain that they predate restore support.

### Changed

- **Interface** — full design-system upgrade pass across every page:
  the Overview was recomposed around its new sections, the sidebar
  regrouped (Overview and Health on top; Analyze, System and Clean up
  groups), and the treemap, biggest-changes panel, forecast card and
  settings forms were brought to one finish.

## [0.5.1] — 2026-11-06 — Windows release

First Windows release of DiskPilot: storage intelligence for Windows 10
and Windows 11, packaged as MSI and NSIS installers.

### Added

- **Overview** — whole-disk storage strip with per-category segments
  and hatched recoverable space; Used / Available / Likely recoverable /
  Scanned figures; top recommendations, category breakdown, largest
  consumers and full scan details.
- **Scanner** — parallel (rayon) read-only engine: allocated-size
  measurement via `GetCompressedFileSizeW`, hard links counted once
  (volume serial + file index), symlinks/junctions/reparse points never
  followed, no volume crossings, OneDrive placeholders counted at local
  size and never opened, extended-length (`\\?\`) paths, ~120 ms
  progress callbacks, cooperative cancellation.
- **Classifier** — data-driven rule table over Windows locations with
  eleven categories; every classification carries rule ID, explanation,
  confidence and inheritance.
- **Storage explorer** — drill-down tree with classification cards and
  Reveal in Explorer; Largest items tab (500 largest files, heavy
  folders).
- **Applications** — installed apps with install + AppData (Local /
  Roaming) sizes; leftover data folders from uninstalled apps reported
  for review, never removed automatically.
- **Cleanup** — recommendations grouped Likely safe / Review first /
  Keep with full reasoning, risks and confidence; tool-native commands
  shown where a tool has one; single-use plans with 10-minute expiry;
  Recycle Bin by default, permanent delete only for all-regenerable
  plans; results recorded to History.
- **Safety gate** — protected-path policy for `C:\Windows`, Program
  Files, ProgramData, Recovery, System Volume Information, drive roots
  and paging/hibernation files; validation at plan time and again
  immediately before execution.
- **Developer Tools** — analyzers for Cursor, VS Code, Claude Code, the
  Claude app, Codex, GitHub Copilot, Gemini CLI, Cursor Agent CLI,
  Node.js (npm/pnpm/Yarn/Bun/nvm-windows), Rust, Python, .NET, Visual
  Studio, JetBrains, Ollama and LM Studio; offline transcript secrets
  check with masked findings.
- **Docker** — `docker system df -v` overview with image/container/
  volume/build-cache breakdown; closed, previewed action set (stopped
  container removal, unused image removal, build-cache prune); WSL2
  vhdx diagnostics with guided compaction steps.
- **Duplicates** — size-then-SHA-256 grouping, hard-link and NTFS clone
  awareness, keeper selection, byte-for-byte re-verification before any
  copy moves to the Recycle Bin.
- **Recycle Bin dialog** — lists contents first, removes exactly the
  listed items one by one with live progress and Stop.
- **History** — SQLite log of cleanup actions and scans.
- **Assistant** (optional, off by default) — read-only chat over scan
  data for Anthropic, OpenAI and OpenRouter; API keys in Windows
  Credential Manager; direct HTTPS to the chosen provider.
- **System tray** — code-rendered free-space ring icon, tooltip with
  available space, menu shortcuts to Cleanup, Duplicates and Empty
  Recycle Bin; closing the window hides to tray.
- **Settings** — scan root picker, System/Light/Dark theme, assistant
  configuration, Empty Recycle Bin entry point, protected-folder
  guidance.
