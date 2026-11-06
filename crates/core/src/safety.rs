//! Safety gate. Every destructive path is validated here, at plan time and
//! again immediately before execution: absolute, no traversal components,
//! inside the scanned root or the user home, outside the protected list, and
//! free of symlinks / reparse points in the path and every parent.

use crate::error::{CoreError, CoreResult};
use std::path::{Component, Path, PathBuf};

pub fn protected_prefixes() -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = Vec::new();
    // Windows protected locations (checked case-insensitively on Windows,
    // harmless exact-prefix checks elsewhere).
    for p in [
        r"C:\Windows",
        r"C:\Program Files",
        r"C:\Program Files (x86)",
        r"C:\ProgramData",
        r"C:\Recovery",
        r"C:\System Volume Information",
        r"C:\$Recycle.Bin",
    ] {
        v.push(PathBuf::from(p));
    }
    if let Some(sys) = std::env::var_os("SystemRoot") {
        v.push(PathBuf::from(sys));
    }
    // Unix equivalents so the gate is meaningful in tests and on other hosts.
    for p in [
        "/etc", "/usr", "/bin", "/sbin", "/boot", "/sys", "/proc", "/dev",
    ] {
        v.push(PathBuf::from(p));
    }
    v
}

pub fn protected_filenames() -> &'static [&'static str] {
    &["hiberfil.sys", "pagefile.sys", "swapfile.sys"]
}

/// Report-only locations: never removed by the engine; the UI points to the
/// operating system's own tools instead.
pub fn is_report_only(path: &Path) -> bool {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    if name == "windows.old" {
        return true;
    }
    let norm = path.to_string_lossy().replace('\\', "/").to_lowercase();
    norm.ends_with("/windows.old") || norm.contains("/windows.old/")
}

fn has_traversal(path: &Path) -> bool {
    path.components().any(|c| {
        matches!(c, Component::ParentDir)
            || matches!(c, Component::CurDir if path.to_string_lossy().contains(".."))
    }) || path
        .to_string_lossy()
        .split(['/', '\\'])
        .any(|seg| seg == "..")
}

fn contains_nul(path: &Path) -> bool {
    path.to_string_lossy().contains('\0')
}

fn is_drive_or_filesystem_root(path: &Path) -> bool {
    path.parent().is_none()
}

fn starts_with_ci(path: &Path, prefix: &Path) -> bool {
    #[cfg(windows)]
    {
        let a = path.to_string_lossy().to_lowercase();
        let b = prefix.to_string_lossy().to_lowercase();
        a == b || a.starts_with(&format!("{}\\", b.trim_end_matches('\\')))
    }
    #[cfg(not(windows))]
    {
        path == prefix || path.starts_with(prefix)
    }
}

fn any_parent_is_link(path: &Path) -> Option<PathBuf> {
    let mut cur = Some(path);
    // Check the path itself and each existing ancestor.
    while let Some(p) = cur {
        if let Ok(md) = std::fs::symlink_metadata(p) {
            if md.file_type().is_symlink() {
                return Some(p.to_path_buf());
            }
            #[cfg(windows)]
            {
                use std::os::windows::fs::MetadataExt;
                const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0400;
                if md.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
                    return Some(p.to_path_buf());
                }
            }
        }
        cur = p.parent();
        // Stop climbing once ancestors no longer exist; lexical checks above
        // already guard the rest.
        if let Some(c) = cur {
            if !c.exists() && c.parent().map(|pp| !pp.exists()).unwrap_or(false) {
                break;
            }
        }
    }
    None
}

/// Profile top-level folders themselves must never be removed (their contents
/// may be, when a specific recommendation targets them).
fn is_profile_top_level(path: &Path, home: &Path) -> bool {
    path.parent().map(|p| p == home).unwrap_or(false) && path != home && {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        matches!(
            name.as_str(),
            "documents" | "downloads" | "pictures" | "music" | "videos" | "desktop" | "appdata"
        )
    }
}

#[derive(Debug, Clone)]
pub struct SafetyContext {
    pub scan_root: PathBuf,
    pub home: PathBuf,
}

