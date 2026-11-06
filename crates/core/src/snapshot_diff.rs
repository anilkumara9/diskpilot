//! Directory-level diff between two scan snapshots — "what changed
//! since the last scan?" at the granularity people act on.
//!
//! A snapshot summary carries directory byte sizes only for the
//! folders it surfaces (the root's children and the 50 heavy folders),
//! so a [`SnapshotView`] pairs those stored sizes with the snapshot's
//! file list, and the diff compares directories on ONE consistent
//! basis: the candidate set is the union of both sides' stored keys
//! and every ancestor directory of both sides' listed files, and
//! each side's value for a candidate is its stored total when it
//! has one, otherwise a prefix aggregation over ITS OWN file list
//! (every file contributes its bytes to every ancestor directory).
//!
//! That consistency matters because the stored set is a top-N: a
//! directory that merely fell out of one scan's stored top-50 must
//! compare at its file-derived size, not vanish at its full stored
//! size on one side and reappear on the other. A directory known to
//! only one side is reported added/removed only when the absence is
//! trustworthy — the other side carries no file list at all (its
//! stored set IS its whole knowledge), or the present side's own
//! files evidence the directory — otherwise it is suppressed:
//! unknown is not zero.
//!
//! File bytes are the scanner's allocated bytes (the product-wide
//! byte-accounting rule), in both the stored sizes and the file lists,
//! so every figure here is allocated bytes.

use crate::model::ScanSummary;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// One file's contribution to a snapshot view: its path and its
/// allocated bytes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotFile {
    pub path: String,
    pub bytes: u64,
}

/// The diffable part of one scan snapshot: stored directory sizes
/// (possibly partial, possibly empty) plus the snapshot's file list.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotView {
    pub dir_sizes: HashMap<String, u64>,
    pub files: Vec<SnapshotFile>,
}

impl SnapshotView {
    /// Build a view from a scan summary: directory sizes from the
    /// root's locations plus the heavy folders (heavy folders win on
    /// overlap — same measurement), files from the largest-files list.
    pub fn from_summary(summary: &ScanSummary) -> Self {
        let mut dir_sizes = HashMap::new();
        for entry in summary.locations.iter().chain(summary.heavy_folders.iter()) {
            dir_sizes.insert(entry.path.clone(), entry.bytes);
        }
        let files = summary
            .top_files
            .iter()
            .map(|f| SnapshotFile {
                path: f.path.clone(),
                bytes: f.bytes,
            })
            .collect();
        Self { dir_sizes, files }
    }

    /// This view's size for one directory on the diff's consistent
    /// basis: the stored total when present, otherwise a prefix
    /// aggregation over this view's own file list. `None` when the
    /// view knows nothing about the directory — no stored total and
    /// no listed file under it (or no file list at all).
    fn dir_value(&self, dir: &str) -> Option<u64> {
        if let Some(bytes) = self.dir_sizes.get(dir) {
            return Some(*bytes);
        }
        self.file_derived_total(dir)
    }

    /// The total of this view's listed files under `dir`, or `None`
    /// when no listed file sits under it.
    fn file_derived_total(&self, dir: &str) -> Option<u64> {
        let mut total = 0u64;
        let mut any = false;
        for file in &self.files {
            if ancestor_dirs(&file.path).iter().any(|a| a == dir) {
                total += file.bytes;
                any = true;
            }
        }
        if any {
            Some(total)
        } else {
            None
        }
    }

    /// Whether this view's own file list evidences `dir` (at least
    /// one listed file under it).
    fn has_file_evidence(&self, dir: &str) -> bool {
        self.file_derived_total(dir).is_some()
    }
}

/// Every ancestor directory of a file path, nearest first. Works on
/// both `/` and `\` separated paths (snapshot paths use the scanned
/// machine's separators), and never returns the path itself or an
/// empty string.
fn ancestor_dirs(path: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut current = path;
    while let Some(idx) = current.rfind(['/', '\\']) {
        if idx == 0 {
            break;
        }
        let dir = &current[..idx];
        if dir.is_empty() {
            break;
        }
        result.push(dir.to_string());
        current = dir;
    }
    result
}

/// The display name of a directory path: its last component, or the
/// path itself for a root-like path with no separator.
fn dir_name(path: &str) -> String {
    match path.rfind(['/', '\\']) {
        Some(idx) if idx + 1 < path.len() => path[idx + 1..].to_string(),
        _ => path.to_string(),
    }
}

/// One directory's change between the two snapshots. `delta_bytes` is
/// signed: positive means the directory grew.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Change {
    pub path: String,
    pub name: String,
    pub previous_bytes: u64,
    pub current_bytes: u64,
    pub delta_bytes: i64,
}

/// The four change lists of a snapshot diff, each capped at the
/// caller's limit: directories that grew, that shrank, that appear
/// only in the current snapshot, and that appear only in the previous
/// one.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BiggestChanges {
    pub grown: Vec<Change>,
    pub shrunk: Vec<Change>,
    pub added: Vec<Change>,
    pub removed: Vec<Change>,
}

