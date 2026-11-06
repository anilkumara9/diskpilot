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
//! pairs the UI was shown — items that arrive later are never touched.
//! The item `id` is the payload's absolute path; before anything is
//! removed, `validate_item_id` re-checks that the id really points at a
//! `$R…` payload inside a `$Recycle.Bin` folder with its `$I…` sibling
//! present, so the command cannot be aimed elsewhere.
//!
//! Two more operations live here because they speak the same on-disk
//! language: restoring one item (move the `$R…` payload back to the
//! path its `$I…` metadata records, then remove the metadata), and
//! emptying the whole bin through the shell API
//! (`SHEmptyRecycleBinW`), which is what Explorer's "Empty Recycle
//! Bin" does.

use std::path::PathBuf;

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

/// The system drive's `$Recycle.Bin` root, e.g. `C:\$Recycle.Bin`.
#[cfg(windows)]
fn recycle_bin_root() -> PathBuf {
    let drive = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".to_string());
    PathBuf::from(drive).join("$Recycle.Bin")
}

/// List every recycled item on the system drive (all user SID folders;
/// folders the process cannot read are skipped).
#[cfg(windows)]
pub fn list_items() -> Vec<RecycleBinItem> {
    let root = recycle_bin_root();
    let mut items = Vec::new();
    let Ok(sid_dirs) = std::fs::read_dir(&root) else {
        return items;
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
    items.sort_by(|a, b| b.deleted_at_ms.cmp(&a.deleted_at_ms));
    items
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

/// Validate an item id before removal: it must name a `$R…` payload
/// inside a `$Recycle.Bin` folder whose `$I…` metadata sibling exists.
/// Returns `(payload_path, info_path)` on success.
pub fn validate_item_id(id: &str) -> Option<(PathBuf, PathBuf)> {
    let payload = PathBuf::from(id);
    let file_name = payload.file_name()?.to_string_lossy().into_owned();
    if !file_name.to_uppercase().starts_with("$R") {
        return None;
    }
    let parent = payload.parent()?;
    let in_bin = parent
        .parent()
        .and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().eq_ignore_ascii_case("$Recycle.Bin"))
        .unwrap_or(false);
    if !in_bin {
        return None;
    }
    let info = parent.join(format!("$I{}", &file_name[2..]));
    if !info.exists() {
        return None;
    }
    Some((payload, info))
}

/// Restore one recycled item to the original path recorded in its
/// `$I…` metadata: the `$R…` payload moves back (parent folders are
/// recreated when the original folder has since been removed) and
/// the metadata file is deleted. Refuses when anything already sits
/// at the destination — a restore never overwrites.
pub fn restore_item(id: &str) -> Result<(), String> {
    let (payload, info) = validate_item_id(id)
        .ok_or_else(|| "item is no longer in the Recycle Bin".to_string())?;
    let data = std::fs::read(&info).map_err(|e| format!("could not read item metadata: {e}"))?;
    let item = parse_info_file(&data, &payload)
        .ok_or_else(|| "item metadata is unreadable".to_string())?;
    let destination = PathBuf::from(&item.original_path);
    if destination.symlink_metadata().is_ok() {
        return Err(format!(
            "{} already exists — restore refused",
            item.original_path
        ));
    }
    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("could not recreate the original folder: {e}"))?;
    }
    std::fs::rename(&payload, &destination)
        .map_err(|e| format!("could not restore {}: {e}", item.original_path))?;
    // The payload is home; a metadata file left behind would list a
    // ghost item, so its removal is part of the operation.
    std::fs::remove_file(&info)
        .map_err(|e| format!("restored, but could not remove the item metadata: {e}"))?;
    Ok(())
}
