//! Relocation validation: the pure half of "move this big folder to
//! another drive and leave a junction behind". The shell supplies
//! the measured source size and the destination drive's free space,
//! performs the volume comparison and the filesystem work, and
//! re-runs this validation at action time; this module decides
//! whether the move is well-formed and safe to attempt at all.
//!
//! Conservative throughout: anything protected, anything that would
//! land inside itself, and anything without clear headroom on the
//! destination is refused with a human-readable reason.

use std::path::{Path, PathBuf};

use crate::safety;

/// Minimum free space that must remain on the destination after the
/// move, on top of the source size itself: 64 MB flat…
const MIN_HEADROOM_BYTES: u64 = 64 * 1024 * 1024;

/// …or 5% of the source size, whichever is larger. A copy-then-verify
/// move needs slack for the copy to coexist with the original, and a
/// drive filled to the last byte is a failure waiting to happen.
fn required_headroom(source_bytes: u64) -> u64 {
    MIN_HEADROOM_BYTES.max(source_bytes / 20)
}

/// Slash-normalized, lowercased rendering for prefix comparisons, so
/// Windows paths compare case-insensitively in the same way the
/// safety gate compares them.
fn norm(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/").to_lowercase()
}

/// Whether `path` equals `prefix` or lives under it, on normalized
/// spellings (component-wise where the platform parses components,
/// string-wise for foreign-spelled paths in tests).
fn is_at_or_under(path: &Path, prefix: &Path) -> bool {
    if path == prefix || path.starts_with(prefix) {
        return true;
    }
    let (p, pre) = (norm(path), norm(prefix));
    let pre = pre.trim_end_matches('/');
    p == pre || p.starts_with(&format!("{pre}/"))
}

/// Whether `path` is protected territory per the safety lists: under
/// a protected prefix, a protected filename, or a report-only
/// location.
fn is_protected(path: &Path) -> bool {
    if safety::protected_prefixes()
        .iter()
        .any(|prefix| is_at_or_under(path, prefix))
    {
        return true;
    }
    if let Some(name) = path.file_name().map(|n| n.to_string_lossy().to_lowercase()) {
        if safety::protected_filenames().contains(&name.as_str()) {
            return true;
        }
    }
    safety::is_report_only(path)
}

/// Validate a relocation and return the destination path
/// (`dest_parent/<source file name>`).
///
/// Rejects when: the source has no file name (a root) or is not
/// absolute; the source is protected/system territory; the
/// destination parent is the source itself or inside it; the
/// destination would be the source itself (same parent); the
/// destination lands in protected territory; or the destination
/// free space cannot hold the source plus the required headroom
/// (the larger of 64 MB and 5% of the source size).
pub fn validate_relocation(
    source: &Path,
    dest_parent: &Path,
    source_bytes: u64,
    dest_free_bytes: u64,
) -> Result<PathBuf, String> {
    let Some(file_name) = source.file_name() else {
        return Err(format!(
            "cannot relocate a drive or filesystem root: {}",
            source.display()
        ));
    };
    if !source.is_absolute() || !dest_parent.is_absolute() {
        return Err("source and destination must both be absolute paths".to_string());
    }
    if is_protected(source) {
        return Err(format!(
            "protected system location, never relocated: {}",
            source.display()
        ));
    }
    if is_at_or_under(dest_parent, source) {
        return Err(format!(
            "destination is the source itself or inside it: {}",
            dest_parent.display()
        ));
    }
    let dest = dest_parent.join(file_name);
    if dest == source {
        return Err(format!(
            "destination is the source itself: {}",
            dest.display()
        ));
    }
    if is_protected(&dest) {
        return Err(format!(
            "destination is inside a protected location: {}",
            dest.display()
        ));
    }
    let headroom = required_headroom(source_bytes);
    // checked, not saturating: a source so large that size + headroom
    // overflows u64 cannot fit on any real destination.
    let Some(needed) = source_bytes.checked_add(headroom) else {
        return Err(format!(
            "source size {source_bytes} plus headroom overflows any destination"
        ));
    };
    if dest_free_bytes < needed {
        return Err(format!(
            "destination has {} bytes free but the move needs {} (source {} + headroom {})",
            dest_free_bytes, needed, source_bytes, headroom
        ));
    }
    Ok(dest)
}

