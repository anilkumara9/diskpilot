# Contributing to DiskPilot

Thanks for helping. DiskPilot's bar is its safety model: contributions
that make analysis better are welcome, and anything that touches
deletion gets extra scrutiny by design.

## Getting set up

Requirements: Rust 1.82+ (rustup), Node.js 22, pnpm 10, and on Windows
the Visual Studio Build Tools (Desktop development with C++ workload).

```powershell
pnpm install
pnpm tauri dev      # full app
cargo test --workspace
```

## Ground rules

1. **Read-only by construction.** Scanners, classifiers and analyzers
   must not be able to modify the filesystem. If a change needs write
   access outside the executor, it is the wrong shape — open an issue
   and let's design it.
2. **Explain yourself.** New classification rules carry an ID,
   explanation and confidence. New developer-tool analyzers report
   items with name, path, size and state, and read other apps' data
   read-only.
3. **Never weaken the safety gate.** Protected paths, link/reparse
   checks, plan single-use and expiry, and re-validation before
   execution are not negotiable. PRs that relax them will be closed.
4. **No new webview powers.** The frontend talks to the core only
   through typed commands. Do not add fs/shell-style capabilities or
   plugins.
5. **Tests on real trees.** Core changes ship with integration tests
   over real temporary folder structures (symlinks, hard links,
   protected paths), not mocks of the filesystem.
6. **Keep the numbers honest.** Byte formatting is decimal
   (1 GB = 10^9), sizes are allocated sizes, and estimates are labelled
   as estimates.

## Checks before a pull request

```powershell
pnpm format:check
pnpm lint
pnpm typecheck
pnpm test
cargo fmt --check
cargo clippy --workspace -- -D warnings
cargo test --workspace
```

CI runs the same set on Windows, plus core tests on Linux.

## Pull requests

- One topic per PR; describe the user-visible change and the safety
  reasoning, if any.
- Update `CHANGELOG.md` under an *Unreleased* heading for notable
  changes.
- Screenshots for UI changes, in light and dark themes.

## Reporting bugs

Include your Windows version, DiskPilot version, the scan root, and —
for anything safety-adjacent — the exact paths involved (redact personal
folder names if you prefer, but keep the structure). For suspected
safety-gate issues, follow [SECURITY.md](SECURITY.md) instead of
opening a public issue.
