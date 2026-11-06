//! Duplicate finder: group by size, confirm with a partial hash, then a full
//! SHA-256. Hidden folders, application data, dependency folders and package
//! caches are skipped; hard links count once; likely filesystem clones are
//! flagged because removing them frees little space.

use crate::model::*;
use rayon::prelude::*;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};

const PARTIAL_BYTES: usize = 64 * 1024;

fn skipped_dir(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower.starts_with('.')
        || matches!(
            lower.as_str(),
            "node_modules" | "target" | "appdata" | "npm-cache" | "cache" | "caches" | "vendor"
        )
}

pub fn is_clone_candidate(a: &Path, b: &Path) -> bool {
    // Cheap heuristic: same size, same modification second and different
    // inodes on filesystems that support reflinks often means a clone. The
    // flag is advisory only.
    let (Ok(ma), Ok(mb)) = (std::fs::metadata(a), std::fs::metadata(b)) else {
        return false;
    };
    if ma.len() != mb.len() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if ma.ino() == mb.ino() && ma.dev() == mb.dev() {
            return false; // hard link, not a clone
        }
        let ta = ma.mtime();
        let tb = mb.mtime();
        (ta - tb).abs() <= 1 && ma.dev() == mb.dev()
    }
    #[cfg(not(unix))]
    {
        let _ = (ma, mb);
        false
    }
}

fn is_same_file(a: &Path, b: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        match (std::fs::metadata(a), std::fs::metadata(b)) {
            (Ok(ma), Ok(mb)) => ma.dev() == mb.dev() && ma.ino() == mb.ino(),
            _ => false,
        }
    }
    #[cfg(not(unix))]
    {
        let _ = (a, b);
        false
    }
}

fn hash_partial(path: &Path) -> Option<String> {
    let mut file = std::fs::File::open(path).ok()?;
    let mut buf = vec![0u8; PARTIAL_BYTES];
    let n = file.read(&mut buf).ok()?;
    buf.truncate(n);
    // Also sample the tail so files sharing a header still diverge.
    let len = file.metadata().ok()?.len();
    if len > (PARTIAL_BYTES as u64) * 2 {
        use std::io::{Seek, SeekFrom};
        if file.seek(SeekFrom::End(-(PARTIAL_BYTES as i64))).is_ok() {
            let mut tail = vec![0u8; PARTIAL_BYTES];
            if let Ok(m) = file.read(&mut tail) {
                tail.truncate(m);
                buf.extend_from_slice(&tail);
            }
        }
    }
    let mut hasher = Sha256::new();
    hasher.update(&buf);
    Some(format!("{:x}", hasher.finalize()))
}

pub fn hash_full(path: &Path) -> Option<String> {
    let mut file = std::fs::File::open(path).ok()?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 128 * 1024];
    loop {
        let n = file.read(&mut buf).ok()?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Some(format!("{:x}", hasher.finalize()))
}

/// Collect every regular, non-empty file under `root`, skipping symlinks,
/// hidden folders and the dependency/cache folders in `skipped_dir`.
/// Subdirectories are walked in parallel (the same shape as the scanner's
/// walk); the resulting order is unspecified, exactly as directory read
/// order was in the serial version.
fn collect_files(root: &Path) -> Vec<PathBuf> {
    let Ok(read) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    let entries: Vec<PathBuf> = read.flatten().map(|e| e.path()).collect();
    entries
        .into_par_iter()
        .flat_map(|path| {
            let Ok(md) = std::fs::symlink_metadata(&path) else {
                return Vec::new();
            };
            if md.file_type().is_symlink() {
                return Vec::new();
            }
            if md.is_dir() {
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();
                if skipped_dir(&name) {
                    Vec::new()
                } else {
                    collect_files(&path)
                }
            } else if md.is_file() && md.len() > 0 {
                vec![path]
            } else {
                Vec::new()
            }
        })
        .collect()
}

pub fn find_duplicates(root: &Path) -> Vec<DuplicateGroup> {
    let files = collect_files(root);
    // Stage 1: group by exact size. Metadata reads run in parallel; the
    // grouping itself is a cheap serial fold over the results.
    let sized: Vec<(u64, PathBuf)> = files
        .into_par_iter()
        .filter_map(|f| std::fs::metadata(&f).ok().map(|md| (md.len(), f)))
        .collect();
    let mut by_size: HashMap<u64, Vec<PathBuf>> = HashMap::new();
    for (len, f) in sized {
        by_size.entry(len).or_default().push(f);
    }
    // Stage 2: partial hash of every file whose size is not unique. The
    // grouping key carries the size so partial classes never mix sizes,
    // exactly as the serial per-size-class nesting did.
    let partial_candidates: Vec<(u64, PathBuf)> = by_size
        .into_iter()
        .filter(|(_, paths)| paths.len() >= 2)
        .flat_map(|(size, paths)| paths.into_iter().map(move |p| (size, p)))
        .collect();
    let partials: Vec<(u64, String, PathBuf)> = partial_candidates
        .into_par_iter()
        .filter_map(|(size, p)| hash_partial(&p).map(|h| (size, h, p)))
        .collect();
    let mut by_partial: HashMap<(u64, String), Vec<PathBuf>> = HashMap::new();
    for (size, hash, p) in partials {
        by_partial.entry((size, hash)).or_default().push(p);
    }
    // Stage 3: full SHA-256 of every file that survived the partial hash.
    let full_candidates: Vec<(u64, PathBuf)> = by_partial
        .into_iter()
        .filter(|(_, paths)| paths.len() >= 2)
        .flat_map(|((size, _), paths)| paths.into_iter().map(move |p| (size, p)))
        .collect();
    let fulls: Vec<(u64, String, PathBuf)> = full_candidates
        .into_par_iter()
        .filter_map(|(size, p)| hash_full(&p).map(|h| (size, h, p)))
        .collect();
    let mut by_full: HashMap<(u64, String), Vec<PathBuf>> = HashMap::new();
    for (size, hash, p) in fulls {
        by_full.entry((size, hash)).or_default().push(p);
    }
    let mut groups: Vec<DuplicateGroup> = Vec::new();
    for ((size, hash), fpaths) in by_full {
        if fpaths.len() < 2 {
            continue;
        }
        let first = fpaths[0].clone();
        let dups: Vec<DuplicateFile> = fpaths
            .iter()
            .map(|p| DuplicateFile {
                path: p.to_string_lossy().to_string(),
                bytes: size,
                is_hardlink: is_same_file(&first, p) && p != &first,
                is_clone: p != &first && is_clone_candidate(&first, p),
            })
            .collect();
        groups.push(DuplicateGroup {
            size,
            hash,
            files: dups,
        });
    }
    groups.sort_by_key(|g| std::cmp::Reverse(g.wasted_bytes()));
    groups
}

/// Re-verify two files are still byte-identical just before acting on a
/// duplicate group.
pub fn verify_identical(a: &Path, b: &Path) -> bool {
    match (hash_full(a), hash_full(b)) {
        (Some(ha), Some(hb)) => ha == hb,
        _ => false,
    }
}
