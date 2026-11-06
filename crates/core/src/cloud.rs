//! OneDrive awareness: where the local OneDrive folder is, how much
//! of it is actually stored on this disk, and asking OneDrive to free
//! that space back up.
//!
//! "Free up space" is Files On-Demand dehydration, not deletion: the
//! shell command marks files unpinned (`attrib +U -P`), which asks
//! the OneDrive sync client to evict the local copy and keep the
//! cloud copy. The eviction itself completes when OneDrive next
//! syncs; the files stay listed in the folder as online-only
//! placeholders and re-download on open.

use crate::model::FileEntry;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// The only cloud provider DiskPilot understands today.
pub const PROVIDER_ONEDRIVE: &str = "OneDrive";

/// Locate the local OneDrive folder. OneDrive records it in the
/// `OneDrive` environment variable (which follows whichever account
/// synced most recently); the per-account `OneDriveConsumer` and
/// `OneDriveCommercial` variables are the fallbacks, in that order.
/// The first variable naming a directory that exists wins.
pub fn detect_onedrive_root() -> Option<PathBuf> {
    for var in ["OneDrive", "OneDriveConsumer", "OneDriveCommercial"] {
        if let Some(value) = std::env::var_os(var) {
            if value.is_empty() {
                continue;
            }
            let path = PathBuf::from(value);
            if path.is_dir() {
                return Some(path);
            }
        }
    }
    None
}

/// Is `path` the root itself or inside it? Pure and deliberately
/// separator- and case-insensitive on every platform: the paths being
/// judged are Windows paths (OneDrive only exists there), and Windows
/// path semantics are case-insensitive whatever host runs the check.
/// The root itself counts as within (a caller may target the folder).
///
/// Any `..` component in either string refuses the match outright:
/// `C:\OneDrive\..\..\Windows` textually starts with the root but
/// resolves outside it, and this check is the only thing standing
/// between a caller-supplied path and `attrib` running on it.
pub fn path_is_within(root: &str, path: &str) -> bool {
    let norm = |s: &str| {
        s.replace('\\', "/")
            .trim_end_matches('/')
            .to_lowercase()
    };
    let root = norm(root);
    let path = norm(path);
    if root.is_empty() || path.is_empty() {
        return false;
    }
    let has_parent_ref = |s: &str| s.split('/').any(|seg| seg == "..");
    if has_parent_ref(&root) || has_parent_ref(&path) {
        return false;
    }
    path == root || path.starts_with(&format!("{root}/"))
}

/// How much of the OneDrive folder is stored locally, computed from a
/// scan index: files under the root that occupy allocated bytes (an
/// online-only placeholder occupies none, so it is not "local").
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudStatus {
    /// Always `"OneDrive"` today.
    pub provider: String,
    pub root_path: String,
    /// Sum of allocated bytes of the files counted below.
    pub local_bytes: u64,
    /// Files under the root occupying allocated bytes.
    pub file_count: u64,
}

/// Local footprint of the OneDrive folder within a scan index.
pub fn cloud_status(root: &str, files: &[FileEntry]) -> CloudStatus {
    let mut local_bytes: u64 = 0;
    let mut file_count: u64 = 0;
    for file in files {
        if file.bytes > 0 && path_is_within(root, &file.path) {
            local_bytes += file.bytes;
            file_count += 1;
        }
    }
    CloudStatus {
        provider: PROVIDER_ONEDRIVE.to_string(),
        root_path: root.to_string(),
        local_bytes,
        file_count,
    }
}

/// One path `free_up_space` could not mark.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FreeUpFailure {
    pub path: String,
    pub error: String,
}

/// Per-path outcome of a `free_up_space` call.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FreeUpSpaceResult {
    pub succeeded: Vec<String>,
    pub failed: Vec<FreeUpFailure>,
}

/// Ask OneDrive to free up the local copies of `paths`.
///
/// Every path must be the detected OneDrive root or inside it; a
/// single outside path refuses the whole call, so the operation can
/// never be aimed elsewhere. Per-path failures are collected in the
/// result, not fatal. Nothing is deleted: the flag change requests
/// dehydration, which the OneDrive client completes on sync.
pub fn free_up_space(paths: &[String]) -> Result<FreeUpSpaceResult, String> {
    #[cfg(not(windows))]
    {
        let _ = paths;
        Err("Free up space for OneDrive is available on Windows only.".to_string())
    }
    #[cfg(windows)]
    {
        let Some(root) = detect_onedrive_root() else {
            return Err("No OneDrive folder was found on this PC.".to_string());
        };
        let root = root.to_string_lossy().into_owned();
        for path in paths {
            if !path_is_within(&root, path) {
                return Err(format!(
                    "Refusing to free up space outside the OneDrive folder: {path}"
                ));
            }
        }
        Ok(mark_unpinned(paths))
    }
}

