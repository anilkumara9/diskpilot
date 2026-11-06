//! Privacy helpers. Private folders are reported only as totals — never as
//! listings, never to the assistant, never in exports.

use crate::scanner::DirIndex;
use std::path::Path;

/// Folder names treated as private when they sit directly under the profile.
pub fn private_folder_names() -> &'static [&'static str] {
    &[".ssh", ".gnupg", ".aws", "credentials", ".config/gcloud"]
}

pub fn is_private_path(path: &Path, home: &Path) -> bool {
    let norm = path.to_string_lossy().replace('\\', "/").to_lowercase();
    let home_norm = home.to_string_lossy().replace('\\', "/").to_lowercase();
    for name in private_folder_names() {
        let full = format!("{}/{}", home_norm.trim_end_matches('/'), name);
        if norm == full || norm.starts_with(&format!("{}/", full)) {
            return true;
        }
    }
    false
}

/// Total bytes of private folders in a scan, with no path detail beyond the
/// folder name itself. Returns (folder name, bytes) pairs.
pub fn private_totals(index: &DirIndex, home: &Path) -> Vec<(String, u64)> {
    let mut out: Vec<(String, u64)> = Vec::new();
    for name in private_folder_names() {
        let full = home.join(name);
        let key = full.to_string_lossy().to_string();
        if let Some(bytes) = index.dir_sizes.get(&key) {
            out.push((name.to_string(), *bytes));
        }
    }
    out
}

/// Display form of a path for assistant / export contexts: home becomes `~`
/// and private paths collapse to their folder total label.
pub fn display_path(path: &str, home: &str) -> String {
    let p = Path::new(path);
    if is_private_path(p, Path::new(home)) {
        return "~/<private folder>".to_string();
    }
    if let Some(rest) = path.strip_prefix(home) {
        return format!("~{}", rest);
    }
    path.to_string()
}
