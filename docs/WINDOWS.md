# Windows platform notes

How DiskPilot measures, what it refuses to touch, and how it ships on
Windows 10 and Windows 11.

## How the scanner measures

Windows does not have one "size". Explorer's default column is logical
length; what you actually get back when you delete something is its
allocated size. DiskPilot consistently measures allocated size, and
counts each physical file once.

| Question | DiskPilot's rule |
| --- | --- |
| File size | Allocated size via `GetCompressedFileSizeW` — correct for compressed, sparse and deduplicated files |
| Hard links | Counted once per volume, keyed by volume serial + file index |
| Symlinks, junctions, reparse points | Never followed; the link itself is metadata only |
| Volume boundaries | The walk never crosses to another volume (volume serial checked) |
| OneDrive placeholders | Files with `RECALL_ON_DATA_ACCESS` are counted at their local size and **never opened** — opening one would download it |
| Long paths | Extended-length `\\?\` paths throughout, so trees past 260 characters scan fully |
| Case | Rule matching is case-insensitive, matching NTFS semantics |
| Unreadable folders | Recorded and listed in Scan details ("Private folders"), never silently skipped |
| Cancellation | Cooperative: the walker checks a shared flag at every progress tick (~120 ms) |

## Protected locations

The safety gate rejects these unconditionally. Some are never even
*recommended*; Windows-owned servicing locations are reported and
pointed at the Windows tool that owns them.

| Location | Policy |
| --- | --- |
| `C:\Windows` (incl. WinSxS) | Never removed |
| `C:\Program Files`, `C:\Program Files (x86)` | Never removed |
| `C:\ProgramData` | Never removed |
| `C:\Recovery` | Never removed |
| `C:\System Volume Information` | Never removed |
| Drive roots (e.g. `C:\`) | Never removed |
| Profile top-level folders themselves (e.g. `C:\Users\<you>\Documents`) | Never removed (their *contents* are analysed normally) |
| `hiberfil.sys`, `pagefile.sys`, `swapfile.sys` | Never removed |
| `Windows.old` | Report only — use Disk Cleanup, Storage Sense or DISM |
| `C:\$Recycle.Bin` contents | Removable **only** through the Empty Recycle Bin flow |

Beyond the list, the gate requires every removal target to be absolute,
inside your home folder and the scanned root, and free of links or
reparse points in itself and in every parent — checked when a plan is
built and again immediately before execution.

## Recycle Bin behaviour

Removal defaults to the Recycle Bin via the Windows shell APIs, so
anything DiskPilot cleans is recoverable from the Recycle Bin until you
empty it. The Empty Recycle Bin dialog is part of the safety story: it
lists the current contents first, then removes exactly those items one
by one, showing live progress, a Stop button and a per-item result.
Items that arrive in the Bin *after* the listing are not touched by
that operation.

## Docker on Windows

Docker Desktop on Windows stores everything in a WSL2 virtual disk:

```text
%LOCALAPPDATA%\Docker\wsl\…\ext4.vhdx
```

Two consequences:

1. **The shrink gap.** Deleting images inside Docker frees space
   *inside* the vhdx; the vhdx file on the host often stays the same
   size. DiskPilot's Docker page shows both numbers — Docker's
   reclaimable figure and the vhdx's actual host size — so the gap is
   visible instead of surprising.
2. **Compaction is manual, by design.** DiskPilot shows the steps and
   never runs them:

   1. Quit Docker Desktop.
   2. From an elevated terminal: `wsl --shutdown`
   3. Compact the disk, either with Hyper-V cmdlets:
      `Optimize-VHD -Path <path to ext4.vhdx> -Mode Full`
      or with `diskpart`:

      ```text
      select vdisk file="<path to ext4.vhdx>"
      attach vdisk readonly
      compact vdisk
      detach vdisk
      ```

   Restart Docker Desktop afterwards and re-check the Docker page.

DiskPilot finds the `docker` CLI in its known install locations
(e.g. `C:\Program Files\Docker\Docker\resources\bin`) because GUI apps
do not inherit the PATH of your shell.

## Distribution

- **Installers.** MSI (per-machine, for managed deployments) and NSIS
  (per-user, no admin rights) are produced by the Tauri bundler from
  the same build.
- **WebView2.** DiskPilot renders with the WebView2 runtime, which is
  preinstalled on Windows 11 and current Windows 10 builds; the
  installer falls back to the WebView2 bootstrapper when it is missing.
- **SmartScreen.** Unsigned builds trigger a SmartScreen prompt on
  first run ("Windows protected your PC → More info → Run anyway").
  That is expected for new open-source builds; verify the SHA-256
  checksum published with the release, or build from source.
- **Signing.** Signing releases with an Authenticode certificate
  (ideally EV, for immediate SmartScreen reputation) is on the roadmap;
  the CI pipeline is structured so signing can be added as a bundler
  step without changing the app.
- **Updates.** There is no background updater phoning home — consistent
  with the no-telemetry privacy model. New versions are installed by
  downloading the new installer; settings and history in
  `%APPDATA%\dev.diskpilot.windows` carry over.

## Where DiskPilot keeps its own data

```text
%APPDATA%\dev.diskpilot.windows\
  settings.json        preferences (scan root, theme, assistant)
  diskpilot.sqlite3    History (SQLite)
  snapshot.json        last scan summary, restored on launch
```

Assistant API keys are **not** in this folder — they are stored in
Windows Credential Manager under the provider's name.
