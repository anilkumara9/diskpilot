# DiskPilot for Windows

**Understand your storage. Know what is safe. Clean it confidently.**

DiskPilot is open-source storage intelligence for Windows. It scans your
drives read-only, explains — in plain language — what is using space and
why it is there, and only ever removes files through a reviewed cleanup
plan that passes a strict safety gate. Nothing is deleted silently, and
the default for everything is the Recycle Bin.

Built with Tauri 2 and Rust: a parallel scanning engine, a rule-based
classifier that explains every classification, dedicated analyzers for
developer tools and Docker, a duplicate finder that re-verifies files
byte-for-byte before touching them, and a full audit history of every
action ever taken.

## Why DiskPilot?

- **"Disk full" is a symptom, not an answer.** Other tools show you a
  treemap and leave you to guess what is safe to delete. DiskPilot
  classifies every folder into one of eleven categories, attaches the
  rule that matched, a confidence level, and an explanation — then turns
  that into recommendations grouped by risk: *Likely safe*, *Review
  first*, *Keep*.
- **Deletion is the last step of a pipeline, not a button.** A plan is
  built from recommendation IDs (the UI never sends paths), validated by
  a safety gate, shown to you in full, executed once, and recorded.
- **Developer machines are the target.** Node modules, Rust `target`
  folders, editor workspace storage, AI-assistant transcripts, Docker's
  WSL2 virtual disk — DiskPilot understands the storage profile of a
  modern development workstation, because that is where the gigabytes
  hide.
- **Private by construction.** No account, no telemetry, no analytics.
  Everything is computed and stored on your machine. The only optional
  network feature is the Assistant, and it talks directly to the AI
  provider you choose.

## Features

### Overview

The first screen answers the question directly: one horizontal strip
shows the whole disk — a segment per category, the rest of the used
space, and the free space — with the recoverable portion hatched inside
each category segment. Below it: Used / Available / Likely recoverable /
Scanned figures, the recommendations most worth reviewing, where space
goes by category, the largest consumers, and full scan details
(including folders that could not be measured).

### Storage explorer

Drill into the scanned tree with breadcrumbs, sorting, name and category
filters, and a details panel for any folder: live metadata plus its
classification card (category, the rule that matched, confidence, and
what it was inherited from) and a *Reveal in Explorer* button. A
**Largest items** tab lists the 500 largest files and the heaviest
folders — with the standing reminder that size alone is never a reason
to delete.

### Applications

Installed applications with their install size *and* the size of the
data they keep in AppData (Local and Roaming), sorted by total
footprint. A separate **Leftovers** section reports data folders left
behind by uninstalled apps. Leftovers are reported for review in
Explorer and are **never removed automatically** — they can contain
documents.

### Cleanup (recommendations)

Every recommendation is expandable: why it was detected, why it may be
safe, the risks, a confidence level, and the estimated reclaim. Where a
tool has its own hygiene command — `pnpm store prune`, `nvm uninstall`,
`cargo cache`, a Docker prune preview — DiskPilot shows you the command
instead of deleting files behind the tool's back.

Selecting items opens the cleanup plan: the exact paths, warnings, any
items the safety gate rejected, and the action — *Move to Recycle Bin*
(default) or *Permanently delete* (offered only when every selected
item is regenerable). Plans are single-use and expire after 10 minutes.
Every result is recorded in History.

### Developer Tools

Per-tool analyzers that read other apps' data **read-only** and report
each item with a state (current / stale / orphaned / superseded /
informational):

- **Cursor & VS Code** — per-project `workspaceStorage` (including
  projects that no longer exist), extensions vs. leftover folders,
  caches, logs, leftover update downloads, stale state-database backups,
  and the Cursor chat database size (read-only).
- **Claude Code** — versions, session transcripts per project (including
  deleted project folders), MCP/debug logs, shell snapshots, caches,
  edit checkpoints.
- **Claude app** — Chromium/WebGPU caches, app cache, updates, logs.
- **Codex** — CLI/app logs, cache, updates. Sessions and prompt history
  are listed but **never** recommended for removal.
- **GitHub Copilot** — logs and sessions, plus Copilot extension storage
  in VS Code/Cursor, including storage left after uninstall.
- **Other AI agents** — Gemini CLI data, Cursor Agent CLI versions.
- **Node.js** — `node_modules` per project with manifest last-change
  dates, npm cache, pnpm store, Yarn cache, Bun, nvm-windows versions.
- **Rust** — rustup toolchains and the default, `target` folders per
  project, Cargo caches.
- **Python** — `.venv`/`venv` per project, pip/uv/Poetry caches.
- **.NET** — `bin`/`obj` per project, the NuGet package cache.
- **Visual Studio / JetBrains** — component and IDE caches.
- **Local AI models** — Ollama and LM Studio model storage, reported by
  reference (models are shared between tools); unused Ollama layers and
  interrupted downloads are offered for review.
