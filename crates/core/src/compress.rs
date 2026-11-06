//! NTFS compression candidates: big, cold directory subtrees that
//! would give real space back if Windows compressed them in place.
//!
//! Detection is advisory and index-only, like the Clean Up cards:
//! nothing here touches the filesystem. Compressing (or
//! decompressing) a folder the user picks happens in the shell,
//! which re-validates the path through the safety gate at action
//! time and runs `compact.exe` itself.
//!
//! A directory qualifies when ALL of these hold:
//!
//! - its subtree totals at least 256 MB of allocated bytes
//!   (decimal, the same convention as the search grammar and the
//!   Clean Up cards);
//! - the newest file in the subtree was last modified at least
//!   90 days ago — the whole subtree is cold, so the CPU cost of
//!   (de)compression on access is unlikely to be felt;
//! - it is not the scan root itself (compressing the root is a
//!   whole-tree decision, not a per-folder suggestion);
//! - the classifier did not mark it [`Category::SystemFiles`] and
//!   it is not a report-only location
//!   ([`crate::safety::is_report_only`], e.g. `Windows.old`):
//!   system territory is never offered, and the shell's action-time
//!   validation refuses it again anyway.
//!
//! Age evidence rule, mirroring the cards: a file with no recorded
//! mtime (`modified_ms <= 0`) is not evidence of anything. A
//! directory whose subtree has NO file with a recorded mtime is
//! never called cold; when some files carry an mtime, the newest
//! recorded one decides.

use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::model::{now_ms, Category};
use crate::safety;
use crate::scanner::DirIndex;

const DAY_MS: i64 = 86_400_000;

/// Minimum subtree size: 256 MB (decimal).
pub const MIN_SUBTREE_BYTES: u64 = 256_000_000;
/// The newest file in the subtree must be at least this old.
pub const MIN_AGE_MS: i64 = 90 * DAY_MS;
/// How many candidates are offered at most.
pub const MAX_CANDIDATES: usize = 20;

/// One directory subtree offered for NTFS compression.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompressionCandidate {
    /// Directory path, in the index's own spelling.
    pub path: String,
    /// Allocated bytes of the whole subtree (the index's directory
    /// total, so every surface agrees on the figure).
    pub bytes: u64,
    /// Files in the subtree.
    pub file_count: u64,
    /// Newest recorded file modification in the subtree, Unix epoch
    /// milliseconds; the age evidence the candidate rests on.
    pub newest_modified_ms: i64,
}

/// The parent directory of an index path, in the same string form:
/// everything before the last separator (either spelling counts;
/// index paths are built with the host separator, but the MFT
/// builder's joins are `\` even in strings a `/`-rooted scan would
/// otherwise spell with `/`). `None` at a filesystem root.
fn parent_dir(path: &str) -> Option<&str> {
    let idx = path.rfind(['\\', '/'])?;
    if idx == 0 {
        // "/etc" -> parent is the root "/"; the root itself has none.
        return if path.len() > 1 { Some(&path[..1]) } else { None };
    }
    Some(&path[..idx])
}

/// The compression candidates of a scan index, largest first
/// (path order breaks byte ties, so the list is deterministic).
/// At most [`MAX_CANDIDATES`].
pub fn compression_candidates(index: &DirIndex) -> Vec<CompressionCandidate> {
    compression_candidates_at(index, now_ms())
}

