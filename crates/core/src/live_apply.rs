//! Applying USN journal changes to an existing scan index — the
//! mutation half of the live index.
//!
//! Soundness is the whole design. A change is applied only when it
//! can be applied *exactly*: a file removal subtracts the stored
//! entry's bytes from every ancestor total; a file upsert re-stats
//! the file through the same metadata calls the walker makes and
//! re-classifies it under its parent directory's stored
//! classification. Anything that cannot be applied exactly — a
//! directory changed (its subtree contents are unknowable from one
//! record), a file appeared in a directory the index does not
//! contain, a hard-linked file whose counted bytes cannot be
//! attributed — marks the batch stale instead of guessing. A stale
//! index is honest: it keeps serving the last scan's data, flagged,
//! until the next scan reconciles it.
//!
//! Paths are matched case-insensitively (NTFS semantics): journal
//! paths are anchored at the volume's canonical prefix and carry
//! the MFT's stored case, while index keys preserve the scan
//! root's spelling, so all lookups go through lower-cased maps
//! built once per batch, and mutations always write back through
//! the index's own stored path strings.

use crate::classifier;
use crate::model::{path_to_string, FileEntry};
use crate::scanner::{allocated_size_for_path, is_excluded, DirIndex, IndexChild};
use crate::usn::ChangeKind;
use crate::usn_volume::ResolvedChange;
use std::collections::HashMap;
use std::path::Path;

/// What one batch of journal changes did to the index.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ApplyReport {
    /// File entries removed (deletes, rename sources, files that
    /// vanished before their re-stat).
    pub removed: usize,
    /// File entries inserted or refreshed by re-stat.
    pub upserted: usize,
    /// The batch could not be applied exactly; the index must be
    /// treated as stale until the next full scan.
    pub became_stale: bool,
}

/// Apply `changes` (already filtered to the scan root, paths
/// resolved) to `index`, in journal order. `root` is the scan root
/// the index was built from; `exclusions` are the scan's exclusion
/// patterns, so a change to an excluded path stays out of scope
/// exactly as it would in a fresh scan.
pub fn apply_changes(
    index: &mut DirIndex,
    root: &Path,
    exclusions: &[String],
    changes: &[ResolvedChange],
) -> ApplyReport {
    let mut applier = Applier {
        positions: index
            .files
            .iter()
            .enumerate()
            .map(|(i, f)| (f.path.to_lowercase(), i))
            .collect(),
        dir_keys: index
            .dir_sizes
            .keys()
            .map(|k| (k.to_lowercase(), k.clone()))
            .collect(),
        root_str: path_to_string(root),
        base_skip: root.components().count().saturating_sub(1),
        report: ApplyReport::default(),
    };
    for change in changes {
        applier.apply_one(index, exclusions, change);
    }
    applier.report
}

struct Applier {
    /// Lower-cased file path → position in `index.files`.
    positions: HashMap<String, usize>,
    /// Lower-cased directory path → the index's stored key.
    dir_keys: HashMap<String, String>,
    root_str: String,
    base_skip: usize,
    report: ApplyReport,
}

impl Applier {
    fn apply_one(&mut self, index: &mut DirIndex, exclusions: &[String], change: &ResolvedChange) {
        // Any directory record — created, deleted, renamed, or
        // merely touched — asserts something about a subtree this
        // one record cannot describe. Never partial-apply it.
        if change.record.is_directory() {
            self.report.became_stale = true;
            return;
        }
        let Some(path) = change.path.as_deref() else {
            // Unresolved changes are the caller's stale signal;
            // there is nothing sound to apply for them here.
            return;
        };
        match change.record.kind() {
            ChangeKind::Deleted | ChangeKind::RenamedFrom => self.remove(index, path),
            ChangeKind::Created | ChangeKind::Modified | ChangeKind::RenamedTo => {
                self.upsert(index, exclusions, path)
            }
        }
    }

    /// The index's stored key for a directory path, matched
    /// case-insensitively.
    fn dir_key(&self, path: &str) -> Option<String> {
        self.dir_keys.get(&path.to_lowercase()).cloned()
    }