impl SafetyContext {
    pub fn new(scan_root: impl AsRef<Path>) -> Self {
        Self {
            scan_root: scan_root.as_ref().to_path_buf(),
            home: dirs::home_dir().unwrap_or_else(|| PathBuf::from("/")),
        }
    }
    pub fn with_home(mut self, home: impl AsRef<Path>) -> Self {
        self.home = home.as_ref().to_path_buf();
        self
    }
}

/// Validate a path for a destructive operation. Returns the path unchanged on
/// success; every rejection carries a human-readable reason.
pub fn validate(path: &Path, ctx: &SafetyContext) -> CoreResult<PathBuf> {
    if contains_nul(path) {
        return Err(CoreError::SafetyRejected(format!(
            "path contains a NUL byte: {}",
            path.display()
        )));
    }
    if !path.is_absolute() {
        return Err(CoreError::SafetyRejected(format!(
            "path is not absolute: {}",
            path.display()
        )));
    }
    if has_traversal(path) {
        return Err(CoreError::SafetyRejected(format!(
            "path contains a traversal component: {}",
            path.display()
        )));
    }
    if is_drive_or_filesystem_root(path) {
        return Err(CoreError::SafetyRejected(
            "refusing to act on a drive or filesystem root".to_string(),
        ));
    }
    if path == ctx.home || path == ctx.scan_root {
        return Err(CoreError::SafetyRejected(format!(
            "refusing to act on the profile or scan root itself: {}",
            path.display()
        )));
    }
    let name_lower = path
        .file_name()
        .map(|n| n.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    if protected_filenames().contains(&name_lower.as_str()) {
        return Err(CoreError::SafetyRejected(format!(
            "protected system file: {}",
            path.display()
        )));
    }
    if is_report_only(path) {
        return Err(CoreError::SafetyRejected(format!(
            "report-only location, use the operating system tools instead: {}",
            path.display()
        )));
    }
    for prefix in protected_prefixes() {
        if starts_with_ci(path, &prefix) {
            return Err(CoreError::SafetyRejected(format!(
                "path is inside a protected location: {}",
                path.display()
            )));
        }
    }
    let inside_root = starts_with_ci(path, &ctx.scan_root);
    let inside_home = starts_with_ci(path, &ctx.home);
    if !inside_root && !inside_home {
        return Err(CoreError::SafetyRejected(format!(
            "path is outside the scanned root and user profile: {}",
            path.display()
        )));
    }
    if is_profile_top_level(path, &ctx.home) {
        return Err(CoreError::SafetyRejected(format!(
            "profile top-level folder itself is never removed: {}",
            path.display()
        )));
    }
    if let Some(link) = any_parent_is_link(path) {
        return Err(CoreError::SafetyRejected(format!(
            "path or a parent is a link or reparse point: {}",
            link.display()
        )));
    }
    Ok(path.to_path_buf())
}

/// Authorisation is validation with intent: only the executor calls this, and
/// it re-runs the full check at the moment of action.
pub fn authorize_destructive(path: &Path, ctx: &SafetyContext) -> CoreResult<PathBuf> {
    validate(path, ctx)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx(root: &std::path::Path) -> SafetyContext {
        SafetyContext::new(root).with_home(root)
    }

    #[test]
    fn rejects_relative_and_traversal() {
        let dir = tempfile::tempdir().unwrap();
        let c = ctx(dir.path());
        assert!(validate(Path::new("relative/path"), &c).is_err());
        let trav = dir.path().join("a/../b");
        assert!(validate(&trav, &c).is_err());
    }

    #[test]
    fn rejects_outside() {
        let dir = tempfile::tempdir().unwrap();
        let c = ctx(dir.path());
        assert!(validate(Path::new("/etc/passwd"), &c).is_err());
    }

    #[test]
    fn rejects_windows_old_and_everything_under_it() {
        // Defense in depth for the guided-only Windows.old
        // recommendation: even a path that somehow reached the gate
        // is refused, the folder itself and any child alike.
        let dir = tempfile::tempdir().unwrap();
        let c = ctx(dir.path());
        let old = dir.path().join("Windows.old");
        assert!(validate(&old, &c).is_err());
        assert!(validate(&old.join("Windows/System32/kernel.dll"), &c).is_err());
        assert!(is_report_only(&old));
        assert!(is_report_only(&old.join("Users/meda/file.txt")));
    }
}
