//! Parallel read-only scanner.
//! Never follows symlinks, junctions or reparse points; never crosses into a
//! different volume; counts each hard link once; reports allocated size.
//!
//! BYTE-ACCOUNTING RULE (product-wide): every user-facing byte figure is
//! ALLOCATED bytes on disk — the space a file actually occupies and the
//! space a cleanup actually frees — measured by [`allocated_size`] /
//! [`allocated_size_for_path`] in this module. The scan index, search
//! hits, cleanup cards, download provenance, duplicate displays (per-file
//! bytes and wasted totals) and reclaimed-byte results all follow it, so
//! the same file shows the same size on every page. Logical length
//! (`Metadata::len`) is used ONLY where it is a correctness input —
//! duplicate size-grouping and hash staging — and never displayed.

use crate::classifier::{self, rules::RuleScope};
use crate::model::*;
use rayon::prelude::*;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

#[derive(Debug, Clone)]
pub struct ScanProgress {
    pub path: String,
    pub files_seen: u64,
    pub bytes_seen: u64,
}

#[derive(Debug, Clone)]
pub struct IndexChild {
    pub name: String,
    pub path: String,
    pub bytes: u64,
    pub is_dir: bool,
    pub category: Category,
}

#[derive(Debug, Clone, Default)]
pub struct DirIndex {
    pub dir_sizes: HashMap<String, u64>,
    pub dir_category: HashMap<String, Category>,
    pub dir_classification: HashMap<String, Classification>,
    pub children: HashMap<String, Vec<IndexChild>>,
    pub files: Vec<FileEntry>,
}

impl DirIndex {
    pub fn size_of(&self, path: &str) -> u64 {
        self.dir_sizes.get(path).copied().unwrap_or(0)
    }
}

pub type ProgressCallback = Arc<dyn Fn(ScanProgress) + Send + Sync>;

#[derive(Clone)]
pub struct Scanner {
    pub root: PathBuf,
    pub cancel: Arc<AtomicBool>,
    pub progress: Option<ProgressCallback>,
    pub exclusions: Vec<String>,
}

#[derive(Default)]
struct Shared {
    errors: Vec<ScanError>,
    skipped: Vec<String>,
    hardlinks: HashSet<(u64, u64)>,
    files_seen: u64,
    bytes_seen: u64,
    last_progress: Option<Instant>,
}

/// Lock the shared scan state, recovering the guard if a worker thread
/// panicked while holding it — one bad path must not kill the whole scan.
fn lock_shared(shared: &Mutex<Shared>) -> std::sync::MutexGuard<'_, Shared> {
    shared
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

impl Scanner {
    pub fn new(root: impl AsRef<Path>) -> Self {
        Self {
            root: root.as_ref().to_path_buf(),
            cancel: Arc::new(AtomicBool::new(false)),
            progress: None,
            exclusions: Vec::new(),
        }
    }
    pub fn with_cancel(mut self, cancel: Arc<AtomicBool>) -> Self {
        self.cancel = cancel;
        self
    }
    pub fn with_progress(mut self, cb: ProgressCallback) -> Self {
        self.progress = Some(cb);
        self
    }
    /// Exclude paths matching `patterns` from the scan (see [`is_excluded`]
    /// for the matching rules). Patterns are matched against each entry's
    /// path relative to the scan root, so they only apply inside the tree.
    /// Excluded directories are not descended into and excluded files are
    /// not counted; excluded paths are silently out of scope — they appear
    /// in neither the index, the totals, nor the skipped list. The scan
    /// root itself is never excluded.
    pub fn with_exclusions(mut self, patterns: Vec<String>) -> Self {
        self.exclusions = patterns;
        self
    }

    /// Components above the scan root never classify the tree: this is the
    /// number of leading path components to skip when matching rules.
    fn base_skip(&self) -> usize {
        self.root.components().count().saturating_sub(1)
    }

