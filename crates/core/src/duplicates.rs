//! Duplicate finder: group by size, confirm with a partial hash, then a full
//! SHA-256. Hidden folders, application data, dependency folders and package
//! caches are skipped; hard links count once (on Windows too, via the
//! volume-serial + file-index identity the scanner uses); likely filesystem
//! clones are flagged because removing them frees little space — the clone
//! heuristic itself is Unix-only today, so on Windows copies are told apart
//! purely by hard-link identity, which is what `wasted_bytes` relies on.

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
    #[cfg(windows)]
    {
        // Same identity the scanner dedupes on: (volume serial,
        // file index) from GetFileInformationByHandle.
        match (
            crate::scanner::file_identity(a),
            crate::scanner::file_identity(b),
        ) {
            (Some(x), Some(y)) => x == y,
            _ => false,
        }
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = (a, b);
        false
    }
}

/// Fill `buf` from `file`, looping over short reads: one `read`
/// call is not obliged to fill the buffer (pipes, network and
/// cloud-backed files all return short reads), and a partial-hash
/// window that silently shrank would group files by whatever
/// prefix happened to arrive. Returns the byte count (short only
/// at EOF or on error).
fn read_fill(file: &mut std::fs::File, buf: &mut [u8]) -> usize {
    let mut filled = 0;
    while filled < buf.len() {
        match file.read(&mut buf[filled..]) {
            Ok(0) => break,
            Ok(n) => filled += n,
            Err(_) => break,
        }
    }
    filled
}