/// Windows: set the unpinned attribute (`attrib +U -P`) on each path —
/// the documented way to request that OneDrive make content
/// online-only. Directories are recursed with `/S /D`.
#[cfg(windows)]
fn mark_unpinned(paths: &[String]) -> FreeUpSpaceResult {
    let mut result = FreeUpSpaceResult {
        succeeded: Vec::new(),
        failed: Vec::new(),
    };
    for path in paths {
        let mut cmd = std::process::Command::new("attrib.exe");
        cmd.args(["+U", "-P", path]);
        if std::path::Path::new(path).is_dir() {
            cmd.args(["/S", "/D"]);
        }
        match cmd.output() {
            Ok(out) if out.status.success() => result.succeeded.push(path.clone()),
            Ok(out) => result.failed.push(FreeUpFailure {
                path: path.clone(),
                error: format!("attrib exited with {}", out.status),
            }),
            Err(e) => result.failed.push(FreeUpFailure {
                path: path.clone(),
                error: format!("could not run attrib: {e}"),
            }),
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Category;

    #[test]
    fn within_root_matching() {
        let root = r"C:\Users\meda\OneDrive";
        assert!(path_is_within(root, r"C:\Users\meda\OneDrive"));
        assert!(path_is_within(root, r"C:\Users\meda\OneDrive\Docs\a.docx"));
        // Case and separators are Windows semantics, not host semantics.
        assert!(path_is_within(root, "c:/users/meda/onedrive/docs/a.docx"));
        assert!(path_is_within(
            "C:/Users/meda/OneDrive/",
            r"C:\Users\meda\OneDrive\a.docx"
        ));
        // Sibling with a shared prefix is NOT inside.
        assert!(!path_is_within(root, r"C:\Users\meda\OneDriveBackup\a.docx"));
        assert!(!path_is_within(root, r"C:\Users\meda\a.docx"));
        assert!(!path_is_within(root, r"D:\OneDrive\a.docx"));
        assert!(!path_is_within("", r"C:\Users\meda\OneDrive\a.docx"));
        assert!(!path_is_within(root, ""));
        // Traversal textually under the root resolves outside it:
        // refused, in either string, with either separator.
        assert!(!path_is_within(
            root,
            r"C:\Users\meda\OneDrive\..\..\Windows\evil.dll"
        ));
        assert!(!path_is_within(
            root,
            "C:/Users/meda/OneDrive/docs/../../secret.txt"
        ));
        assert!(!path_is_within(
            r"C:\Users\meda\OneDrive\..\OneDrive",
            r"C:\Users\meda\OneDrive\a.docx"
        ));
        // A dotted filename is not a parent reference.
        assert!(path_is_within(root, r"C:\Users\meda\OneDrive\my..file.txt"));
    }

    fn entry(path: &str, bytes: u64) -> FileEntry {
        FileEntry {
            path: path.to_string(),
            name: path.rsplit(['/', '\\']).next().unwrap_or(path).to_string(),
            bytes,
            category: Category::UserData,
            modified_ms: 0,
            ocr_text: None,
        }
    }

    #[test]
    fn status_counts_only_local_files_under_the_root() {
        let root = r"C:\Users\meda\OneDrive";
        let files = vec![
            entry(r"C:\Users\meda\OneDrive\a.bin", 1_000),
            entry(r"C:\Users\meda\OneDrive\sub\b.bin", 2_500),
            // Online-only placeholder: no allocated bytes, not local.
            entry(r"C:\Users\meda\OneDrive\cloud-only.bin", 0),
            entry(r"C:\Users\meda\Elsewhere\c.bin", 9_999),
        ];
        let status = cloud_status(root, &files);
        assert_eq!(status.provider, "OneDrive");
        assert_eq!(status.root_path, root);
        assert_eq!(status.local_bytes, 3_500);
        assert_eq!(status.file_count, 2);
    }

    #[test]
    fn detect_returns_none_without_env() {
        // The test host does not set OneDrive variables for this
        // process in practice; the assertion is only about not
        // panicking and honouring a variable that names no directory.
        let found = detect_onedrive_root();
        if std::env::var_os("OneDrive").is_none()
            && std::env::var_os("OneDriveConsumer").is_none()
            && std::env::var_os("OneDriveCommercial").is_none()
        {
            assert!(found.is_none());
        }
    }
}