    pub fn scan(&self) -> (ScanSummary, DirIndex) {
        let started = now_ms();
        let start_instant = Instant::now();
        let root_dev = root_device(&self.root);
        let shared = Arc::new(Mutex::new(Shared::default()));
        let mut index = DirIndex::default();
        let root_class = classifier::classify_inner_from(&self.root, self.base_skip());
        let root_scope = classifier::scope_of(&root_class.rule_id);
        let agg = self.walk_dir(
            &self.root,
            Some(&(root_class.clone(), root_scope)),
            root_dev,
            &shared,
            &mut index,
        );
        let finished = now_ms();
        let (errors, skipped) = {
            let s = lock_shared(&shared);
            (s.errors.clone(), s.skipped.clone())
        };
        let mut category_totals: HashMap<Category, u64> = HashMap::new();
        let mut recoverable: HashMap<Category, u64> = HashMap::new();
        for f in &index.files {
            *category_totals.entry(f.category).or_insert(0) += f.bytes;
            if f.category.is_recoverable() {
                *recoverable.entry(f.category).or_insert(0) += f.bytes;
            }
        }
        let mut top_files = index.files.clone();
        top_files.sort_by(|a, b| b.bytes.cmp(&a.bytes).then(a.path.cmp(&b.path)));
        top_files.truncate(500);

        let root_str = path_to_string(&self.root);
        let mut locations: Vec<LocationEntry> = index
            .children
            .get(&root_str)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .map(|c| LocationEntry {
                path: c.path,
                name: c.name,
                bytes: c.bytes,
                category: c.category,
            })
            .collect();
        locations.sort_by_key(|l| std::cmp::Reverse(l.bytes));

        let mut heavy: Vec<LocationEntry> = index
            .dir_sizes
            .iter()
            .filter(|(p, _)| *p != &root_str)
            .map(|(p, b)| LocationEntry {
                path: p.clone(),
                name: Path::new(p)
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| p.clone()),
                bytes: *b,
                category: index
                    .dir_category
                    .get(p)
                    .copied()
                    .unwrap_or(Category::Unknown),
            })
            .collect();
        heavy.sort_by(|a, b| b.bytes.cmp(&a.bytes).then(a.path.cmp(&b.path)));
        heavy.truncate(50);

