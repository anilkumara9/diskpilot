# Changelog

All notable changes to DiskPilot are documented here. The format is
based on Keep a Changelog, and the project follows semantic versioning.

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
