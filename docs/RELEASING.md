# Releasing DiskPilot

The maintainer runbook: from version bump to a published release
that installed copies can update themselves into. Releases are
built by GitHub Actions (`.github/workflows/release.yml`) into a
**draft** — a human always reviews and publishes.

## The two installers, and why both exist

- **NSIS (`.exe`) — the consumer download.** Configured with
  `installMode: "currentUser"` (Tauri's per-user mode; its schema
  has no `perUser` value — `currentUser` *is* the per-user mode):
  installs into the user profile, **no administrator rights** for
  install or normal use. It is also the installer the in-app
  updater applies.
- **MSI (`.msi`) — the managed alternative.** WiX per-machine
  install into Program Files; **needs administrator rights**.
  Same app, same features.

Running as administrator is never required. Two engines — the fast
MFT scan and the live USN index — read raw NTFS structures and
engage only when the app itself runs elevated; both fall back to
the standard paths invisibly when it doesn't.

## One-time setup

1. **The repository name matters.** The updater endpoint baked into
   shipped apps is
   `https://github.com/anilkumara9/diskpilot/releases/latest/download/latest.json`.
   If the public repo is created under a different name, change that
   one line in `src-tauri/tauri.conf.json` (`plugins.updater.endpoints`)
   **before the first release ships** — apps already installed will
   keep polling the old address.
2. **Set the updater signing secret.** Repo *Settings → Secrets and
   variables → Actions → New repository secret*:
   - Name: `TAURI_SIGNING_PRIVATE_KEY`
   - Value: the key line from the private key file held by the
     maintainer (the single base64 line — not the comment header).
   The key has no password, so `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`
   is not needed (the workflow passes it through if ever set).
   **Custody:** if this key is lost, every installed copy rejects
   all future signed updates and the updater is bricked — keep an
   offline backup. It is *not* a code-signing certificate; the two
   signatures are unrelated.
3. **Enable private vulnerability reporting** (repo *Settings →
   Security*) so SECURITY.md's reporting route works.

## Cutting a release

1. **Bump the version — it must agree in all four places:**
   - `src-tauri/tauri.conf.json` (`"version"`)
   - `src-tauri/Cargo.toml`
   - `crates/core/Cargo.toml`
   - `package.json`
   Then run `cargo check --workspace` once so `Cargo.lock` picks up
   the new workspace versions, and commit everything together.
2. **CHANGELOG.md:** rename the top `## [X.Y.Z] — Unreleased`
   heading to `## [X.Y.Z] — YYYY-MM-DD` and open a fresh
   `## [Unreleased]` above it. The release notes users see in the
   update dialog come from the GitHub release body, which points
   here — keep the changelog entry user-readable.
3. **Tag and push:**
   ```powershell
   git tag vX.Y.Z
   git push origin main vX.Y.Z
   ```
4. **Watch the Release workflow.** It builds MSI + NSIS, signs the
   updater artifacts, generates `latest.json`, attaches
   `SHA256SUMS.txt`, and opens a **draft** release named
   *DiskPilot vX.Y.Z*.
5. **Review the draft:** asset list complete (MSI, NSIS `.exe`,
   their `.sig` files, `latest.json`, `SHA256SUMS.txt`), notes
   read well, version right. Then **Publish release**.
6. **Publishing is what turns updates on.** `latest.json` is served
   from `releases/latest`, which only ever points at the newest
   published (non-draft, non-prerelease) release — drafts are
   invisible to installed apps. After the first publish, verify
   from any machine:
   `https://github.com/anilkumara9/diskpilot/releases/latest/download/latest.json`
   resolves, and an installed older build flags the update in
   Settings → About & updates.

## Known gap: SmartScreen

Installers are not code-signed yet, so Windows SmartScreen warns on
first run (*More info → Run anyway*); the README and the release
notes say so plainly, and `SHA256SUMS.txt` covers verification in
the meantime. The fix is an OV (or EV) code-signing certificate:
wire it into `tauri.conf.json` (`bundle.windows.certificateThumbprint`
/ signing settings) and the release workflow when one exists. The
updater keeps working either way — its minisign signatures are
independent of Authenticode.