fn hash_partial(path: &Path) -> Option<String> {
    let mut file = std::fs::File::open(path).ok()?;
    let mut buf = vec![0u8; PARTIAL_BYTES];
    let n = read_fill(&mut file, &mut buf);
    buf.truncate(n);
    // Also sample the tail so files sharing a header still diverge.
    let len = file.metadata().ok()?.len();
    if len > (PARTIAL_BYTES as u64) * 2 {
        use std::io::{Seek, SeekFrom};
        if file.seek(SeekFrom::End(-(PARTIAL_BYTES as i64))).is_ok() {
            let mut tail = vec![0u8; PARTIAL_BYTES];
            let m = read_fill(&mut file, &mut tail);
            tail.truncate(m);
            buf.extend_from_slice(&tail);
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
/// order was in the serial version. Descent stops at the scanner's
/// depth cap ([`crate::scanner::MAX_DEPTH`]): files in a directory
/// at the cap are still collected, its subdirectories are not.
pub(crate) fn collect_files(root: &Path) -> Vec<PathBuf> {
    collect_files_depth(root, 0)
}

fn collect_files_depth(root: &Path, depth: usize) -> Vec<PathBuf> {
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
                if skipped_dir(&name) || depth >= crate::scanner::MAX_DEPTH {
                    Vec::new()
                } else {
                    collect_files_depth(&path, depth + 1)
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
    for ((size, hash), mut fpaths) in by_full {
        if fpaths.len() < 2 {
            continue;
        }
        // Deterministic output: the parallel collection above makes
        // input order arbitrary, so paths sort BEFORE the group is
        // built — the keeper (`DuplicateGroup::wasted_bytes` treats
        // files[0] as the keeper) and the hard-link flags are both
        // defined relative to the first entry, so that entry must
        // be a stable one. Groups below sort by (wasted desc, hash).
        fpaths.sort();
        let first = fpaths[0].clone();
        let dups: Vec<DuplicateFile> = fpaths
            .iter()
            .map(|p| DuplicateFile {
                path: p.to_string_lossy().to_string(),
                // Display bytes are allocated (the space each copy
                // actually occupies); `size` above stays the logical
                // grouping key. See the byte-accounting rule in
                // scanner/mod.rs docs.
                bytes: crate::scanner::allocated_size_for_path(p),
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
    groups.sort_by(|a, b| {
        b.wasted_bytes()
            .cmp(&a.wasted_bytes())
            .then(a.hash.cmp(&b.hash))
    });
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

// ------------------------------------------------- look-alike images ----
//
// Exact duplicates are a solved problem above (SHA-256). This section
// finds LOOK-ALIKE images — bursts, edits, re-exports — with a perceptual
// difference hash (dHash): decode, grayscale, shrink to 9x8, and record
// the horizontal gradient as 64 bits. Two images are look-alikes when
// their hashes differ in at most SIMILARITY_THRESHOLD bits.

/// Extensions treated as images for the look-alike search.
const IMAGE_EXTENSIONS: [&str; 6] = ["jpg", "jpeg", "png", "gif", "bmp", "webp"];
/// Images larger than this are not decoded for hashing.
const MAX_IMAGE_BYTES: u64 = 25 * 1024 * 1024;
/// Maximum Hamming distance between two dHashes in one look-alike group.
pub const SIMILARITY_THRESHOLD: u32 = 5;

/// Perceptual difference hash of one image: 64 bits of horizontal
/// gradient over a 9x8 grayscale shrink. `None` for files over 25 MB,
/// undecodable files, and non-images.
pub fn dhash(path: &Path) -> Option<u64> {
    let len = std::fs::metadata(path).ok()?.len();
    if len == 0 || len > MAX_IMAGE_BYTES {
        return None;
    }
    let img = image::open(path).ok()?;
    let small = img
        .resize_exact(9, 8, image::imageops::FilterType::Triangle)
        .to_luma8();
    let mut hash: u64 = 0;
    for y in 0..8u32 {
        for x in 0..8u32 {
            let left = small.get_pixel(x, y)[0];
            let right = small.get_pixel(x + 1, y)[0];
            hash = (hash << 1) | u64::from(left > right);
        }
    }
    Some(hash)
}

/// Hamming distance between two dHashes.
pub fn dhash_distance(a: u64, b: u64) -> u32 {
    (a ^ b).count_ones()
}

/// One image in a look-alike group.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SimilarImage {
    pub path: String,
    /// Allocated bytes on disk — the user-facing figure (see the
    /// byte-accounting rule in scanner/mod.rs docs).
    pub bytes: u64,
    pub width: u32,
    pub height: u32,
}

/// A set of look-alike images. The keeper is the largest file by
/// LOGICAL size (the pre-existing keeper rule — ordering deliberately
/// does not change with the display accounting); `wasted_bytes` is
/// allocated-based: the allocated bytes of every non-keeper copy.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SimilarGroup {
    /// Files, largest first by logical size (the first entry is the
    /// keeper). Displayed `bytes` are allocated, so adjacent rows can
    /// occasionally appear out of order by a block or two.
    pub files: Vec<SimilarImage>,
    pub wasted_bytes: u64,
}

fn is_image_path(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| IMAGE_EXTENSIONS.contains(&e.to_lowercase().as_str()))
        .unwrap_or(false)
}

/// Find groups of look-alike images under `root`. Grouping is by
/// connected components over the "distance ≤ threshold" relation, so a
/// chain of near-copies lands in one group. Exact duplicates (distance 0)
/// are included too — the SHA-256 finder remains the authority for
/// byte-identical files, but showing them here as well keeps one image
/// from appearing unrelated across the two views. Comparison is
/// O(n²) over candidate images, which is fine for photo-library sizes
/// but noted for very large trees.
pub fn find_similar_images(root: &Path) -> Vec<SimilarGroup> {
    let candidates: Vec<PathBuf> = collect_files(root)
        .into_iter()
        .filter(|p| is_image_path(p))
        .collect();
    // Each candidate carries its LOGICAL length alongside the display
    // record: the keeper rule (largest first) is defined on logical
    // size and must not change, while `SimilarImage::bytes` is the
    // allocated display figure.
    let hashed: Vec<(SimilarImage, u64, u64)> = candidates
        .into_par_iter()
        .filter_map(|path| {
            let hash = dhash(&path)?;
            let md = std::fs::metadata(&path).ok()?;
            let (width, height) = match crate::media_meta::media_info(&path) {
                Some(info) => match (info.width, info.height) {
                    (Some(w), Some(h)) => (w, h),
                    _ => image::image_dimensions(&path).ok()?,
                },
                None => image::image_dimensions(&path).ok()?,
            };
            Some((
                SimilarImage {
                    path: path.to_string_lossy().to_string(),
                    bytes: crate::scanner::allocated_size_for_path(&path),
                    width,
                    height,
                },
                md.len(),
                hash,
            ))
        })
        .collect();

    // Union-find over pairwise distances.
    let n = hashed.len();
    let mut parent: Vec<usize> = (0..n).collect();
    fn find(parent: &mut [usize], mut x: usize) -> usize {
        while parent[x] != x {
            parent[x] = parent[parent[x]];
            x = parent[x];
        }
        x
    }
    for i in 0..n {
        for j in i + 1..n {
            if dhash_distance(hashed[i].2, hashed[j].2) <= SIMILARITY_THRESHOLD {
                let (a, b) = (find(&mut parent, i), find(&mut parent, j));
                if a != b {
                    parent[a] = b;
                }
            }
        }
    }
    let mut components: HashMap<usize, Vec<(SimilarImage, u64)>> = HashMap::new();
    for (i, (img, logical, _)) in hashed.into_iter().enumerate() {
        let root_idx = find(&mut parent, i);
        components.entry(root_idx).or_default().push((img, logical));
    }
    let mut groups: Vec<SimilarGroup> = components
        .into_values()
        .filter(|files| files.len() >= 2)
        .map(|mut files| {
            // Keeper rule unchanged: largest LOGICAL file first.
            // Path breaks size ties so the keeper — and the whole
            // rendering — is deterministic across runs.
            files.sort_by(|a, b| {
                b.1.cmp(&a.1)
                    .then(a.0.path.cmp(&b.0.path))
            });
            let files: Vec<SimilarImage> = files.into_iter().map(|(img, _)| img).collect();
            // Wasted is allocated-based: every non-keeper copy's
            // allocated bytes (files[0] is the keeper).
            let wasted_bytes: u64 = files.iter().skip(1).map(|f| f.bytes).sum();
            SimilarGroup {
                wasted_bytes,
                files,
            }
        })
        .collect();
    groups.sort_by(|a, b| {
        b.wasted_bytes.cmp(&a.wasted_bytes).then(
            a.files
                .first()
                .map(|f| f.path.as_str())
                .cmp(&b.files.first().map(|f| f.path.as_str())),
        )
    });
    groups
}

#[cfg(test)]
mod similar_tests {
    use super::*;
    use image::{ImageBuffer, Rgba};

    /// 36x32 image whose rows alternate a strong left-to-right gradient
    /// direction, so the dHash is well-defined and stable.
    fn gradient_image(flip: bool) -> ImageBuffer<Rgba<u8>, Vec<u8>> {
        ImageBuffer::from_fn(36, 32, |x, y| {
            let rising = (y / 4) % 2 == 0;
            let t = if rising { x } else { 35 - x };
            let mut v = (t * 7) as u8;
            if flip {
                v = 255 - v;
            }
            Rgba([v, v, v, 255])
        })
    }

    #[test]
    fn identical_near_and_different_images() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        gradient_image(false).save(root.join("a.png")).unwrap();
        gradient_image(false).save(root.join("b.png")).unwrap();
        // Near copy: one pixel changed in a flat-ish region.
        let mut near = gradient_image(false);
        near.put_pixel(10, 9, Rgba([200, 200, 200, 255]));
        near.save(root.join("c.png")).unwrap();
        gradient_image(true)
            .save(root.join("different.png"))
            .unwrap();
        // A text file is not an image candidate even with a big size.
        std::fs::write(root.join("notes.txt"), vec![7u8; 4096]).unwrap();

        let ha = dhash(&root.join("a.png")).unwrap();
        let hb = dhash(&root.join("b.png")).unwrap();
        assert_eq!(dhash_distance(ha, hb), 0);
        let hc = dhash(&root.join("c.png")).unwrap();
        assert!(
            dhash_distance(ha, hc) <= SIMILARITY_THRESHOLD,
            "near copy distance {} should group",
            dhash_distance(ha, hc)
        );
        let hd = dhash(&root.join("different.png")).unwrap();
        assert!(dhash_distance(ha, hd) > SIMILARITY_THRESHOLD);

        let groups = find_similar_images(root);
        assert_eq!(groups.len(), 1);
        let group = &groups[0];
        let names: Vec<&str> = group
            .files
            .iter()
            .map(|f| f.path.rsplit('/').next().unwrap())
            .collect();
        assert_eq!(group.files.len(), 3);
        assert!(names.contains(&"a.png") && names.contains(&"b.png") && names.contains(&"c.png"));
        assert!(group.wasted_bytes > 0);
        assert_eq!((group.files[0].width, group.files[0].height), (36, 32));
    }

    #[test]
    fn dhash_rejects_non_images_and_missing_files() {
        let dir = tempfile::tempdir().unwrap();
        let text = dir.path().join("x.png");
        std::fs::write(&text, b"not really a png").unwrap();
        assert_eq!(dhash(&text), None);
        assert_eq!(dhash(&dir.path().join("missing.png")), None);
    }
}