    fn remove(&mut self, index: &mut DirIndex, path: &str) {
        // A removed path that is a directory in the index would
        // orphan its subtree entries: structural, therefore stale.
        if self.dir_key(path).is_some() {
            self.report.became_stale = true;
            return;
        }
        let Some(&pos) = self.positions.get(&path.to_lowercase()) else {
            return; // not indexed (out of scope or already gone)
        };
        let entry = index.files.swap_remove(pos);
        self.positions.remove(&entry.path.to_lowercase());
        if pos < index.files.len() {
            // The swap moved the last entry into `pos`.
            let moved = index.files[pos].path.to_lowercase();
            self.positions.insert(moved, pos);
        }
        self.add_delta(index, &entry.path, -(entry.bytes as i64));
        if let Some(parent) = parent_of(&entry.path) {
            if let Some(key) = self.dir_key(parent) {
                if let Some(children) = index.children.get_mut(&key) {
                    children.retain(|c| c.path.to_lowercase() != entry.path.to_lowercase());
                }
            }
        }
        self.report.removed += 1;
    }

    fn upsert(&mut self, index: &mut DirIndex, exclusions: &[String], path: &str) {
        // Excluded paths are silently out of scope, exactly as in
        // a scan: an excluded file that changed is a removal if it
        // was somehow indexed, otherwise nothing.
        let rel = Path::new(path)
            .strip_prefix(&self.root_str)
            .unwrap_or(Path::new(path));
        if is_excluded(rel, exclusions) {
            self.remove(index, path);
            return;
        }
        let md = match std::fs::symlink_metadata(path) {
            Ok(md) => md,
            Err(_) => {
                // Gone between the journal write and now: a delete
                // in effect.
                self.remove(index, path);
                return;
            }
        };
        let ft = md.file_type();
        if ft.is_symlink() || is_reparse_point(&md) {
            // The walker never indexes links or reparse points.
            self.remove(index, path);
            return;
        }
        if ft.is_dir() {
            self.report.became_stale = true;
            return;
        }
        if !ft.is_file() {
            self.remove(index, path);
            return;
        }
        // Hard-link accounting: the walker counts a multiply-linked
        // file once, under one of its paths, and from here there is
        // no way to know whether this path was the counted one.
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            if md.nlink() > 1 {
                self.report.became_stale = true;
                return;
            }
        }
        // The parent directory must be part of the index, with its
        // stored classification to inherit from. A file appearing
        // in an unknown directory means the tree changed shape.
        let Some(parent) = parent_of(path) else {
            self.report.became_stale = true;
            return;
        };
        let Some(parent_key) = self.dir_key(parent) else {
            self.report.became_stale = true;
            return;
        };
        let Some(parent_class) = index.dir_classification.get(&parent_key).cloned() else {
            self.report.became_stale = true;
            return;
        };
        let parent_scope = classifier::scope_of(&parent_class.rule_id);
        let (class, _) = classifier::classify_child_from(
            Path::new(path),
            false,
            Some(&(parent_class, parent_scope)),
            self.base_skip,
        );
        let bytes = allocated_size_for_path(Path::new(path));
        let name = Path::new(path)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();