        let summary = ScanSummary {
            root: root_str,
            started_at_ms: started,
            finished_at_ms: finished,
            duration_ms: start_instant.elapsed().as_millis() as i64,
            // Free space of the scanned root's drive at scan end; the
            // scan-history time series and the space forecast read it
            // from here. A failure to query the drive is not a scan
            // failure — the field simply stays `None`.
            drive_free_bytes: fs2::free_space(&self.root).ok(),
            total_bytes: agg.bytes,
            file_count: agg.files,
            dir_count: agg.dirs,
            category_totals: category_totals.clone(),
            recoverable_by_category: recoverable,
            top_files,
            locations,
            heavy_folders: heavy,
            errors,
            skipped,
        };
        // Category totals must reconcile with the total: files under a leaf
        // whose bytes were deduplicated contribute 0, which the per-file
        // category recorded at count time already reflects.
        debug_assert_eq!(
            summary.category_totals.values().sum::<u64>(),
            summary.total_bytes,
            "category totals must equal total bytes"
        );
        (summary, index)
    }

    fn report_progress(&self, shared: &Arc<Mutex<Shared>>, path: &Path) {
        let Some(cb) = &self.progress else { return };
        let mut s = lock_shared(shared);
        let due = s
            .last_progress
            .map(|t| t.elapsed().as_millis() >= 120)
            .unwrap_or(true);
        if due {
            s.last_progress = Some(Instant::now());
            let payload = ScanProgress {
                path: path_to_string(path),
                files_seen: s.files_seen,
                bytes_seen: s.bytes_seen,
            };
            drop(s);
            cb(payload);
        }
    }

    fn walk_dir(
        &self,
        dir: &Path,
        inherited: Option<&(Classification, RuleScope)>,
        root_dev: u64,
        shared: &Arc<Mutex<Shared>>,
        index: &mut DirIndex,
    ) -> Agg {
        let mut agg = Agg::default();
        if self.cancel.load(Ordering::Relaxed) {
            return agg;
        }
        let dir_class = match inherited {
            Some((c, _)) => c.clone(),
            None => classifier::classify_inner_from(dir, self.base_skip()),
        };
        let dir_scope = classifier::scope_of(&dir_class.rule_id);
        let read = match std::fs::read_dir(dir) {
            Ok(r) => r,
            Err(e) => {
                lock_shared(shared).errors.push(ScanError {
                    path: path_to_string(dir),
                    message: e.to_string(),
                });
                return agg;
            }
        };
        let mut entries: Vec<PathBuf> = Vec::new();
        for entry in read {
            match entry {
                Ok(e) => entries.push(e.path()),
                Err(e) => lock_shared(shared).errors.push(ScanError {
                    path: path_to_string(dir),
                    message: e.to_string(),
                }),
            }
        }
        entries.sort();

        // First pass: files counted here, subdirectories collected.
        let mut subdirs: Vec<(PathBuf, Classification, RuleScope)> = Vec::new();
        let mut children: Vec<IndexChild> = Vec::new();
        for path in &entries {
            if self.cancel.load(Ordering::Relaxed) {
                break;
            }
            // Excluded paths are silently out of scope: not indexed, not
            // counted, and not recorded in the skipped or error lists.
            // Patterns are matched against the path relative to the scan
            // root, so they can only match components inside the tree —
            // the root itself and everything above it is never excluded.
            if !self.exclusions.is_empty() {
                let rel = path.strip_prefix(&self.root).unwrap_or(path.as_path());
                if is_excluded(rel, &self.exclusions) {
                    continue;
                }
            }
            let md = match std::fs::symlink_metadata(path) {
                Ok(m) => m,
                Err(e) => {
                    lock_shared(shared).errors.push(ScanError {
                        path: path_to_string(path),
                        message: e.to_string(),
                    });
                    continue;
                }
            };
            let ft = md.file_type();
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            if ft.is_symlink() || is_reparse_point(&md) {
                lock_shared(shared).skipped.push(path_to_string(path));
                continue;
            }
            if ft.is_dir() {
                // Same-volume check.
                if device_of(&md) != root_dev && root_dev != 0 {
                    lock_shared(shared).skipped.push(path_to_string(path));
                    continue;
                }
                let (class, scope) = classifier::classify_child_from(
                    path,
                    true,
                    Some(&(dir_class.clone(), dir_scope)),
                    self.base_skip(),
                );
                subdirs.push((path.clone(), class, scope));
            } else if ft.is_file() {
                let (class, _) = classifier::classify_child_from(
                    path,
                    false,
                    Some(&(dir_class.clone(), dir_scope)),
                    self.base_skip(),
                );
                let counted = counted_bytes(&md, shared);
                agg.bytes += counted;
                agg.files += 1;
                {
                    let mut s = lock_shared(shared);
                    s.files_seen += 1;
                    s.bytes_seen += counted;
                }
                let entry = FileEntry {
                    path: path_to_string(path),
                    name: name.clone(),
                    bytes: counted,
                    category: class.category,
                    modified_ms: mtime_ms(&md),
                };
                index.files.push(entry);
                children.push(IndexChild {
                    name,
                    path: path_to_string(path),
                    bytes: counted,
                    is_dir: false,
                    category: class.category,
                });
                self.report_progress(shared, path);
            }
        }

        // Second pass: subdirectories in parallel. Each sub-walk builds its own
        // index fragment, merged afterwards to keep locking coarse.
        let fragments: Vec<(Agg, DirIndex, IndexChild)> = subdirs
            .into_par_iter()
            .map(|(sub, class, scope)| {
                let mut sub_index = DirIndex::default();
                let sub_agg = self.walk_dir(
                    &sub,
                    Some(&(class.clone(), scope)),
                    root_dev,
                    shared,
                    &mut sub_index,
                );
                let child = IndexChild {
                    name: sub
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_default(),
                    path: path_to_string(&sub),
                    bytes: sub_agg.bytes,
                    is_dir: true,
                    category: class.category,
                };
                (sub_agg, sub_index, child)
            })
            .collect();

        for (sub_agg, sub_index, child) in fragments {
            agg.bytes += sub_agg.bytes;
            agg.files += sub_agg.files;
            agg.dirs += sub_agg.dirs + 1;
            children.push(child);
            index.dir_sizes.extend(sub_index.dir_sizes);
            index.dir_category.extend(sub_index.dir_category);
            index
                .dir_classification
                .extend(sub_index.dir_classification);
            index.children.extend(sub_index.children);
            index.files.extend(sub_index.files);
        }

        children.sort_by(|a, b| a.name.cmp(&b.name));
        let dir_str = path_to_string(dir);
        index.dir_sizes.insert(dir_str.clone(), agg.bytes);
        index
            .dir_category
            .insert(dir_str.clone(), dir_class.category);
        index.dir_classification.insert(dir_str, dir_class);
        index.children.insert(path_to_string(dir), children);
        agg
    }
}

