# Security policy

## The threat model in one paragraph

DiskPilot's entire value is that it can be trusted around a delete
button. The safety gate in the Rust core is the product's security
boundary: every path that reaches the executor has been validated, at
plan time and again immediately before execution, to be absolute,
inside the user's home and the scanned root, outside the protected
list, and free of symlinks, junctions and reparse points in itself and
every parent. **A way around the safety gate is a critical
vulnerability**, and so is any way for the webview to gain filesystem
or shell access beyond the typed Tauri commands.

Other properties we treat as security requirements:

- The scanner never follows links and never opens OneDrive placeholder
  files.
- Docker actions come from a closed set; volumes are never removed and
  `--force` is never passed to `rm`.
- Assistant API keys live in Windows Credential Manager, never in
  settings files or logs. Transcript secret findings are masked before
  they are displayed and never leave the machine.
- Duplicate copies are re-verified byte-for-byte immediately before a
  move; a mismatch aborts that copy's removal.

## Reporting a vulnerability

Please report vulnerabilities privately to the maintainers via the
repository's security advisory feature (GitHub: *Security → Report a
vulnerability*) rather than opening a public issue. Include:

- what you found and the version/build it affects,
- steps to reproduce — a temporary folder tree that demonstrates the
  issue is ideal,
- the impact as you see it (which safety property breaks).

We aim to acknowledge reports within a few days. There is no bug
bounty; there is sincere gratitude and a credit in the changelog if
you'd like one.

## Supported versions

Only the latest release is supported with security fixes.