        if let Some(&pos) = self.positions.get(&path.to_lowercase()) {
            // Refresh in place, keeping the index's stored path
            // string (its root spelling and case are the index's
            // own conventions).
            let stored_path = index.files[pos].path.clone();
            let old_bytes = index.files[pos].bytes;
            index.files[pos] = FileEntry {
                path: stored_path.clone(),
                name,
                bytes,
                category: class.category,
                modified_ms: mtime_ms(&md),
                ocr_text: None,
            };
            self.add_delta(index, &stored_path, bytes as i64 - old_bytes as i64);
            self.update_child(index, &parent_key, &stored_path, bytes, class.category);
            self.report.upserted += 1;
        } else {
            // A new file: its stored path joins the parent's stored
            // key, so the entry matches index conventions exactly.
            let stored_path = join_child(&parent_key, &name);
            let pos = index.files.len();
            index.files.push(FileEntry {
                path: stored_path.clone(),
                name,
                bytes,
                category: class.category,
                modified_ms: mtime_ms(&md),
                ocr_text: None,
            });
            self.positions.insert(stored_path.to_lowercase(), pos);
            self.add_delta(index, &stored_path, bytes as i64);
            if let Some(children) = index.children.get_mut(&parent_key) {
                children.push(IndexChild {
                    name: Path::new(&stored_path)
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_default(),
                    path: stored_path.clone(),
                    bytes,
                    is_dir: false,
                    category: class.category,
                });
                children.sort_by(|a, b| a.name.cmp(&b.name));
            }
            self.report.upserted += 1;
        }
    }

    fn update_child(
        &self,
        index: &mut DirIndex,
        parent_key: &str,
        path: &str,
        bytes: u64,
        category: crate::model::Category,
    ) {
        if let Some(children) = index.children.get_mut(parent_key) {
            if let Some(child) = children
                .iter_mut()
                .find(|c| c.path.to_lowercase() == path.to_lowercase())
            {
                child.bytes = bytes;
                child.category = category;
            }
        }
    }

    /// Add a signed byte delta to every ancestor directory total of
    /// `file_path`, from its parent up to and including the root.
    fn add_delta(&self, index: &mut DirIndex, file_path: &str, delta: i64) {
        if delta == 0 {
            return;
        }
        let mut current = parent_of(file_path).map(str::to_string);
        while let Some(dir) = current {
            let Some(key) = self.dir_key(&dir) else { break };
            if let Some(total) = index.dir_sizes.get_mut(&key) {
                *total = total.saturating_add_signed(delta);
            }
            if key == self.root_str {
                break;
            }
            current = parent_of(&key).map(str::to_string);
        }
    }
}

/// Parent directory of a path string, splitting on the last `\` or
/// `/`. A bare drive parent (`C:`) is returned in its rooted form
/// (`C:\`) — the form index keys take.
fn parent_of(path: &str) -> Option<&str> {
    let idx = path.rfind(['\\', '/'])?;
    if idx == 0 {
        return Some(&path[..1]);
    }
    let parent = &path[..idx];
    if parent.len() == 2 && parent.as_bytes()[1] == b':' {
        // "C:\file" split at the separator gives "C:"; the index
        // keys the root as "C:\".
        return Some(if path.as_bytes()[idx] == b'\\' {
            &path[..idx + 1]
        } else {
            parent
        });
    }
    Some(parent)
}

/// Join a directory key and a file name the way the walker's
/// `read_dir` joins produce them: with the separator the key
/// itself uses.
fn join_child(parent_key: &str, name: &str) -> String {
    if parent_key.ends_with('\\') || parent_key.ends_with('/') {
        format!("{parent_key}{name}")
    } else if parent_key.contains('\\') {
        format!("{parent_key}\\{name}")
    } else {
        format!("{parent_key}/{name}")
    }
}

fn is_reparse_point(md: &std::fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0400;
        md.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
    }
    #[cfg(not(windows))]
    {
        let _ = md;
        false
    }
}