/// [`compression_candidates`] against an explicit clock, so the
/// age rule is testable without waiting 90 days.
fn compression_candidates_at(index: &DirIndex, now: i64) -> Vec<CompressionCandidate> {
    if index.dir_sizes.is_empty() {
        return Vec::new();
    }
    // Per-subtree file counts and newest recorded mtimes, gathered
    // by walking each file up through the ancestors the index knows.
    let mut file_counts: HashMap<&str, u64> = HashMap::new();
    let mut newest: HashMap<&str, i64> = HashMap::new();
    for file in &index.files {
        let mut dir = parent_dir(&file.path);
        while let Some(d) = dir {
            if !index.dir_sizes.contains_key(d) {
                break;
            }
            *file_counts.entry(d).or_default() += 1;
            if file.modified_ms > 0 {
                let slot = newest.entry(d).or_default();
                if file.modified_ms > *slot {
                    *slot = file.modified_ms;
                }
            }
            dir = parent_dir(d);
        }
    }
    // The scan root is the directory no other indexed directory is
    // the parent of; it is never a candidate itself.
    let roots: Vec<&str> = index
        .dir_sizes
        .keys()
        .map(String::as_str)
        .filter(|path| match parent_dir(path) {
            Some(parent) => !index.dir_sizes.contains_key(parent),
            None => true,
        })
        .collect();

    let mut candidates: Vec<CompressionCandidate> = Vec::new();
    for (path, &bytes) in &index.dir_sizes {
        if bytes < MIN_SUBTREE_BYTES {
            continue;
        }
        if roots.contains(&path.as_str()) {
            continue;
        }
        if index.dir_category.get(path) == Some(&Category::SystemFiles) {
            continue;
        }
        if safety::is_report_only(Path::new(path)) {
            continue;
        }
        let Some(&newest_ms) = newest.get(path.as_str()) else {
            continue; // no file with a recorded mtime: no age evidence
        };
        if now - newest_ms < MIN_AGE_MS {
            continue;
        }
        candidates.push(CompressionCandidate {
            path: path.clone(),
            bytes,
            file_count: file_counts.get(path.as_str()).copied().unwrap_or(0),
            newest_modified_ms: newest_ms,
        });
    }
    candidates.sort_by(|a, b| b.bytes.cmp(&a.bytes).then_with(|| a.path.cmp(&b.path)));
    candidates.truncate(MAX_CANDIDATES);
    candidates
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::FileEntry;

    const NOW: i64 = 1_800_000_000_000;
    const OLD: i64 = NOW - 120 * DAY_MS; // comfortably past 90 days
    const FRESH: i64 = NOW - 10 * DAY_MS;

    fn entry(path: &str, bytes: u64, modified_ms: i64) -> FileEntry {
        FileEntry {
            path: path.to_string(),
            name: path.rsplit(['\\', '/']).next().unwrap_or(path).to_string(),
            bytes,
            category: Category::UserData,
            modified_ms,
            ocr_text: None,
        }
    }

    /// Build an index over `root` from file entries, deriving the
    /// directory totals the way the scanner's aggregation does, and
    /// stamping the given category on the given directories.
    fn build_index(
        root: &str,
        files: Vec<FileEntry>,
        dir_categories: &[(&str, Category)],
    ) -> DirIndex {
        let mut index = DirIndex {
            files,
            ..DirIndex::default()
        };
        index.dir_sizes.insert(root.to_string(), 0);
        for file in &index.files {
            let mut dir = parent_dir(&file.path).map(str::to_string);
            while let Some(d) = dir {
                *index.dir_sizes.entry(d.clone()).or_default() += file.bytes;
                if d == root {
                    break;
                }
                dir = parent_dir(&d).map(str::to_string);
            }
        }
        for (path, category) in dir_categories {
            index.dir_category.insert(path.to_string(), *category);
        }
        index
    }

    #[test]
    fn a_big_cold_directory_qualifies() {
        let root = r"C:\Root";
        let files = vec![
            entry(r"C:\Root\Old\a.bin", 200_000_000, OLD),
            entry(r"C:\Root\Old\sub\b.bin", 100_000_000, OLD - DAY_MS),
            entry(r"C:\Root\Fresh\c.bin", 5_000, FRESH),
        ];
        let index = build_index(root, files, &[]);
        let found = compression_candidates_at(&index, NOW);
        assert_eq!(found.len(), 1);
        let c = &found[0];
        assert_eq!(c.path, r"C:\Root\Old");
        assert_eq!(c.bytes, 300_000_000);
        assert_eq!(c.file_count, 2);
        assert_eq!(
            c.newest_modified_ms, OLD,
            "the newest file in the subtree decides"
        );
    }

    #[test]
    fn too_small_and_too_fresh_directories_are_out() {
        let root = r"C:\Root";
        let files = vec![
            entry(r"C:\Root\Small\a.bin", 100_000_000, OLD),
            entry(r"C:\Root\Warm\a.bin", 300_000_000, OLD),
            // One fresh file warms the whole subtree.
            entry(r"C:\Root\Warm\b.bin", 1_000, FRESH),
        ];
        let index = build_index(root, files, &[]);
        assert!(compression_candidates_at(&index, NOW).is_empty());
    }

    #[test]
    fn the_scan_root_itself_is_never_a_candidate() {
        let root = r"C:\Root";
        let files = vec![entry(r"C:\Root\a.bin", 900_000_000, OLD)];
        let index = build_index(root, files, &[]);
        assert!(compression_candidates_at(&index, NOW).is_empty());
    }

    #[test]
    fn system_files_and_report_only_directories_are_out() {
        let root = r"C:\Root";
        let files = vec![
            entry(r"C:\Root\WinSxS\a.bin", 400_000_000, OLD),
            entry(r"C:\Root\Windows.old\a.bin", 400_000_000, OLD),
            entry(r"C:\Root\Plain\a.bin", 400_000_000, OLD),
        ];
        let index = build_index(
            root,
            files,
            &[(r"C:\Root\WinSxS", Category::SystemFiles)],
        );
        let found = compression_candidates_at(&index, NOW);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].path, r"C:\Root\Plain");
    }

    #[test]
    fn no_mtime_evidence_means_no_candidate() {
        let root = r"C:\Root";
        let files = vec![
            entry(r"C:\Root\Unknown\a.bin", 400_000_000, 0),
            entry(r"C:\Root\Unknown\b.bin", 100_000_000, -5),
        ];
        let index = build_index(root, files, &[]);
        assert!(compression_candidates_at(&index, NOW).is_empty());

        // A single dated file is enough evidence for the subtree.
        let files = vec![
            entry(r"C:\Root\Unknown\a.bin", 400_000_000, 0),
            entry(r"C:\Root\Unknown\b.bin", 100_000_000, OLD),
        ];
        let index = build_index(root, files, &[]);
        let found = compression_candidates_at(&index, NOW);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].file_count, 2, "undated files still count as files");
        assert_eq!(found[0].newest_modified_ms, OLD);
    }

    #[test]
    fn nested_candidates_are_both_listed_largest_first() {
        let root = r"C:\Root";
        let files = vec![
            entry(r"C:\Root\Outer\a.bin", 200_000_000, OLD),
            entry(r"C:\Root\Outer\Inner\b.bin", 300_000_000, OLD),
        ];
        let index = build_index(root, files, &[]);
        let found = compression_candidates_at(&index, NOW);
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].path, r"C:\Root\Outer");
        assert_eq!(found[0].bytes, 500_000_000);
        assert_eq!(found[1].path, r"C:\Root\Outer\Inner");
    }

    #[test]
    fn the_list_is_capped_at_twenty() {
        let root = r"C:\Root";
        let files: Vec<FileEntry> = (0..25)
            .map(|i| {
                entry(
                    &format!(r"C:\Root\D{i:02}\a.bin"),
                    300_000_000 + i as u64,
                    OLD,
                )
            })
            .collect();
        let index = build_index(root, files, &[]);
        let found = compression_candidates_at(&index, NOW);
        assert_eq!(found.len(), MAX_CANDIDATES);
        // The smallest qualifying subtree is the one dropped.
        assert!(found.iter().all(|c| c.bytes > 300_000_004));
    }

    #[test]
    fn empty_index_has_no_candidates() {
        let index = DirIndex::default();
        assert!(compression_candidates_at(&index, NOW).is_empty());
        let _ = compression_candidates(&index); // the public entry compiles + agrees
    }
}