/// Diff two snapshot views at directory level. `limit` caps each of
/// the four lists.
pub fn diff_snapshots(
    current: &SnapshotView,
    previous: &SnapshotView,
    limit: usize,
) -> BiggestChanges {
    // One candidate set for both sides: every stored key plus every
    // ancestor of every listed file, from either view.
    let mut candidates: Vec<String> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    for view in [current, previous] {
        for key in view.dir_sizes.keys() {
            if seen.insert(key.clone()) {
                candidates.push(key.clone());
            }
        }
        for file in &view.files {
            for ancestor in ancestor_dirs(&file.path) {
                if seen.insert(ancestor.clone()) {
                    candidates.push(ancestor);
                }
            }
        }
    }

    let mut changes = BiggestChanges::default();
    for path in &candidates {
        let current_bytes = current.dir_value(path);
        let previous_bytes = previous.dir_value(path);
        match (previous_bytes, current_bytes) {
            (Some(previous_bytes), Some(current_bytes)) => {
                let delta = current_bytes as i64 - previous_bytes as i64;
                if delta > 0 {
                    changes.grown.push(Change {
                        path: path.clone(),
                        name: dir_name(path),
                        previous_bytes,
                        current_bytes,
                        delta_bytes: delta,
                    });
                } else if delta < 0 {
                    changes.shrunk.push(Change {
                        path: path.clone(),
                        name: dir_name(path),
                        previous_bytes,
                        current_bytes,
                        delta_bytes: delta,
                    });
                }
            }
            (None, Some(current_bytes)) => {
                // One-sided: report only when the previous side's
                // absence is trustworthy — it keeps no file list at
                // all (complete-stored view), or the current side's
                // files evidence the directory.
                if current_bytes > 0
                    && (previous.files.is_empty() || current.has_file_evidence(path))
                {
                    changes.added.push(Change {
                        path: path.clone(),
                        name: dir_name(path),
                        previous_bytes: 0,
                        current_bytes,
                        delta_bytes: current_bytes as i64,
                    });
                }
            }
            (Some(previous_bytes), None) => {
                if current.files.is_empty() || previous.has_file_evidence(path) {
                    changes.removed.push(Change {
                        path: path.clone(),
                        name: dir_name(path),
                        previous_bytes,
                        current_bytes: 0,
                        delta_bytes: -(previous_bytes as i64),
                    });
                }
            }
            (None, None) => {}
        }
    }

    changes
        .grown
        .sort_by(|a, b| b.delta_bytes.cmp(&a.delta_bytes).then(a.path.cmp(&b.path)));
    changes
        .shrunk
        .sort_by(|a, b| a.delta_bytes.cmp(&b.delta_bytes).then(a.path.cmp(&b.path)));
    changes
        .added
        .sort_by(|a, b| b.current_bytes.cmp(&a.current_bytes).then(a.path.cmp(&b.path)));
    changes
        .removed
        .sort_by(|a, b| b.previous_bytes.cmp(&a.previous_bytes).then(a.path.cmp(&b.path)));
    changes.grown.truncate(limit);
    changes.shrunk.truncate(limit);
    changes.added.truncate(limit);
    changes.removed.truncate(limit);
    changes
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view(dirs: &[(&str, u64)], files: &[(&str, u64)]) -> SnapshotView {
        SnapshotView {
            dir_sizes: dirs
                .iter()
                .map(|(p, b)| (p.to_string(), *b))
                .collect(),
            files: files
                .iter()
                .map(|(p, b)| SnapshotFile {
                    path: p.to_string(),
                    bytes: *b,
                })
                .collect(),
        }
    }

    #[test]
    fn grows_shrinks_adds_and_removes() {
        let previous = view(
            &[("/root/up", 100), ("/root/down", 100), ("/root/gone", 50)],
            &[],
        );
        let current = view(
            &[("/root/up", 160), ("/root/down", 40), ("/root/new", 30)],
            &[],
        );
        let diff = diff_snapshots(&current, &previous, 8);
        assert_eq!(diff.grown.len(), 1);
        assert_eq!(diff.grown[0].path, "/root/up");
        assert_eq!(diff.grown[0].name, "up");
        assert_eq!(diff.grown[0].delta_bytes, 60);
        assert_eq!(diff.shrunk.len(), 1);
        assert_eq!(diff.shrunk[0].delta_bytes, -60);
        assert_eq!(diff.added.len(), 1);
        assert_eq!(diff.added[0].path, "/root/new");
        assert_eq!(diff.added[0].previous_bytes, 0);
        assert_eq!(diff.removed.len(), 1);
        assert_eq!(diff.removed[0].path, "/root/gone");
        assert_eq!(diff.removed[0].current_bytes, 0);
        assert_eq!(diff.removed[0].delta_bytes, -50);
    }

    #[test]
    fn identical_snapshots_diff_to_nothing() {
        let v = view(&[("/root/a", 10), ("/root/b", 20)], &[]);
        let diff = diff_snapshots(&v, &v, 8);
        assert_eq!(diff, BiggestChanges::default());
    }

    #[test]
    fn file_lists_derive_directory_totals_by_prefix() {
        let previous = view(&[], &[("/root/a/f1.bin", 100), ("/root/a/f2.bin", 50)]);
        let current = view(
            &[],
            &[("/root/a/f1.bin", 100), ("/root/a/f2.bin", 50), ("/root/b/f3.bin", 70)],
        );
        let diff = diff_snapshots(&current, &previous, 8);
        // /root/b appears (70 bytes under it); /root aggregates both
        // sides, so it grows by 70 too; /root/a is unchanged.
        let added_paths: Vec<&str> = diff.added.iter().map(|c| c.path.as_str()).collect();
        assert_eq!(added_paths, ["/root/b"]);
        assert_eq!(diff.added[0].current_bytes, 70);
        assert_eq!(diff.grown.len(), 1);
        assert_eq!(diff.grown[0].path, "/root");
        assert_eq!(diff.grown[0].delta_bytes, 70);
    }

    #[test]
    fn windows_style_paths_split_on_backslashes() {
        let previous = view(&[], &[("C:\\Users\\a\\old\\f.bin", 10)]);
        let current = view(&[], &[("C:\\Users\\a\\new\\f.bin", 10)]);
        let diff = diff_snapshots(&current, &previous, 8);
        let added: Vec<&str> = diff.added.iter().map(|c| c.path.as_str()).collect();
        assert!(added.contains(&"C:\\Users\\a\\new"), "added {added:?}");
        let removed: Vec<&str> = diff.removed.iter().map(|c| c.path.as_str()).collect();
        assert!(removed.contains(&"C:\\Users\\a\\old"), "removed {removed:?}");
    }

    #[test]
    fn limit_caps_each_list() {
        let previous = view(&[("/r/a", 1), ("/r/b", 1), ("/r/c", 1)], &[]);
        let current = view(&[("/r/a", 10), ("/r/b", 20), ("/r/c", 30)], &[]);
        let diff = diff_snapshots(&current, &previous, 2);
        assert_eq!(diff.grown.len(), 2);
        // Biggest growth first.
        assert_eq!(diff.grown[0].path, "/r/c");
        assert_eq!(diff.grown[1].path, "/r/b");
    }

    #[test]
    fn zero_byte_additions_are_not_reported() {
        let previous = view(&[], &[]);
        let current = view(&[("/root/empty", 0)], &[]);
        let diff = diff_snapshots(&current, &previous, 8);
        assert!(diff.added.is_empty());
    }

    #[test]
    fn rank_flip_compares_instead_of_fabricating_add_and_remove() {
        // The audit's phantom: /root/x is in the previous scan's
        // stored set at 100, but in the current scan it fell out of
        // the stored top-N while its files are still listed (90).
        // The old per-side basis reported x removed at 100 and the
        // newcomer /root/y added at 200; the consistent basis
        // compares x (100 vs file-derived 90 → shrunk 10) and
        // suppresses y, which neither side's files evidence and
        // whose absence on the previous side is not trustworthy
        // (the previous side does carry a file list).
        let previous = view(&[("/root/x", 100)], &[("/root/x/f.bin", 90)]);
        let current = view(&[("/root/y", 200)], &[("/root/x/f.bin", 90)]);
        let diff = diff_snapshots(&current, &previous, 8);
        assert!(diff.added.is_empty(), "added: {:?}", diff.added);
        assert!(diff.removed.is_empty(), "removed: {:?}", diff.removed);
        assert_eq!(diff.shrunk.len(), 1);
        assert_eq!(diff.shrunk[0].path, "/root/x");
        assert_eq!(diff.shrunk[0].previous_bytes, 100);
        assert_eq!(diff.shrunk[0].current_bytes, 90);
        assert_eq!(diff.shrunk[0].delta_bytes, -10);
    }

    #[test]
    fn one_sided_stored_dir_is_reported_when_other_side_is_complete_stored() {
        // Both views stored-only (the history shape): a one-sided
        // directory is a genuine add/remove, as before.
        let previous = view(&[("/root/a", 10)], &[]);
        let current = view(&[("/root/a", 10), ("/root/b", 70)], &[]);
        let diff = diff_snapshots(&current, &previous, 8);
        assert_eq!(diff.added.len(), 1);
        assert_eq!(diff.added[0].path, "/root/b");
    }

    #[test]
    fn summary_view_uses_stored_directory_sizes() {
        let summary = ScanSummary {
            heavy_folders: vec![crate::model::LocationEntry {
                path: "/root/big".to_string(),
                name: "big".to_string(),
                bytes: 500,
                category: crate::model::Category::UserData,
            }],
            ..ScanSummary::default()
        };
        let v = SnapshotView::from_summary(&summary);
        assert_eq!(v.dir_sizes.get("/root/big"), Some(&500));
    }
}