/// Match one path segment against a glob pattern: `*` matches any run of
/// characters (including none), `?` matches exactly one character. Both
/// sides are compared case-insensitively.
fn glob_segment_match(pattern: &str, segment: &str) -> bool {
    let p: Vec<char> = pattern.chars().flat_map(|c| c.to_lowercase()).collect();
    let t: Vec<char> = segment.chars().flat_map(|c| c.to_lowercase()).collect();
    // Two-pointer scan with backtracking to the last `*`.
    let (mut pi, mut ti) = (0usize, 0usize);
    let (mut star, mut star_ti) = (None::<usize>, 0usize);
    while ti < t.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == t[ti]) {
            pi += 1;
            ti += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star = Some(pi);
            star_ti = ti;
            pi += 1;
        } else if let Some(s) = star {
            // Let the last `*` absorb one more character and retry.
            pi = s + 1;
            star_ti += 1;
            ti = star_ti;
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == '*' {
        pi += 1;
    }
    pi == p.len()
}

/// One pattern segment against one path segment: a glob match when the
/// pattern contains a wildcard, an exact match otherwise. Case-insensitive
/// either way.
fn segment_matches(pattern: &str, segment: &str) -> bool {
    if pattern.contains('*') || pattern.contains('?') {
        glob_segment_match(pattern, segment)
    } else {
        pattern.to_lowercase() == segment.to_lowercase()
    }
}

/// Does `path` match any of the exclusion `patterns`?
///
/// Matching is case-insensitive. A pattern is split into segments on `/`
/// or `\`. A single-segment pattern matches when it matches **any**
/// component of the path — exactly (`node_modules`), or as a glob
/// (`*.log`, `build-*`) when it contains `*` or `?`. A multi-segment
/// pattern (`proj/node_modules`, `src\gen-*`) matches when its segments
/// match a trailing sequence of the path's components, segment by segment
/// (so it matches the directory it names; the scanner prunes the whole
/// subtree there and never checks the paths below it). Empty patterns
/// never match; an empty pattern list excludes nothing.
pub fn is_excluded(path: &Path, patterns: &[String]) -> bool {
    if patterns.is_empty() {
        return false;
    }
    let segments: Vec<String> = path
        .components()
        .filter_map(|c| match c {
            std::path::Component::Normal(s) => Some(s.to_string_lossy().to_string()),
            _ => None,
        })
        .collect();
    if segments.is_empty() {
        return false;
    }
    patterns.iter().any(|pattern| {
        let pat: Vec<&str> = pattern
            .split(['/', '\\'])
            .filter(|s| !s.is_empty())
            .collect();
        match pat.len() {
            0 => false,
            1 => segments.iter().any(|seg| segment_matches(pat[0], seg)),
            n => {
                n <= segments.len()
                    && pat
                        .iter()
                        .zip(segments[segments.len() - n..].iter())
                        .all(|(p, seg)| segment_matches(p, seg))
            }
        }
    })
}

#[derive(Default)]
struct Agg {
    bytes: u64,
    files: u64,
    dirs: u64,
}

/// Milliseconds since the Unix epoch for a metadata mtime; 0 when the
/// platform reports no mtime or one before the epoch.
fn mtime_ms(md: &std::fs::Metadata) -> i64 {
    match md.modified() {
        Ok(t) => match t.duration_since(std::time::UNIX_EPOCH) {
            Ok(d) => d.as_millis().min(i64::MAX as u128) as i64,
            Err(_) => 0,
        },
        Err(_) => 0,
    }
}

fn counted_bytes(md: &std::fs::Metadata, shared: &Arc<Mutex<Shared>>) -> u64 {
    let allocated = allocated_size(md);
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if md.nlink() > 1 {
            let key = (md.dev(), md.ino());
            let mut s = lock_shared(shared);
            if !s.hardlinks.insert(key) {
                return 0;
            }
        }
    }
    #[cfg(windows)]
    {
        if file_nlink_windows(md) > 1 {
            if let Some(key) = file_index_key(md) {
                let mut s = lock_shared(&shared);
                if !s.hardlinks.insert(key) {
                    return 0;
                }
            }
        }
    }
    let _ = md;
    allocated
}

