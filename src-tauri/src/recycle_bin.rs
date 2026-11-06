//! Recycle Bin contents, read from the on-disk `$Recycle.Bin` metadata.
//!
//! Windows stores every recycled item twice per user SID folder under
//! `<drive>\$Recycle.Bin\<SID>\`: a payload file `$R<random><ext>` and a
//! metadata file `$I<random><ext>` describing it. The `$I` format is:
//!
//! ```text
//! offset 0   u64  version (1 = fixed 260-char path, 2 = length-prefixed)
//! offset 8   u64  original size in bytes
//! offset 16  u64  deletion time (Windows FILETIME, 100 ns ticks)
//! offset 24  version 1: UTF-16LE path, NUL-terminated
//!            version 2: u32 char count, then UTF-16LE path
//! ```
//!
//! Listing is read-only. Emptying removes exactly the payload + metadata
//! pairs the UI was shown, one by one — items that arrive later are
//! never touched, and there is no whole-bin API call. The item `id` is
//! the payload's absolute path; before anything is removed,
//! `validate_item_id` re-checks that the id really points at a `$R…`
//! payload inside one of the machine's real per-drive `$Recycle.Bin`
//! roots with its `$I…` sibling present, so the command cannot be
//! aimed elsewhere — a folder merely *named* `$Recycle.Bin` somewhere
//! else on disk does not qualify.
//!
//! Restoring one item lives here too, because it speaks the same
//! on-disk language: move the `$R…` payload back to the path its
//! `$I…` metadata records, then remove the metadata. A restore never
//! overwrites and never lands inside a protected location.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// One Recycle Bin item, as shown by the Empty Recycle Bin dialog.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecycleBinItem {
    /// Absolute path of the `$R…` payload file (unique per item).
    pub id: String,
    /// File name of the item at its original location.
    pub name: String,
    /// Absolute path the item was deleted from.
    pub original_path: String,
    /// Original size in bytes.
    pub bytes: u64,
    /// Deletion time, Unix epoch milliseconds.
    pub deleted_at_ms: i64,
}

/// Every real `$Recycle.Bin` root on this machine: `<Drive>:\$Recycle.Bin`
/// for each drive letter whose bin folder exists. (A `$Recycle.Bin`
/// directory only exists on a volume Windows has actually recycled
/// from, so existence is the practical fixed-drive test.) Validation
/// accepts payload ids only under one of these roots.
#[cfg(windows)]
pub fn recycle_bin_roots() -> Vec<PathBuf> {
    ('A'..='Z')
        .map(|c| PathBuf::from(format!("{c}:\\$Recycle.Bin")))
        .filter(|root| root.is_dir())
        .collect()
}

#[cfg(not(windows))]
pub fn recycle_bin_roots() -> Vec<PathBuf> {
    Vec::new()
}

/// List every recycled item on every drive (all user SID folders;
/// folders the process cannot read are skipped).
#[cfg(windows)]
pub fn list_items() -> Vec<RecycleBinItem> {
    let mut items = Vec::new();
    for root in recycle_bin_roots() {
        list_items_under(&root, &mut items);
    }
    items.sort_by_key(|a| std::cmp::Reverse(a.deleted_at_ms));
    items
}

/// Append the items of one `$Recycle.Bin` root to `items`.
#[cfg(windows)]
fn list_items_under(root: &Path, items: &mut Vec<RecycleBinItem>) {
    let Ok(sid_dirs) = std::fs::read_dir(root) else {
        return;
    };
    for sid in sid_dirs.flatten() {
        let sid_path = sid.path();
        if !sid_path.is_dir() {
            continue;
        }
        let Ok(entries) = std::fs::read_dir(&sid_path) else {
            continue;
        };
        for entry in entries.flatten() {
            let info_path = entry.path();
            let Some(file_name) = info_path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
            else {
                continue;
            };
            if !file_name.to_uppercase().starts_with("$I") {
                continue;
            }
            let payload_name = format!("$R{}", &file_name[2..]);
            let payload_path = sid_path.join(&payload_name);
            if !payload_path.exists() {
                continue;
            }
            let Ok(data) = std::fs::read(&info_path) else {
                continue;
            };
            if let Some(item) = parse_info_file(&data, &payload_path) {
                items.push(item);
            }
        }
    }
}