- **Transcript secrets check** — Claude Code, Codex and Gemini
  transcripts are scanned locally for known credential formats (API
  keys, tokens, private keys). Findings are shown masked; nothing is
  uploaded and nothing is changed.

Tier mapping is consistent everywhere: caches, logs and update
leftovers are *Likely safe*; checkpoints, stale backups and sessions
for deleted projects are *Review first*; history, sign-in data and
settings are never recommended.

### Docker

Shows Docker's state (not installed / not running / ready) and parses
`docker system df -v` into image, container, volume and build-cache
usage with reclaimable amounts. Actions are a closed set, each previewed
with the exact command and re-checked before it runs: remove a stopped
container (never `-f`, never `-v`), remove an unused image (never
`-f`), and prune unused build cache after listing it. Volumes are
listed and inspectable but **never removed**.

Diagnostics show the Docker Desktop WSL2 virtual disk
(`%LOCALAPPDATA%\Docker\wsl`) next to the space Docker reports as
reclaimable, and walk you through compaction (quit Docker,
`wsl --shutdown`, then `Optimize-VHD` or `diskpart compact vdisk`) —
the steps are shown, never run for you.

### Duplicates

Files are grouped by size first, then confirmed with SHA-256. Hidden
folders, AppData, dependency folders and package caches are skipped;
hard links are counted once, and NTFS clones/deduped files are flagged
(removing them frees little). You pick the keeper — one copy always
stays — and every other copy is re-verified byte-for-byte immediately
before it is moved to the Recycle Bin.

### Recycle Bin

Emptying is a reviewed operation too: the dialog lists the Recycle
Bin's contents first, then removes exactly those items one by one with
live progress, a Stop button, and a per-item result.

### History

Every cleanup action — timestamp, operation, paths, space reclaimed,
errors — plus scan history, in a local SQLite log. If DiskPilot did it,
it is in History.

### Assistant (optional, off by default)

An in-app chat over your scan data, with Anthropic, OpenAI or OpenRouter
(one API key, stored in Windows Credential Manager). The assistant has a
fixed, read-only tool set — categories, top items, recommendations,
Docker usage, developer-tool summaries. It cannot delete, move or run
anything; it can only highlight existing recommendations for your
review. Paths are shown with your home folder as `~`, private folders
are only ever totals, and file contents are never read or sent.
Requests go directly to the provider's host, HTTPS only, no redirects.

### System tray

DiskPilot keeps running in the tray when you close the window. The tray
icon is a ring that fills as your system drive fills, with a tooltip
showing available space, and a menu for Cleanup, Duplicates and Empty
Recycle Bin.

## Safety model

One direction, six stages — and only the last one can remove anything:

```text
 Scanner ──▶ Classifier ──▶ Recommendations ──▶ Planner ──▶ Safety gate ──▶ Executor
 (read)      (pure)         (explain only)      (preview)   (validate)      (act, record)
```

- **Scanner** — parallel, read-only. Never follows symlinks, junctions
  or reparse points; never crosses to another volume; counts hard links
  once; measures *allocated* size; never opens OneDrive placeholder
  files (they are counted at their local size).
- **Classifier** — a plain data table of rules, first match wins. Every
  classification carries its rule ID, explanation and confidence.
  Classification never uses size and never decides deletion.
- **Recommendations** — explain only: what, why, risks, confidence, and
  the tool's own command when one exists.
- **Planner** — the UI sends recommendation IDs, never paths. Plans are
  single-use and expire in 10 minutes.
- **Safety gate** — a path must be absolute, inside your home folder and
  the scanned root, outside the protected list, and free of links or
  reparse points in itself and every parent. It is validated when the
  plan is built **and again immediately before execution**.
- **Executor** — moves to the Recycle Bin via the operating system;
  permanent deletion only for plans where every item is regenerable.
  Every outcome is written to History.

Protected locations are never removed: `C:\Windows` (including WinSxS),
`C:\Program Files`, `C:\Program Files (x86)`, `C:\ProgramData`,
`C:\Recovery`, `C:\System Volume Information`, drive roots,
`hiberfil.sys` / `pagefile.sys` / `swapfile.sys`, and the top-level
folders of your profile themselves. Some things are only *reported*:
`Windows.old`, for example, is pointed at Disk Cleanup / Storage Sense /
DISM, where it belongs. Recycle Bin contents are removable only through
the Empty Recycle Bin flow.

The webview itself has no filesystem or shell access at all — the
frontend can only call DiskPilot's typed commands.

## Privacy

- No account, no sign-up, no telemetry, no analytics, no crash
  reporting. DiskPilot works fully offline.
- Scan data, settings and history live in
  `%APPDATA%\dev.diskpilot.windows` on your machine and nowhere else.
- The optional Assistant is the only network feature. It is off by
  default, uses your own API key (stored in Windows Credential
  Manager), connects directly to the provider you selected, and only
  ever receives storage metadata — never file contents.

## Installation

### Download

Grab the latest release installer:

