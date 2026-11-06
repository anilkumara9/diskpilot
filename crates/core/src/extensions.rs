//! Per-extension footprint of a scan index: how much space each
//! file type occupies, and the largest files of one type. Pure
//! aggregations over the index — the Storage page's Types view is
//! served from these, so the figures agree with every other
//! surface by construction.

use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::model::FileEntry;
use crate::scanner::DirIndex;

/// Allocated bytes and file count for one extension.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtensionStat {
    /// Lowercase extension without the dot; `""` groups the files
    /// that have no extension (dotfiles like `.gitignore` included —
    /// `Path::extension` semantics).
    pub extension: String,
    /// Sum of allocated bytes (the product's one byte rule).
    pub bytes: u64,
    /// Files carrying this extension.
    pub count: u64,
}

/// The normalized extension of a file name: lowercase, no dot,
/// `""` when the name has none.
fn extension_of_name(name: &str) -> String {
    Path::new(name)
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default()
}

/// Normalize a caller-supplied extension for comparison: trim,
/// drop one leading dot, lowercase (`"JPG"`, `".jpg"` and `"jpg"`
/// all address the same group).
fn normalize_query(ext: &str) -> String {
    ext.trim()
        .strip_prefix('.')
        .unwrap_or(ext.trim())
        .to_lowercase()
}

/// Per-extension totals over the index's files, largest first
/// (extension order breaks byte ties, so the list is
/// deterministic), truncated to `limit` groups.
pub fn extension_stats(index: &DirIndex, limit: usize) -> Vec<ExtensionStat> {
    let mut by_ext: HashMap<String, (u64, u64)> = HashMap::new();
    for file in &index.files {
        let slot = by_ext.entry(extension_of_name(&file.name)).or_default();
        slot.0 += file.bytes;
        slot.1 += 1;
    }
    let mut stats: Vec<ExtensionStat> = by_ext
        .into_iter()
        .map(|(extension, (bytes, count))| ExtensionStat {
            extension,
            bytes,
            count,
        })
        .collect();
    stats.sort_by(|a, b| {
        b.bytes
            .cmp(&a.bytes)
            .then_with(|| a.extension.cmp(&b.extension))
    });
    stats.truncate(limit);
    stats
}

/// The index's own entries carrying `ext`, largest first by
/// allocated bytes (path order breaks byte ties), truncated to
/// `limit`. The entries are returned as stored — the caller shows
/// exactly what the scan recorded.
pub fn largest_files_with_extension(index: &DirIndex, ext: &str, limit: usize) -> Vec<FileEntry> {
    let want = normalize_query(ext);
    let mut files: Vec<FileEntry> = index
        .files
        .iter()
        .filter(|f| extension_of_name(&f.name) == want)
        .cloned()
        .collect();
    files.sort_by(|a, b| b.bytes.cmp(&a.bytes).then_with(|| a.path.cmp(&b.path)));
    files.truncate(limit);
    files
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Category;

    fn entry(path: &str, bytes: u64) -> FileEntry {
        FileEntry {
            path: path.to_string(),
            name: path.rsplit(['\\', '/']).next().unwrap_or(path).to_string(),
            bytes,
            category: Category::UserData,
            modified_ms: 0,
            ocr_text: None,
        }
    }

    fn make_index(files: Vec<FileEntry>) -> DirIndex {
        DirIndex {
            files,
            ..DirIndex::default()
        }
    }

    #[test]
    fn case_folds_and_extensionless_files_group_under_empty() {
        let index = make_index(vec![
            entry(r"C:\Pics\a.JPG", 100),
            entry(r"C:\Pics\b.jpg", 50),
            entry(r"C:\Pics\c.Jpeg", 30),
            entry(r"C:\Docs\README", 10),
            entry(r"C:\Docs\.gitignore", 5),
            entry(r"C:\Docs\archive.tar.gz", 200),
        ]);
        let stats = extension_stats(&index, 100);
        assert_eq!(stats.len(), 4);
        // gz first (200), then the merged jpg group (150).
        assert_eq!(
            stats[0],
            ExtensionStat {
                extension: "gz".into(),
                bytes: 200,
                count: 1
            }
        );
        assert_eq!(
            stats[1],
            ExtensionStat {
                extension: "jpg".into(),
                bytes: 150,
                count: 2
            },
            "JPG and jpg are one group"
        );
        assert_eq!(
            stats[2],
            ExtensionStat {
                extension: "jpeg".into(),
                bytes: 30,
                count: 1
            },
            "jpeg is its own extension, not folded into jpg"
        );
        assert_eq!(
            stats[3],
            ExtensionStat {
                extension: String::new(),
                bytes: 15,
                count: 2
            },
            "no extension and dotfiles group under \"\""
        );
    }

    #[test]
    fn stats_sort_by_bytes_then_extension_and_honour_the_limit() {
        let index = make_index(vec![
            entry(r"C:\x\a.bin", 100),
            entry(r"C:\x\b.dat", 100),
            entry(r"C:\x\c.zip", 300),
        ]);
        let stats = extension_stats(&index, 2);
        assert_eq!(stats.len(), 2);
        assert_eq!(stats[0].extension, "zip");
        assert_eq!(stats[1].extension, "bin", "byte tie breaks by extension");
        assert!(extension_stats(&index, 0).is_empty());
        assert!(extension_stats(&make_index(vec![]), 100).is_empty());
    }

    #[test]
    fn largest_files_filters_sorts_and_limits() {
        let idx = make_index(vec![
            entry(r"C:\Vids\small.mp4", 10),
            entry(r"C:\Vids\big.mp4", 900),
            entry(r"C:\Vids\mid.MP4", 500),
            entry(r"C:\Vids\other.mkv", 5_000),
            entry(r"C:\Vids\also-big.mp4", 900),
        ]);
        let found = largest_files_with_extension(&idx, "mp4", 10);
        let paths: Vec<&str> = found.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(
            paths,
            [
                r"C:\Vids\also-big.mp4", // 900, path tiebreak before big.mp4
                r"C:\Vids\big.mp4",
                r"C:\Vids\mid.MP4",
                r"C:\Vids\small.mp4",
            ]
        );
        // The query normalizes case and a leading dot.
        assert_eq!(largest_files_with_extension(&idx, ".MP4", 2).len(), 2);
        assert_eq!(
            largest_files_with_extension(&idx, "MP4", 1)[0].path,
            r"C:\Vids\also-big.mp4"
        );
        // Extensionless lookup returns the extensionless files.
        let bare = make_index(vec![entry(r"C:\x\LICENSE", 7), entry(r"C:\x\a.txt", 9)]);
        assert_eq!(largest_files_with_extension(&bare, "", 10).len(), 1);
        assert!(largest_files_with_extension(&idx, "nope", 10).is_empty());
    }
}