#[cfg(not(windows))]
pub fn list_items() -> Vec<RecycleBinItem> {
    Vec::new()
}

/// Parse one `$I…` metadata file.
fn parse_info_file(data: &[u8], payload_path: &std::path::Path) -> Option<RecycleBinItem> {
    if data.len() < 24 {
        return None;
    }
    let version = u64::from_le_bytes(data[0..8].try_into().ok()?);
    let size = u64::from_le_bytes(data[8..16].try_into().ok()?);
    let filetime = u64::from_le_bytes(data[16..24].try_into().ok()?);
    // FILETIME counts 100 ns intervals since 1601-01-01.
    let deleted_at_ms = (filetime / 10_000) as i64 - 11_644_473_600_000;

    let original_path = if version >= 2 {
        if data.len() < 28 {
            return None;
        }
        let count = u32::from_le_bytes(data[24..28].try_into().ok()?) as usize;
        read_utf16(&data[28..], count)
    } else {
        read_utf16(&data[24..], usize::MAX)
    };
    if original_path.is_empty() {
        return None;
    }
    let name = std::path::Path::new(&original_path)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| original_path.clone());

    Some(RecycleBinItem {
        id: payload_path.to_string_lossy().into_owned(),
        name,
        original_path,
        bytes: size,
        deleted_at_ms,
    })
}

/// Decode up to `max_chars` UTF-16LE code units, stopping at NUL.
fn read_utf16(data: &[u8], max_chars: usize) -> String {
    let units: Vec<u16> = data
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .take(max_chars)
        .take_while(|u| *u != 0)
        .collect();
    String::from_utf16_lossy(&units)
}

/// Path comparison for bin roots: one separator style, no trailing
/// separator, case-insensitive (Windows paths are case-insensitive;
/// the normalized comparison is harmless on other hosts).
fn same_path(a: &Path, b: &Path) -> bool {
    fn norm(p: &Path) -> String {
        p.to_string_lossy()
            .replace('/', "\\")
            .trim_end_matches('\\')
            .to_lowercase()
    }
    norm(a) == norm(b)
}

/// Validate an item id before removal: it must name a `$R…` payload
/// directly inside a SID folder of one of the machine's REAL
/// `$Recycle.Bin` roots (see `recycle_bin_roots`), with its `$I…`
/// metadata sibling present. A folder merely named `$Recycle.Bin`
/// anywhere else does not qualify. Returns `(payload_path,
/// info_path)` on success.
pub fn validate_item_id(id: &str) -> Option<(PathBuf, PathBuf)> {
    validate_item_id_within(id, &recycle_bin_roots())
}

/// `validate_item_id` against an explicit root set, so the rule is
/// testable without real drives.
fn validate_item_id_within(id: &str, roots: &[PathBuf]) -> Option<(PathBuf, PathBuf)> {
    let payload = PathBuf::from(id);
    let file_name = payload.file_name()?.to_string_lossy().into_owned();
    if !file_name.to_uppercase().starts_with("$R") {
        return None;
    }
    let sid_dir = payload.parent()?;
    let bin_root = sid_dir.parent()?;
    if !roots.iter().any(|root| same_path(bin_root, root)) {
        return None;
    }
    let info = sid_dir.join(format!("$I{}", &file_name[2..]));
    if !info.exists() {
        return None;
    }
    Some((payload, info))
}

/// Parse the `$I…` metadata paired with a validated payload — the
/// server-side record of what the item is (original path, size,
/// deletion time). Callers that report or record an operation use
/// THIS, never the webview's copy of the item.
pub fn read_item(payload: &Path, info: &Path) -> Option<RecycleBinItem> {
    let data = std::fs::read(info).ok()?;
    parse_info_file(&data, payload)
}

/// The one definition of "nothing occupies this destination": no
/// directory entry of any kind exists at the path — a dangling
/// symlink counts as OCCUPIED, because its metadata is there even
/// though its target is gone. Restore refuses occupied destinations
/// and the History restorable-count doesn't count them; both use
/// this predicate so they can never disagree.
pub fn destination_is_free(path: &str) -> bool {
    Path::new(path).symlink_metadata().is_err()
}