- **MSI** — for managed/per-machine installs.
- **NSIS `.exe`** — per-user install, no admin rights needed.

Windows may show a SmartScreen prompt for new builds ("Windows
protected your PC"). DiskPilot is open source; you can verify the
checksum published with the release and inspect or build the exact code
yourself — see below. See [docs/WINDOWS.md](docs/WINDOWS.md) for
distribution and signing notes.

DiskPilot uses the WebView2 runtime, which ships with Windows 11 and
current Windows 10 builds; the installer downloads the bootstrapper if
it is missing.

### Build from source

Requirements:

- Rust 1.82 or newer (via [rustup](https://rustup.rs))
- Node.js 22 and [pnpm](https://pnpm.io) 10
- Visual Studio Build Tools (Desktop development with C++ workload)
- WebView2 runtime (preinstalled on Windows 11)

```powershell
git clone <this repository> diskpilot-windows
cd diskpilot-windows
pnpm install
pnpm tauri build        # installers land in src-tauri/target/release/bundle
```

## Using DiskPilot

1. **Scan.** The default scan root is your profile folder; change it in
   Settings. Scanning is read-only and cancellable.
2. **Read the Overview.** The strip, the recoverable figure, and the
   top recommendations tell you where you stand.
3. **Review Cleanup.** Expand anything you do not recognise. Confidence
   and risks are always visible before you select anything.
4. **Confirm the plan.** Exact paths, warnings, gate rejections — then
   Move to Recycle Bin (default) or, when offered, Permanently delete.
5. **Check History.** Everything that happened is logged, including how
   much space came back.

## Development

```powershell
pnpm install
pnpm dev            # frontend only (shows the "runs as a desktop app" state)
pnpm tauri dev      # full desktop app with hot reload
pnpm format:check   # prettier
pnpm lint           # eslint
pnpm typecheck      # tsc --noEmit
pnpm test           # vitest
cargo fmt --check
cargo clippy --workspace -- -D warnings
cargo test --workspace
```

The Rust workspace has two members: `crates/core` (the engine:
scanner, classifier, recommendations, safety gate, cleanup, duplicates,
Docker, developer-tools analyzers, history, assistant) and `src-tauri`
(the desktop shell: window, tray, commands, packaging). The core crate
has no UI dependencies and ships a read-only `diskpilot-scan` binary
that scans a path and prints the summary as JSON — handy for debugging
and verification.

## Project structure

```text
crates/core/     Rust engine (the safety pipeline lives here)
src-tauri/       Tauri 2 shell: commands, tray, bundle config
src/             React 19 + TypeScript frontend
  lib/           types, api bridge, store, formatting, theme
  components/    layout, shared widgets, UI primitives
  features/      one folder per page
docs/            ARCHITECTURE.md, DESIGN.md, WINDOWS.md
```

## Architecture

DiskPilot is split so that the part that can delete files is small,
pure where possible, and heavily tested — and the part that renders UI
cannot touch the filesystem at all. See
[docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for the pipeline, data
flow and storage layout, [docs/DESIGN.md](docs/DESIGN.md) for the
design system, and [docs/WINDOWS.md](docs/WINDOWS.md) for platform
measurement details and protected-path policy.

## Roadmap

- **MFT fast mode** — an optional, opt-in accelerated scanner that
  reads the NTFS Master File Table directly for near-instant full-disk
  scans (requires elevation; the current portable scanner stays the
  default).
- **MCP server** — expose the read-only analysis (categories, top
  items, recommendations) to external AI tools over the Model Context
  Protocol, with the same privacy rules as the built-in Assistant.
- Scheduled drift reports ("what grew this week"), additional
  developer-tool analyzers, and signed releases.

## Known limitations

- **Docker vhdx shrink gap.** Docker can report gigabytes as
  reclaimable inside its WSL2 virtual disk while the `.vhdx` file on
  the host does not shrink. DiskPilot shows both numbers and the manual
  compaction steps; it does not compact the disk for you.
- **SmartScreen.** Until releases are code-signed, Windows SmartScreen
  may warn on first run. Verify checksums, or build from source.
- **Allocated vs. logical size.** Sparse, compressed and deduplicated
  files are measured at their allocated size, which is what you
  actually get back — but it can differ from the size Explorer shows by
  default.
- **OneDrive placeholders** are counted at their local (downloaded)
  size and never opened, so a files-on-demand folder may look smaller
  than its cloud size.
- **Applications data sizes** cover AppData Local/Roaming; data a
  program stores elsewhere is attributed by the scan, not the app list.

## Contributing

Contributions are welcome — see [CONTRIBUTING.md](CONTRIBUTING.md).
The bar is the safety model: new analyzers and rules must be read-only
by construction, explain themselves, and ship with tests on real
temporary folder trees.

## Security

See [SECURITY.md](SECURITY.md) for the threat model and how to report a
vulnerability. The short version: the safety gate is the product; a way
around it is a critical bug.

## License

MIT — see [LICENSE](LICENSE).