#[cfg(test)]
mod tests {
    use super::*;

    const GB: u64 = 1_000_000_000;

    #[test]
    fn happy_path_returns_joined_destination() {
        let dest = validate_relocation(
            Path::new("/home/user/Games/BigGame"),
            Path::new("/mnt/data"),
            40 * GB,
            100 * GB,
        )
        .unwrap();
        assert_eq!(dest, PathBuf::from("/mnt/data/BigGame"));
    }

    #[test]
    fn root_source_has_no_file_name() {
        assert!(validate_relocation(Path::new("/"), Path::new("/mnt/data"), 0, 100 * GB).is_err());
    }

    #[test]
    fn relative_paths_rejected() {
        assert!(validate_relocation(
            Path::new("Games/BigGame"),
            Path::new("/mnt/data"),
            GB,
            100 * GB
        )
        .is_err());
        assert!(validate_relocation(
            Path::new("/home/user/Games"),
            Path::new("mnt/data"),
            GB,
            100 * GB
        )
        .is_err());
    }

    #[test]
    fn protected_sources_rejected() {
        // Unix protected prefixes exercise the same list on this host;
        // the Windows spellings ride the identical code path.
        assert!(validate_relocation(
            Path::new("/etc/nginx"),
            Path::new("/mnt/data"),
            GB,
            100 * GB
        )
        .is_err());
        assert!(
            validate_relocation(Path::new("/usr/lib"), Path::new("/mnt/data"), GB, 100 * GB)
                .is_err()
        );
        // Report-only locations are protected too.
        assert!(validate_relocation(
            Path::new("/home/user/Windows.old"),
            Path::new("/mnt/data"),
            GB,
            100 * GB
        )
        .is_err());
    }

    #[test]
    fn destination_inside_source_rejected() {
        let src = Path::new("/home/user/Games");
        assert!(validate_relocation(src, Path::new("/home/user/Games"), GB, 100 * GB).is_err());
        assert!(validate_relocation(src, Path::new("/home/user/Games/Sub"), GB, 100 * GB).is_err());
        // A sibling whose name merely shares a prefix is fine.
        assert!(validate_relocation(src, Path::new("/home/user/Games2"), GB, 100 * GB).is_ok());
    }

    #[test]
    fn destination_equal_to_source_rejected() {
        // dest_parent == source's parent -> dest == source.
        assert!(validate_relocation(
            Path::new("/home/user/Games"),
            Path::new("/home/user"),
            GB,
            100 * GB
        )
        .is_err());
    }

    #[test]
    fn destination_in_protected_territory_rejected() {
        assert!(validate_relocation(
            Path::new("/home/user/Games"),
            Path::new("/etc"),
            GB,
            100 * GB
        )
        .is_err());
    }

    #[test]
    fn headroom_is_max_of_flat_and_five_percent() {
        assert_eq!(required_headroom(0), MIN_HEADROOM_BYTES);
        assert_eq!(required_headroom(GB), MIN_HEADROOM_BYTES); // 5% = 50MB < 64MiB
        assert_eq!(required_headroom(100 * GB), 5 * GB);
    }

    #[test]
    fn free_space_must_cover_source_plus_headroom() {
        let src = Path::new("/home/user/Games");
        let dst = Path::new("/mnt/data");
        // Exactly enough: 100 GB source + 5 GB headroom.
        assert!(validate_relocation(src, dst, 100 * GB, 105 * GB).is_ok());
        // One byte short.
        assert!(validate_relocation(src, dst, 100 * GB, 105 * GB - 1).is_err());
        // Saturating: a near-u64::MAX source can never fit anywhere.
        assert!(validate_relocation(src, dst, u64::MAX, u64::MAX).is_err());
    }
}
