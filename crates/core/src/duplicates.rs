//! Duplicate finder: group by size, confirm with a partial hash, then a full
//! SHA-256. Hidden folders, application data, dependency folders and package
//! caches are skipped; hard links count once; likely filesystem clones are
//! flagged because removing them frees little space.

use crate::model::*;
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
        return (ta - tb).abs() <= 1 && ma.dev() == mb.dev();
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

fn collect_files(root: &Path, out: &mut Vec<PathBuf>) {
    let Ok(read) = std::fs::read_dir(root) else {
        return;
    };
    for entry in read.flatten() {
        let path = entry.path();
        let Ok(md) = std::fs::symlink_metadata(&path) else {
            continue;
        };
        if md.file_type().is_symlink() {
            continue;
        }
        if md.is_dir() {
            let name = entry.file_name().to_string_lossy().to_string();
            if skipped_dir(&name) {
                continue;
            }
            collect_files(&path, out);
        } else if md.is_file() && md.len() > 0 {
            out.push(path);
        }
    }
}

pub fn find_duplicates(root: &Path) -> Vec<DuplicateGroup> {
    let mut files = Vec::new();
    collect_files(root, &mut files);
    // Stage 1: group by exact size.
    let mut by_size: HashMap<u64, Vec<PathBuf>> = HashMap::new();
    for f in files {
        if let Ok(md) = std::fs::metadata(&f) {
            by_size.entry(md.len()).or_default().push(f);
        }
    }
    let mut groups: Vec<DuplicateGroup> = Vec::new();
    for (size, paths) in by_size {
        if paths.len() < 2 {
            continue;
        }
        // Stage 2: partial hash.
        let mut by_partial: HashMap<String, Vec<PathBuf>> = HashMap::new();
        for p in paths {
            if let Some(h) = hash_partial(&p) {
                by_partial.entry(h).or_default().push(p);
            }
        }
        for (_, ppaths) in by_partial {
            if ppaths.len() < 2 {
                continue;
            }
            // Stage 3: full SHA-256.
            let mut by_full: HashMap<String, Vec<PathBuf>> = HashMap::new();
            for p in ppaths {
                if let Some(h) = hash_full(&p) {
                    by_full.entry(h).or_default().push(p);
                }
            }
            for (hash, fpaths) in by_full {
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
        }
    }
    groups.sort_by(|a, b| b.wasted_bytes().cmp(&a.wasted_bytes()));
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