/// Whether a restore destination is off-limits: inside one of the
/// core safety gate's protected prefixes, a protected system file
/// name, or a report-only location. The lists come from
/// `diskpilot_core::safety` (the single source of truth); only the
/// prefix comparison is local, because the core's own comparison
/// helper is not public — worth unifying in core later.
fn destination_is_protected(destination: &Path) -> bool {
    if diskpilot_core::safety::is_report_only(destination) {
        return true;
    }
    if let Some(name) = destination.file_name() {
        let name = name.to_string_lossy().to_lowercase();
        if diskpilot_core::safety::protected_filenames().contains(&name.as_str()) {
            return true;
        }
    }
    diskpilot_core::safety::protected_prefixes()
        .iter()
        .any(|prefix| same_path(destination, prefix) || path_is_below(destination, prefix))
}

/// Is `path` strictly below `prefix` (component-wise, normalized
/// like `same_path`)? `C:\WindowsOld` is not below `C:\Windows`.
fn path_is_below(path: &Path, prefix: &Path) -> bool {
    fn norm(p: &Path) -> String {
        p.to_string_lossy()
            .replace('/', "\\")
            .trim_end_matches('\\')
            .to_lowercase()
    }
    let (path, prefix) = (norm(path), norm(prefix));
    path.len() > prefix.len() && path.starts_with(&prefix) && path.as_bytes()[prefix.len()] == b'\\'
}

/// Move a file, guaranteeing no overwrite. On Windows this is
/// `MoveFileExW` WITHOUT `MOVEFILE_REPLACE_EXISTING`: the move
/// itself fails when the destination exists, so the guarantee is
/// atomic rather than check-then-move. Off Windows (development
/// hosts only — the bin is empty there) it is the free-check plus
/// `rename`.
#[cfg(windows)]
fn move_no_overwrite(from: &Path, to: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;

    #[link(name = "Kernel32")]
    extern "system" {
        fn MoveFileExW(existing: *const u16, new: *const u16, flags: u32) -> i32;
    }

    let wide = |p: &Path| -> Vec<u16> { p.as_os_str().encode_wide().chain(Some(0)).collect() };
    let (from_w, to_w) = (wide(from), wide(to));
    // flags = 0: no REPLACE_EXISTING, no cross-volume copy fallback.
    let ok = unsafe { MoveFileExW(from_w.as_ptr(), to_w.as_ptr(), 0) };
    if ok == 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(not(windows))]
fn move_no_overwrite(from: &Path, to: &Path) -> std::io::Result<()> {
    if to.symlink_metadata().is_ok() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            "destination already exists",
        ));
    }
    std::fs::rename(from, to)
}

/// Comparison key for matching a recorded original path against a
/// bin item's: Windows paths compare case-insensitively and either
/// separator spelling means the same location.
fn original_path_key(path: &str) -> String {
    path.replace('/', "\\")
        .trim_end_matches('\\')
        .to_lowercase()
}

/// The bin item that was deleted from `original_path`, if one is
/// still in the bin. When several items share an original path
/// (deleted, restored, deleted again), the most recently deleted
/// one wins — callers pass the listing in any order.
pub fn find_by_original_path<'a>(
    items: &'a [RecycleBinItem],
    original_path: &str,
) -> Option<&'a RecycleBinItem> {
    let key = original_path_key(original_path);
    items
        .iter()
        .filter(|item| original_path_key(&item.original_path) == key)
        .max_by_key(|item| item.deleted_at_ms)
}

/// How many of `trash_paths` could be restored right now: the item
/// is still in the bin AND nothing occupies its original location
/// (restore refuses occupied destinations). `destination_free` is
/// injected so the counting is testable without a filesystem; in
/// production it is "no entry exists at the path".
pub fn count_restorable(
    trash_paths: &[String],
    bin_items: &[RecycleBinItem],
    destination_free: &dyn Fn(&str) -> bool,
) -> usize {
    trash_paths
        .iter()
        .filter(|path| find_by_original_path(bin_items, path).is_some())
        .filter(|path| destination_free(path))
        .count()
}