/// Milliseconds since the Unix epoch for a metadata mtime; 0 when
/// the platform reports no usable mtime (the walker's convention).
fn mtime_ms(md: &std::fs::Metadata) -> i64 {
    match md.modified() {
        Ok(t) => match t.duration_since(std::time::UNIX_EPOCH) {
            Ok(d) => d.as_millis().min(i64::MAX as u128) as i64,
            Err(_) => 0,
        },
        Err(_) => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::classifier;
    use crate::usn::UsnRecord;
    use std::io::Write;
    use std::path::PathBuf;

    fn change(kind_reason: u32, attrs: u32, name: &str, path: Option<String>) -> ResolvedChange {
        ResolvedChange {
            record: UsnRecord {
                file_ref: 100,
                parent_ref: 5,
                usn: 0,
                timestamp_ms: 0,
                reason: kind_reason,
                file_attributes: attrs,
                name: name.to_string(),
            },
            path,
        }
    }

    /// An index over a fabricated tree rooted at C:\root:
    ///   sub\ (dir, classified by the real classifier)
    ///   sub\a.txt — 100 bytes
    ///   b.bin     — 50 bytes
    /// dir_sizes: root = 150, sub = 100.
    fn fixture_index() -> (tempfile::TempDir, PathBuf, DirIndex) {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("root");
        let sub = root.join("sub");
        std::fs::create_dir_all(&sub).unwrap();
        let a = sub.join("a.txt");
        let b = root.join("b.bin");
        write_file(&a, 100);
        write_file(&b, 50);
        let mut index = DirIndex::default();
        let root_str = path_to_string(&root);
        let sub_str = path_to_string(&sub);
        let (root_class, _) = classifier::classify_child(Path::new(&root_str), true, None);
        let (sub_class, _) = classifier::classify_child(Path::new(&sub_str), true, None);
        let a_entry = FileEntry {
            path: path_to_string(&a),
            name: "a.txt".into(),
            bytes: 100,
            category: sub_class.category,
            modified_ms: 1,
            ocr_text: None,
        };
        let b_entry = FileEntry {
            path: path_to_string(&b),
            name: "b.bin".into(),
            bytes: 50,
            category: root_class.category,
            modified_ms: 1,
            ocr_text: None,
        };
        index.files = vec![a_entry.clone(), b_entry.clone()];
        index.dir_sizes.insert(root_str.clone(), 150);
        index.dir_sizes.insert(sub_str.clone(), 100);
        for (key, class) in [(&root_str, &root_class), (&sub_str, &sub_class)] {
            index.dir_category.insert(key.clone(), class.category);
            index
                .dir_classification
                .insert(key.clone(), class.clone());
        }
        index.children.insert(
            root_str.clone(),
            vec![
                IndexChild {
                    name: "b.bin".into(),
                    path: b_entry.path.clone(),
                    bytes: 50,
                    is_dir: false,
                    category: root_class.category,
                },
                IndexChild {
                    name: "sub".into(),
                    path: sub_str.clone(),
                    bytes: 100,
                    is_dir: true,
                    category: sub_class.category,
                },
            ],
        );
        index.children.insert(
            sub_str,
            vec![IndexChild {
                name: "a.txt".into(),
                path: a_entry.path.clone(),
                bytes: 100,
                is_dir: false,
                category: sub_class.category,
            }],
        );
        (tmp, root, index)
    }

    fn write_file(path: &Path, bytes: usize) {
        let mut f = std::fs::File::create(path).unwrap();
        f.write_all(&vec![7u8; bytes]).unwrap();
    }

    use crate::usn::{REASON_FILE_CREATE, REASON_FILE_DELETE, REASON_RENAME_NEW_NAME, REASON_RENAME_OLD_NAME};

    #[test]
    fn delete_removes_entry_and_ancestor_totals() {
        let (_tmp, root, mut index) = fixture_index();
        let a_path = path_to_string(&root.join("sub").join("a.txt"));
        let report = apply_changes(
            &mut index,
            &root,
            &[],
            &[change(REASON_FILE_DELETE, 0x20, "a.txt", Some(a_path))],
        );
        assert_eq!(report.removed, 1);
        assert!(!report.became_stale);
        assert_eq!(index.files.len(), 1);
        assert_eq!(index.files[0].name, "b.bin");
        let root_str = path_to_string(&root);
        let sub_str = path_to_string(&root.join("sub"));
        assert_eq!(index.dir_sizes[&root_str], 50);
        assert_eq!(index.dir_sizes[&sub_str], 0);
        assert!(index.children[&sub_str].is_empty());
    }

    #[test]
    fn modify_restats_and_updates_totals() {
        let (_tmp, root, mut index) = fixture_index();
        let b_path = root.join("b.bin");
        write_file(&b_path, 80); // grew 50 → 80
        let report = apply_changes(
            &mut index,
            &root,
            &[],
            &[change(
                crate::usn::REASON_DATA_OVERWRITE,
                0x20,
                "b.bin",
                Some(path_to_string(&b_path)),
            )],
        );
        assert_eq!(report.upserted, 1);
        assert!(!report.became_stale);
        let entry = index.files.iter().find(|f| f.name == "b.bin").unwrap();
        // Allocated size on this host is at least the logical size.
        assert!(entry.bytes >= 80);
        let root_str = path_to_string(&root);
        assert_eq!(index.dir_sizes[&root_str], 100 + entry.bytes);
    }

    #[test]
    fn create_in_indexed_directory_is_added() {
        let (_tmp, root, mut index) = fixture_index();
        let c_path = root.join("sub").join("c.log");
        write_file(&c_path, 30);
        let report = apply_changes(
            &mut index,
            &root,
            &[],
            &[change(
                REASON_FILE_CREATE,
                0x20,
                "c.log",
                Some(path_to_string(&c_path)),
            )],
        );
        assert_eq!(report.upserted, 1);
        assert!(!report.became_stale);
        let entry = index.files.iter().find(|f| f.name == "c.log").unwrap();
        assert!(entry.bytes >= 30);
        // The stored path joins the parent's stored key.
        assert_eq!(entry.path, path_to_string(&c_path));
        let sub_str = path_to_string(&root.join("sub"));
        assert_eq!(index.children[&sub_str].len(), 2);
        // Ancestor totals grew by the new file's bytes.
        assert_eq!(index.dir_sizes[&sub_str], 100 + entry.bytes);
    }

    #[test]
    fn rename_is_remove_plus_upsert() {
        let (_tmp, root, mut index) = fixture_index();
        let old_path = root.join("b.bin");
        let new_path = root.join("renamed.bin");
        std::fs::rename(&old_path, &new_path).unwrap();
        let report = apply_changes(
            &mut index,
            &root,
            &[],
            &[
                change(
                    REASON_RENAME_OLD_NAME,
                    0x20,
                    "b.bin",
                    Some(path_to_string(&old_path)),
                ),
                change(
                    REASON_RENAME_NEW_NAME,
                    0x20,
                    "renamed.bin",
                    Some(path_to_string(&new_path)),
                ),
            ],
        );
        assert_eq!(report.removed, 1);
        assert_eq!(report.upserted, 1);
        assert!(!report.became_stale);
        assert!(index.files.iter().any(|f| f.name == "renamed.bin"));
        assert!(!index.files.iter().any(|f| f.name == "b.bin"));
        let root_str = path_to_string(&root);
        // Totals unchanged apart from allocated-size restatement.
        assert!(index.dir_sizes[&root_str] >= 150);
    }

    #[test]
    fn directory_change_marks_stale_and_applies_nothing() {
        let (_tmp, root, mut index) = fixture_index();
        let sub_path = path_to_string(&root.join("sub"));
        let report = apply_changes(
            &mut index,
            &root,
            &[],
            &[change(
                crate::usn::REASON_BASIC_INFO_CHANGE,
                0x10, // directory attribute
                "sub",
                Some(sub_path),
            )],
        );
        assert!(report.became_stale);
        assert_eq!(report.removed + report.upserted, 0);
        assert_eq!(index.files.len(), 2);
    }

    #[test]
    fn create_in_unknown_directory_marks_stale() {
        let (_tmp, root, mut index) = fixture_index();
        // The file really exists on disk, but its directory was
        // never part of the index — the tree changed shape.
        std::fs::create_dir_all(root.join("not-indexed")).unwrap();
        let ghost = root.join("not-indexed").join("x.txt");
        std::fs::write(&ghost, b"hello").unwrap();
        let report = apply_changes(
            &mut index,
            &root,
            &[],
            &[change(
                REASON_FILE_CREATE,
                0x20,
                "x.txt",
                Some(path_to_string(&ghost)),
            )],
        );
        assert!(report.became_stale);
        assert_eq!(report.upserted, 0);
    }

    #[test]
    fn parent_of_handles_drives_and_separators() {
        assert_eq!(parent_of(r"C:\root\sub\a.txt"), Some(r"C:\root\sub"));
        assert_eq!(parent_of(r"C:\a.txt"), Some(r"C:\"));
        assert_eq!(parent_of("/a/b/c"), Some("/a/b"));
        assert_eq!(parent_of("noslash"), None);
    }
}