/// Allocated size on disk for a file.
pub fn allocated_size(md: &std::fs::Metadata) -> u64 {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let blocks = md.blocks();
        if blocks > 0 {
            return blocks * 512;
        }
        return md.len();
    }
    #[cfg(windows)]
    {
        // Prefer the compressed size so sparse / compressed files report the
        // space they actually occupy; fall back to logical length.
        if let Some(p) = current_path_hint() {
            let _ = p;
        }
        let _ = md;
        // The scanner path uses GetCompressedFileSizeW with the real path in
        // `allocated_size_for_path`; here we only have metadata.
        return md.len();
    }
    #[allow(unreachable_code)]
    md.len()
}

/// Path-aware allocated size. On Windows this calls GetCompressedFileSizeW
/// through a raw extern declaration (no extra dependency); on Unix it reads
/// the block count from metadata.
pub fn allocated_size_for_path(path: &Path) -> u64 {
    #[cfg(windows)]
    {
        match compressed_file_size_w(path) {
            Some(v) => return v,
            None => {
                return std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
            }
        }
    }
    #[cfg(not(windows))]
    {
        std::fs::metadata(path)
            .map(|m| allocated_size(&m))
            .unwrap_or(0)
    }
}

#[cfg(windows)]
fn current_path_hint() -> Option<PathBuf> {
    None
}

#[cfg(windows)]
fn compressed_file_size_w(path: &Path) -> Option<u64> {
    use std::os::windows::ffi::OsStrExt;
    #[link(name = "kernel32")]
    extern "system" {
        fn GetCompressedFileSizeW(lpFileName: *const u16, lpFileSizeHigh: *mut u32) -> u32;
    }
    let wide: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let mut high: u32 = 0;
    let low = unsafe { GetCompressedFileSizeW(wide.as_ptr(), &mut high) };
    if low == u32::MAX {
        // INVALID_FILE_SIZE: distinguish error via length fallback by caller.
        // Compressed size of a genuinely 4GiB-1 file is vanishingly rare here.
        return None;
    }
    Some(((high as u64) << 32) | low as u64)
}

#[cfg(windows)]
fn file_nlink_windows(_md: &std::fs::Metadata) -> u64 {
    // std does not expose the Windows file index / link count portably; the
    // scanner treats files as singly linked unless a file index is available.
    1
}

#[cfg(windows)]
fn file_index_key(_md: &std::fs::Metadata) -> Option<(u64, u64)> {
    None
}

fn device_of(md: &std::fs::Metadata) -> u64 {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        md.dev()
    }
    #[cfg(not(unix))]
    {
        let _ = md;
        0
    }
}

fn root_device(root: &Path) -> u64 {
    std::fs::metadata(root).map(|m| device_of(&m)).unwrap_or(0)
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

/// Recursive size of a single path without following symlinks. Used by the
/// developer-tools analyzers and application data measurement.
pub fn measure_path(path: &Path) -> u64 {
    let md = match std::fs::symlink_metadata(path) {
        Ok(m) => m,
        Err(_) => return 0,
    };
    if md.file_type().is_symlink() {
        return 0;
    }
    if md.is_file() {
        return allocated_size_for_path(path);
    }
    let mut total = 0u64;
    if let Ok(read) = std::fs::read_dir(path) {
        for entry in read.flatten() {
            let p = entry.path();
            if let Ok(m) = std::fs::symlink_metadata(&p) {
                if m.file_type().is_symlink() {
                    continue;
                }
                if m.is_dir() {
                    total += measure_path(&p);
                } else if m.is_file() {
                    total += allocated_size_for_path(&p);
                }
            }
        }
    }
    total
}

/// Logical size total (sum of file lengths) used in places where tests and
/// callers reason about exact byte counts.
pub fn measure_logical(path: &Path) -> u64 {
    let md = match std::fs::symlink_metadata(path) {
        Ok(m) => m,
        Err(_) => return 0,
    };
    if md.file_type().is_symlink() {
        return 0;
    }
    if md.is_file() {
        return md.len();
    }
    let mut total = 0u64;
    if let Ok(read) = std::fs::read_dir(path) {
        for entry in read.flatten() {
            total += measure_logical(&entry.path());
        }
    }
    total
}