/// Restore one recycled item to the original path recorded in its
/// `$I…` metadata: the `$R…` payload moves back (parent folders are
/// recreated when the original folder has since been removed) and
/// the metadata file is deleted. Refuses when anything already sits
/// at the destination — a restore never overwrites — and when the
/// destination sits inside a protected location: the metadata is
/// on-disk data, and a crafted `$I…` must not be able to aim a
/// restore at system files.
pub fn restore_item(id: &str) -> Result<(), String> {
    let (payload, info) =
        validate_item_id(id).ok_or_else(|| "item is no longer in the Recycle Bin".to_string())?;
    let item =
        read_item(&payload, &info).ok_or_else(|| "item metadata is unreadable".to_string())?;
    let destination = PathBuf::from(&item.original_path);
    if destination_is_protected(&destination) {
        return Err(format!(
            "{} is inside a protected location — restore refused",
            item.original_path
        ));
    }
    if !destination_is_free(&item.original_path) {
        return Err(format!(
            "{} already exists — restore refused",
            item.original_path
        ));
    }
    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("could not recreate the original folder: {e}"))?;
    }
    move_no_overwrite(&payload, &destination)
        .map_err(|e| format!("could not restore {}: {e}", item.original_path))?;
    // The payload is home; a metadata file left behind would list a
    // ghost item, so its removal is part of the operation.
    std::fs::remove_file(&info)
        .map_err(|e| format!("restored, but could not remove the item metadata: {e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(original_path: &str, deleted_at_ms: i64) -> RecycleBinItem {
        RecycleBinItem {
            id: format!(
                "C:\\$Recycle.Bin\\S-1-5-21\\$R{:06}",
                deleted_at_ms.rem_euclid(1_000_000)
            ),
            name: original_path
                .rsplit('\\')
                .next()
                .unwrap_or(original_path)
                .to_string(),
            original_path: original_path.to_string(),
            bytes: 10,
            deleted_at_ms,
        }
    }

    #[test]
    fn find_matches_case_and_separator_insensitively() {
        let items = vec![item("C:\\Users\\Ana\\Downloads\\Report.PDF", 100)];
        assert!(find_by_original_path(&items, "c:\\users\\ana\\downloads\\report.pdf").is_some());
        assert!(find_by_original_path(&items, "C:/Users/Ana/Downloads/Report.PDF").is_some());
        assert!(find_by_original_path(&items, "C:\\Users\\Ana\\Downloads\\Other.PDF").is_none());
        assert!(find_by_original_path(&[], "C:\\Users\\Ana\\Downloads\\Report.PDF").is_none());
    }

    #[test]
    fn find_prefers_the_most_recently_deleted_copy() {
        let items = vec![
            item("D:\\scratch\\a.tmp", 100),
            item("D:\\scratch\\a.tmp", 300),
            item("D:\\scratch\\a.tmp", 200),
        ];
        let found = find_by_original_path(&items, "D:\\scratch\\a.tmp").unwrap();
        assert_eq!(found.deleted_at_ms, 300);
    }

    #[test]
    fn validate_accepts_only_payloads_under_real_bin_roots() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("$Recycle.Bin");
        let sid = root.join("S-1-5-21-1000");
        std::fs::create_dir_all(&sid).unwrap();
        std::fs::write(sid.join("$RABC123.txt"), b"payload").unwrap();
        std::fs::write(sid.join("$IABC123.txt"), b"metadata").unwrap();
        let roots = vec![root.clone()];

        // A genuine payload under a real root validates.
        let id = sid.join("$RABC123.txt").to_string_lossy().into_owned();
        let (payload, info) = validate_item_id_within(&id, &roots).expect("real bin payload");
        assert_eq!(payload, sid.join("$RABC123.txt"));
        assert_eq!(info, sid.join("$IABC123.txt"));

        // The exact same layout under a folder merely NAMED
        // `$Recycle.Bin` somewhere else does not qualify.
        let fake_sid = dir
            .path()
            .join("stash")
            .join("$Recycle.Bin")
            .join("S-1-5-21-1000");
        std::fs::create_dir_all(&fake_sid).unwrap();
        std::fs::write(fake_sid.join("$RABC123.txt"), b"payload").unwrap();
        std::fs::write(fake_sid.join("$IABC123.txt"), b"metadata").unwrap();
        let fake_id = fake_sid.join("$RABC123.txt").to_string_lossy().into_owned();
        assert!(validate_item_id_within(&fake_id, &roots).is_none());

        // A payload whose `$I…` sibling is gone does not validate.
        let lone = sid.join("$RLONE99.txt");
        std::fs::write(&lone, b"payload").unwrap();
        assert!(
            validate_item_id_within(&lone.to_string_lossy(), &roots).is_none(),
            "missing metadata sibling"
        );

        // Non-payload names under a real root do not validate.
        let other = sid.join("notes.txt");
        std::fs::write(&other, b"x").unwrap();
        assert!(validate_item_id_within(&other.to_string_lossy(), &roots).is_none());
    }

    #[test]
    fn restore_destinations_in_protected_locations_are_refused() {
        assert!(destination_is_protected(Path::new(
            r"C:\Windows\System32\drivers\etc\hosts"
        )));
        assert!(destination_is_protected(Path::new(
            r"C:\Program Files\App\app.exe"
        )));
        assert!(destination_is_protected(Path::new(r"C:\pagefile.sys")));
        assert!(destination_is_protected(Path::new(
            r"C:\Windows.old\Users\Ana\x.txt"
        )));
        assert!(!destination_is_protected(Path::new(
            r"C:\Users\Ana\Downloads\report.pdf"
        )));
        assert!(!destination_is_protected(Path::new(r"D:\scratch\a.tmp")));
        // A lookalike prefix is not the protected folder.
        assert!(!destination_is_protected(Path::new(
            r"C:\WindowsOld\file.txt"
        )));
    }

    #[test]
    fn destination_free_means_no_directory_entry_at_all() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("gone.txt");
        assert!(destination_is_free(&missing.to_string_lossy()));
        let present = dir.path().join("here.txt");
        std::fs::write(&present, b"x").unwrap();
        assert!(!destination_is_free(&present.to_string_lossy()));
    }

    #[cfg(unix)]
    #[test]
    fn a_dangling_symlink_counts_as_occupied() {
        let dir = tempfile::tempdir().unwrap();
        let link = dir.path().join("link.txt");
        std::os::unix::fs::symlink(dir.path().join("no-such-target"), &link).unwrap();
        assert!(
            !destination_is_free(&link.to_string_lossy()),
            "the link itself occupies the destination even though its target is gone"
        );
    }

    #[test]
    fn move_no_overwrite_moves_once_and_then_refuses() {
        let dir = tempfile::tempdir().unwrap();
        let from = dir.path().join("$R1.txt");
        let to = dir.path().join("restored.txt");
        std::fs::write(&from, b"payload").unwrap();
        move_no_overwrite(&from, &to).unwrap();
        assert_eq!(std::fs::read(&to).unwrap(), b"payload");
        assert!(!from.exists());

        let second = dir.path().join("$R2.txt");
        std::fs::write(&second, b"other").unwrap();
        assert!(move_no_overwrite(&second, &to).is_err());
        assert_eq!(
            std::fs::read(&to).unwrap(),
            b"payload",
            "the existing destination is untouched"
        );
        assert!(second.exists(), "the refused payload stays in place");
    }

    #[test]
    fn count_restorable_needs_bin_presence_and_a_free_destination() {
        let bin = vec![
            item("C:\\gone.tmp", 1),
            item("C:\\back.tmp", 2),
            item("C:\\occupied.tmp", 3),
        ];
        let paths: Vec<String> = [
            "C:\\gone.tmp",     // in bin, destination free → counts
            "C:\\back.tmp",     // restored already? no — still in bin, free → counts
            "C:\\occupied.tmp", // in bin but destination taken → does not count
            "C:\\never.tmp",    // not in bin → does not count
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let free = |p: &str| !p.eq_ignore_ascii_case("C:\\occupied.tmp");
        assert_eq!(count_restorable(&paths, &bin, &free), 2);
        assert_eq!(count_restorable(&[], &bin, &free), 0);
        assert_eq!(count_restorable(&paths, &[], &free), 0);
    }
}
