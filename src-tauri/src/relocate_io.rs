//! Folder relocation: move a folder to another drive and leave a
//! directory junction at its original location, so every path that
//! referenced the folder keeps working.
//!
//! The sequence is deliberately failure-stingy — the original is
//! never touched until a verified copy exists elsewhere:
//!
//! 1. validate (exists, a real folder, not a link, not protected,
//!    destination on a DIFFERENT volume with room — the space and
//!    protection rules are the core's `relocate::validate_relocation`),
//! 2. copy with robocopy (argv, never a shell),
//! 3. verify the copy (file count AND allocated bytes, files only),
//! 4. delete the original tree (symlink-safe: links inside are
//!    removed, never followed),
//! 5. create the junction at the original path via
//!    `FSCTL_SET_REPARSE_POINT`.
//!
//! If step 5 fails, the move is rolled back (copy back, verify the
//! count, remove the destination tree) and the error says so. Undo
//! (from the History entry) re-validates that the junction still
//! points at the recorded destination before it removes anything.
//!
//! Windows-only: robocopy, junctions and volume APIs have no
//! equivalent elsewhere, and other platforms get a plain error.
//! NOTE: compile-verified only — no Windows machine was available
//! to execute a relocation.

#[cfg(windows)]
use std::path::PathBuf;

use serde::Serialize;
use tauri::State;

use diskpilot_core::model::now_ms;

use crate::commands::with_history;
use crate::ShellState;

type CmdResult<T> = Result<T, String>;

/// What a relocation did: the folder that lived at `source` now
/// physically lives at `target`, with a junction left at `source`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RelocateResult {
    pub source: String,
    pub target: String,
    pub bytes: u64,
    /// Set when the operation itself succeeded but recording it
    /// to History failed: the work is done, but Undo (which is
    /// driven by the History entry) is not available for it.
    pub warning: Option<String>,
}

/// Why a relocation failed. The `SplitTree` variant is the
/// delete-original failure: a verified copy exists at the
/// destination and (part of) the original may still exist too, no
/// junction was created — a state the user must be able to find
/// again, so the command records it to History before reporting.
enum RelocateError {
    Message(String),
    SplitTree {
        message: String,
        source: String,
        target: String,
        bytes: u64,
    },
}

impl RelocateError {
    fn message(&self) -> String {
        match self {
            RelocateError::Message(message) => message.clone(),
            RelocateError::SplitTree { message, .. } => message.clone(),
        }
    }
}

impl From<String> for RelocateError {
    fn from(message: String) -> Self {
        RelocateError::Message(message)
    }
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

/// Move the folder at `source` onto the drive holding `dest_dir`
/// (the folder keeps its name: the destination is
/// `dest_dir\<name>`), leaving a junction behind. Runs on a
/// blocking worker thread — a large folder takes as long as the
/// copy takes. The move is recorded to History, which is also what
/// makes it undoable.
#[tauri::command]
pub async fn relocate_folder(
    state: State<'_, ShellState>,
    source: String,
    dest_dir: String,
) -> CmdResult<RelocateResult> {
    let outcome = tauri::async_runtime::spawn_blocking(move || relocate_impl(&source, &dest_dir))
        .await
        .map_err(|e| format!("background task failed: {e}"))?;
    let mut result = match outcome {
        Ok(result) => result,
        Err(failure) => {
            // A split tree (verified copy made, original not fully
            // removed, no junction) is recorded before the error is
            // reported, so the copy's location is never lost.
            if let RelocateError::SplitTree {
                source,
                target,
                bytes,
                ..
            } = &failure
            {
                let paths = vec![source.clone(), target.clone()];
                let message = failure.message();
                let detail = format!(
                    "Relocation of {source} to {target} stopped after the copy was verified: the original could not be fully removed and no link was created"
                );
                let _ = with_history(&state, |history| {
                    history
                        .record_action(
                            now_ms(),
                            "relocate-failed",
                            &paths,
                            *bytes,
                            std::slice::from_ref(&message),
                            Some(&detail),
                            diskpilot_core::history::RemovedItemMode::Permanent,
                        )
                        .map(|_| ())
                });
            }
            return Err(failure.message());
        }
    };
    let detail = format!(
        "Moved {} to {} and left a junction at the original location",
        result.source, result.target
    );
    let paths = vec![result.source.clone(), result.target.clone()];
    let bytes = result.bytes;
    let recorded = with_history(&state, |history| {
        history
            .record_action(
                now_ms(),
                "relocate",
                &paths,
                bytes,
                &[],
                Some(&detail),
                // The core history API stamps every item with one
                // removal mode and has no "not a removal" value;
                // Permanent is the honest stamp here — it keeps the
                // entry out of Recycle-Bin group-restore, and undo
                // is this module's own operation.
                diskpilot_core::history::RemovedItemMode::Permanent,
            )
            .map(|_| ())
    });
    if recorded.is_err() {
        result.warning = Some(
            "The move completed, but it couldn't be recorded in History, so Undo won't be available for it."
                .to_string(),
        );
    }
    Ok(result)
}

/// Reverse the relocation recorded in History entry `entry_id`:
/// remove the junction (the link itself — its contents are never
/// touched), copy the folder back, verify, and remove the copy at
/// the destination. Refuses when the entry is not a relocation,
/// when the original location is no longer a junction, or when the
/// junction no longer points at the recorded destination.
#[tauri::command]
pub async fn undo_relocation(
    state: State<'_, ShellState>,
    entry_id: i64,
) -> CmdResult<RelocateResult> {
    let entry = with_history(&state, |history| {
        Ok(history
            .list_history(10_000)?
            .into_iter()
            .find(|e| e.id == entry_id))
    })?
    .ok_or_else(|| format!("history entry {entry_id} not found"))?;
    if entry.operation != "relocate" {
        return Err("that history entry is not a folder relocation".to_string());
    }
    if entry.paths.len() != 2 {
        return Err("that relocation record is incomplete".to_string());
    }
    let (source, target) = (entry.paths[0].clone(), entry.paths[1].clone());
    let mut result = tauri::async_runtime::spawn_blocking(move || undo_impl(&source, &target))
        .await
        .map_err(|e| format!("background task failed: {e}"))??;
    let detail = format!(
        "Moved {} back to {} and removed the junction",
        result.target, result.source
    );
    let paths = vec![result.target.clone(), result.source.clone()];
    let bytes = result.bytes;
    let recorded = with_history(&state, |history| {
        history
            .record_action(
                now_ms(),
                "relocate-undo",
                &paths,
                bytes,
                &[],
                Some(&detail),
                diskpilot_core::history::RemovedItemMode::Permanent,
            )
            .map(|_| ())
    });
    if recorded.is_err() {
        result.warning = Some(
            "The folder was moved back, but the undo couldn't be recorded in History."
                .to_string(),
        );
    }
    Ok(result)
}

// ---------------------------------------------------------------------------
// Platform entry points
// ---------------------------------------------------------------------------

#[cfg(not(windows))]
fn relocate_impl(_source: &str, _dest_dir: &str) -> Result<RelocateResult, RelocateError> {
    Err(RelocateError::Message(
        "Folder relocation is only available on Windows".to_string(),
    ))
}

#[cfg(not(windows))]
fn undo_impl(_source: &str, _target: &str) -> Result<RelocateResult, String> {
    Err("Folder relocation is only available on Windows".to_string())
}

/// The canonical spelling of an existing path with the `\\?\`
/// verbatim prefix removed: links and 8.3 short names are resolved
/// (so string-prefix protection checks see the real location),
/// while the result stays an ordinary DOS path that robocopy and
/// the junction code accept. `None` when the path cannot be
/// canonicalized (it does not exist).
#[cfg(windows)]
fn resolved_spelling(path: &std::path::Path) -> Option<PathBuf> {
    let canonical = std::fs::canonicalize(path).ok()?;
    let text = canonical.to_string_lossy().into_owned();
    let stripped = if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{rest}")
    } else if let Some(rest) = text.strip_prefix(r"\\?\") {
        rest.to_string()
    } else {
        text
    };
    Some(PathBuf::from(stripped))
}

/// Measure a tree the way the verification compares it: files only
/// (directory-entry slack never counts), allocated bytes via the
/// crate's rule, symlinks neither followed nor counted. Robocopy
/// runs with `/XJ`, so the copy skips links in exactly the same
/// way and the two sides of the comparison agree.
#[cfg(windows)]
fn walk_tree(root: &std::path::Path) -> (u64, u64) {
    fn inner(path: &std::path::Path, depth: usize, bytes: &mut u64, files: &mut u64) {
        if depth > 256 {
            return;
        }
        let Ok(entries) = std::fs::read_dir(path) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(md) = std::fs::symlink_metadata(&path) else {
                continue;
            };
            let file_type = md.file_type();
            if file_type.is_symlink() {
                continue;
            }
            if file_type.is_dir() {
                inner(&path, depth + 1, bytes, files);
            } else if file_type.is_file() {
                *files += 1;
                *bytes += diskpilot_core::scanner::allocated_size_for_path(&path);
            }
        }
    }
    let (mut bytes, mut files) = (0u64, 0u64);
    inner(root, 0, &mut bytes, &mut files);
    (bytes, files)
}

/// Whether `path` is protected territory per the core safety lists
/// (protected prefix, protected file name, or report-only). The
/// core's `relocate::validate_relocation` applies the same lists
/// again; this early check only orders the error messages.
#[cfg(windows)]
fn is_protected_location(path: &std::path::Path) -> bool {
    fn norm(p: &std::path::Path) -> String {
        p.to_string_lossy()
            .replace('/', "\\")
            .trim_end_matches('\\')
            .to_lowercase()
    }
    if diskpilot_core::safety::is_report_only(path) {
        return true;
    }
    if let Some(name) = path.file_name() {
        let name = name.to_string_lossy().to_lowercase();
        if diskpilot_core::safety::protected_filenames().contains(&name.as_str()) {
            return true;
        }
    }
    let path_norm = norm(path);
    diskpilot_core::safety::protected_prefixes()
        .iter()
        .any(|prefix| {
            let prefix_norm = norm(prefix);
            path_norm == prefix_norm
                || (path_norm.len() > prefix_norm.len()
                    && path_norm.starts_with(&prefix_norm)
                    && path_norm.as_bytes()[prefix_norm.len()] == b'\\')
        })
}

/// Path equality the Windows way: one separator spelling, no
/// trailing separator, case-insensitive.
#[cfg(windows)]
fn same_location(a: &str, b: &str) -> bool {
    fn norm(s: &str) -> String {
        s.replace('/', "\\").trim_end_matches('\\').to_lowercase()
    }
    norm(a) == norm(b)
}

#[cfg(windows)]
fn relocate_impl(source: &str, dest_dir: &str) -> Result<RelocateResult, RelocateError> {
    let given_source = PathBuf::from(source);
    let given_dest = PathBuf::from(dest_dir);

    // (a) The source must be a real, unprotected folder.
    // Link-ness is judged on the spelling the user gave:
    // canonicalizing first would resolve a junction to its target
    // and hide the fact that the path is a link at all.
    let md = std::fs::symlink_metadata(&given_source).map_err(|_| {
        RelocateError::Message(format!("that folder no longer exists: {source}"))
    })?;
    if md.file_type().is_symlink() || win::is_reparse_point(&given_source) {
        return Err(format!(
            "{source} is already a link — it cannot be relocated again"
        )
        .into());
    }
    if !md.is_dir() {
        return Err(format!("that path is not a folder: {source}").into());
    }
    if !given_dest.is_dir() {
        return Err(format!("the destination folder does not exist: {dest_dir}").into());
    }

    // (b) Protection checks run on CANONICAL spellings. The
    // protection layer compares path prefixes as strings, so an
    // 8.3 spelling (PROGRA~1) or a verbatim (\\?\) spelling of a
    // protected folder would otherwise slip a protected tree past
    // both the shell check below and the core's validation. The
    // user's original spelling is kept for display and History.
    let source_path = resolved_spelling(&given_source).ok_or_else(|| {
        RelocateError::Message(format!("that folder no longer exists: {source}"))
    })?;
    let dest_path = resolved_spelling(&given_dest).ok_or_else(|| {
        RelocateError::Message(format!(
            "the destination folder does not exist: {dest_dir}"
        ))
    })?;
    if is_protected_location(&source_path) {
        return Err(format!(
            "protected system location, never relocated: {source}"
        )
        .into());
    }

    // (c) Measure.
    let (bytes, files) = walk_tree(&source_path);

    // (d) Different volumes, and room at the destination.
    let source_volume = win::volume_root(&source_path)
        .ok_or_else(|| RelocateError::Message("could not determine the source drive".to_string()))?;
    let dest_volume = win::volume_root(&dest_path).ok_or_else(|| {
        RelocateError::Message("could not determine the destination drive".to_string())
    })?;
    if same_location(&source_volume, &dest_volume) {
        return Err(format!(
            "Choose a folder on a different drive — {source} is already on {source_volume}"
        )
        .into());
    }
    let free = win::free_space(&dest_path).ok_or_else(|| {
        RelocateError::Message("could not read the destination drive's free space".to_string())
    })?;

    // (e) The core rule decides the destination path (protection,
    // containment, headroom) — on the same canonical spellings.
    let target_path =
        diskpilot_core::relocate::validate_relocation(&source_path, &dest_path, bytes, free)?;
    if target_path.symlink_metadata().is_ok() {
        return Err(format!(
            "something already exists at the destination: {}",
            target_path.display()
        )
        .into());
    }
    let target_string = target_path.to_string_lossy().into_owned();

    // (f) Copy.
    let copy = win::run_robocopy(&source_path, &target_path)?;
    if copy.code < 0 || copy.code >= 8 {
        let _ = diskpilot_core::cleanup::executor::symlink_safe_delete(&target_path);
        return Err(format!(
            "the copy to {} failed (robocopy exit {}): {}",
            target_string, copy.code, copy.tail
        )
        .into());
    }

    // (g) Verify before anything is removed.
    let (copied_bytes, copied_files) = walk_tree(&target_path);
    if copied_files != files || copied_bytes != bytes {
        let _ = diskpilot_core::cleanup::executor::symlink_safe_delete(&target_path);
        return Err(
            "Verification failed — the copy does not match the original, so the original was not touched"
                .to_string()
                .into(),
        );
    }

    // (h) Remove the original tree. A failure here splits the
    // tree — a verified copy at the destination, a partial
    // original, no junction — and is reported as `SplitTree` so
    // the command records it to History before surfacing it.
    if let Err(e) = diskpilot_core::cleanup::executor::symlink_safe_delete(&source_path) {
        return Err(RelocateError::SplitTree {
            message: format!(
                "the folder was copied and verified at {target_string}, but the original could not be removed: {e}"
            ),
            source: source.to_string(),
            target: target_string,
            bytes,
        });
    }

    // (i) Leave the junction. On failure: roll back.
    if let Err(junction_error) = win::create_junction(&source_path, &target_path) {
        match rollback(&source_path, &target_path, files, bytes) {
            Ok(()) => {
                return Err(format!(
                    "could not create the link at the original location ({junction_error}); the move was rolled back — the folder is back at {source}"
                )
                .into());
            }
            Err(rollback_error) => {
                return Err(format!(
                    "could not create the link at the original location ({junction_error}), and the rollback copy also failed ({rollback_error}); the folder's contents are intact at {target_string}"
                )
                .into());
            }
        }
    }

    Ok(RelocateResult {
        source: source.to_string(),
        target: target_string,
        bytes,
        warning: None,
    })
}

/// Roll a failed relocation back: copy the destination tree back
/// to the (now empty) original path, verify the copy back (file
/// count AND allocated bytes — a count-only match could restore
/// truncated content), and only then remove the destination tree.
/// On a mismatch nothing is removed: both copies stay in place
/// and the error names both paths.
#[cfg(windows)]
fn rollback(
    source_path: &std::path::Path,
    target_path: &std::path::Path,
    expected_files: u64,
    expected_bytes: u64,
) -> Result<(), String> {
    let copy = win::run_robocopy(target_path, source_path)?;
    if copy.code < 0 || copy.code >= 8 {
        return Err(format!("robocopy exit {}: {}", copy.code, copy.tail));
    }
    let (restored_bytes, restored_files) = walk_tree(source_path);
    if restored_files != expected_files || restored_bytes != expected_bytes {
        return Err(format!(
            "the copy back does not match the verified copy ({restored_files} of {expected_files} files, {restored_bytes} of {expected_bytes} bytes) — both copies were left in place: the partial restore at {} and the complete copy at {}",
            source_path.display(),
            target_path.display()
        ));
    }
    diskpilot_core::cleanup::executor::symlink_safe_delete(target_path)
        .map_err(|e| format!("could not remove the destination copy: {e}"))?;
    Ok(())
}

#[cfg(windows)]
fn undo_impl(source: &str, target: &str) -> Result<RelocateResult, String> {
    let source_path = PathBuf::from(source);
    let target_path = PathBuf::from(target);

    if win::is_reparse_point(&source_path) {
        // The normal case: the original location is still the
        // junction the relocation left, and it must still point at
        // the recorded destination — a link the user has since
        // repointed is theirs, not ours to remove.
        let points_at = win::junction_target(&source_path)
            .ok_or_else(|| format!("could not read where the link at {source} points"))?;
        if !same_location(&points_at, target) {
            return Err(format!(
                "the link at {source} points at {points_at}, not the recorded destination {target} — refusing to touch it"
            ));
        }
        if !target_path.is_dir() {
            return Err(format!("the moved folder no longer exists at {target}"));
        }
        let (bytes, files) = walk_tree(&target_path);

        // Re-run the core rule (protection + headroom) against the
        // way back BEFORE the junction is touched — on canonical
        // spellings, like the relocation itself. Destination
        // parent is the original folder's parent, and the
        // destination it derives is the original path itself.
        let parent = source_path
            .parent()
            .ok_or_else(|| "the original location has no parent folder".to_string())?;
        let free = win::free_space(parent)
            .ok_or_else(|| "could not read the original drive's free space".to_string())?;
        let validated_target =
            resolved_spelling(&target_path).unwrap_or_else(|| target_path.clone());
        let validated_parent = resolved_spelling(parent).unwrap_or_else(|| parent.to_path_buf());
        diskpilot_core::relocate::validate_relocation(
            &validated_target,
            &validated_parent,
            bytes,
            free,
        )?;

        // Remove the junction itself. RemoveDirectoryW on a
        // junction removes the (empty) link directory entry only —
        // it never follows the link, so the destination's contents
        // are safe.
        win::remove_junction(&source_path)
            .map_err(|e| format!("could not remove the junction at {source}: {e}"))?;

        return copy_back(&source_path, &target_path, bytes, files, source, target);
    }

    // Interrupted undo: a previous undo removed the junction and
    // began copying back, then stopped — the source is a real
    // directory again (holding a partial copy) and the recorded
    // target still holds the complete tree. The History entry's
    // exact path match is the authorization to finish the job:
    // copy back over the partial copy, verify against a fresh
    // measure of the target, and remove the target tree. Any
    // other non-junction state is refused, as before.
    let source_is_real_dir = std::fs::symlink_metadata(&source_path)
        .map(|md| md.is_dir() && !md.file_type().is_symlink())
        .unwrap_or(false);
    if source_is_real_dir && target_path.is_dir() {
        let (bytes, files) = walk_tree(&target_path);
        return copy_back(&source_path, &target_path, bytes, files, source, target);
    }

    match std::fs::symlink_metadata(&source_path) {
        Ok(_) => Err(format!("{source} is no longer a link — nothing to undo")),
        Err(_) => Err(format!("{source} no longer exists — nothing to undo")),
    }
}

/// The shared undo tail: copy the recorded destination tree back
/// to the original location, verify file count AND allocated
/// bytes against the fresh measure of the destination, and only
/// then remove the destination copy. A failed copy or a failed
/// verification removes nothing — the complete copy stays at the
/// destination.
#[cfg(windows)]
fn copy_back(
    source_path: &std::path::Path,
    target_path: &std::path::Path,
    bytes: u64,
    files: u64,
    source: &str,
    target: &str,
) -> Result<RelocateResult, String> {
    let copy = win::run_robocopy(target_path, source_path)?;
    if copy.code < 0 || copy.code >= 8 {
        return Err(format!(
            "the copy back failed partway (robocopy exit {}): {}. The folder is partially restored at {source}; the complete copy is still at {target}",
            copy.code, copy.tail
        ));
    }
    let (restored_bytes, restored_files) = walk_tree(source_path);
    if restored_files != files || restored_bytes != bytes {
        return Err(format!(
            "verification of the restored copy failed, so nothing was removed: the complete copy is still at {target}"
        ));
    }
    diskpilot_core::cleanup::executor::symlink_safe_delete(target_path).map_err(|e| {
        format!(
            "the folder is restored at {source}, but the copy at {target} could not be removed: {e}"
        )
    })?;
    Ok(RelocateResult {
        source: source.to_string(),
        target: target.to_string(),
        bytes,
        warning: None,
    })
}

// ---------------------------------------------------------------------------
// Windows primitives
// ---------------------------------------------------------------------------

#[cfg(windows)]
mod win {
    use std::ffi::c_void;
    use std::io::Read;
    use std::os::windows::ffi::OsStrExt;
    use std::path::Path;
    use std::process::{Command, Stdio};

    type Handle = *mut c_void;

    const GENERIC_READ: u32 = 0x8000_0000;
    const GENERIC_WRITE: u32 = 0x4000_0000;
    const FILE_SHARE_READ: u32 = 0x0000_0001;
    const FILE_SHARE_WRITE: u32 = 0x0000_0002;
    const OPEN_EXISTING: u32 = 3;
    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;
    const INVALID_FILE_ATTRIBUTES: u32 = 0xFFFF_FFFF;

    const FSCTL_SET_REPARSE_POINT: u32 = 0x0009_00A4;
    const FSCTL_GET_REPARSE_POINT: u32 = 0x0009_00A8;
    const IO_REPARSE_TAG_MOUNT_POINT: u32 = 0xA000_0003;

    #[link(name = "Kernel32")]
    extern "system" {
        fn CreateFileW(
            file_name: *const u16,
            desired_access: u32,
            share_mode: u32,
            security_attributes: *mut c_void,
            creation_disposition: u32,
            flags_and_attributes: u32,
            template_file: Handle,
        ) -> Handle;
        fn CloseHandle(handle: Handle) -> i32;
        fn DeviceIoControl(
            device: Handle,
            control_code: u32,
            in_buffer: *const c_void,
            in_buffer_size: u32,
            out_buffer: *mut c_void,
            out_buffer_size: u32,
            bytes_returned: *mut u32,
            overlapped: *mut c_void,
        ) -> i32;
        fn GetFileAttributesW(file_name: *const u16) -> u32;
        fn RemoveDirectoryW(path_name: *const u16) -> i32;
        fn GetVolumePathNameW(
            file_name: *const u16,
            volume_path_name: *mut u16,
            buffer_length: u32,
        ) -> i32;
        fn GetDiskFreeSpaceExW(
            directory_name: *const u16,
            free_bytes_available: *mut u64,
            total_number_of_bytes: *mut u64,
            total_number_of_free_bytes: *mut u64,
        ) -> i32;
    }

    fn wide_null(path: &Path) -> Vec<u16> {
        path.as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    }

    fn last_error(context: &str) -> String {
        format!("{context}: {}", std::io::Error::last_os_error())
    }

    pub fn is_reparse_point(path: &Path) -> bool {
        let wide = wide_null(path);
        let attrs = unsafe { GetFileAttributesW(wide.as_ptr()) };
        attrs != INVALID_FILE_ATTRIBUTES && attrs & FILE_ATTRIBUTE_REPARSE_POINT != 0
    }

    /// The volume mount point a path lives on (`C:\`), via
    /// `GetVolumePathNameW`.
    pub fn volume_root(path: &Path) -> Option<String> {
        let wide = wide_null(path);
        let mut buffer = [0u16; 264];
        let ok =
            unsafe { GetVolumePathNameW(wide.as_ptr(), buffer.as_mut_ptr(), buffer.len() as u32) };
        if ok == 0 {
            return None;
        }
        let end = buffer.iter().position(|c| *c == 0).unwrap_or(buffer.len());
        Some(String::from_utf16_lossy(&buffer[..end]))
    }

    /// Free bytes available to the caller on `path`'s volume.
    pub fn free_space(path: &Path) -> Option<u64> {
        let wide = wide_null(path);
        let (mut available, mut total, mut free) = (0u64, 0u64, 0u64);
        let ok =
            unsafe { GetDiskFreeSpaceExW(wide.as_ptr(), &mut available, &mut total, &mut free) };
        if ok == 0 {
            None
        } else {
            Some(available)
        }
    }

    /// Open a directory as a reparse point (for the junction
    /// IOCTLs): backup semantics so directories open at all, and
    /// open-reparse-point so the link itself is the handle's target.
    fn open_reparse_point(path: &Path, access: u32) -> Result<Handle, String> {
        let wide = wide_null(path);
        let handle = unsafe {
            CreateFileW(
                wide.as_ptr(),
                access,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                std::ptr::null_mut(),
                OPEN_EXISTING,
                FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
                std::ptr::null_mut(),
            )
        };
        if handle.is_null() || handle as isize == -1 {
            Err(last_error("could not open the folder as a link"))
        } else {
            Ok(handle)
        }
    }

    /// Create a directory junction at `link` pointing at `target`:
    /// first the (empty) directory, then the mount-point reparse
    /// data naming the target as an NT namespace path (`\??\D:\…`).
    /// A failure after the directory exists removes it again, so a
    /// failed attempt leaves no trace.
    pub fn create_junction(link: &Path, target: &Path) -> Result<(), String> {
        std::fs::create_dir(link)
            .map_err(|e| format!("could not create the junction folder: {e}"))?;
        let outcome = set_mount_point(link, target);
        if outcome.is_err() {
            let _ = std::fs::remove_dir(link);
        }
        outcome
    }

    fn set_mount_point(link: &Path, target: &Path) -> Result<(), String> {
        let target_text = target
            .to_string_lossy()
            .replace('/', "\\")
            .trim_end_matches('\\')
            .to_string();
        let substitute = format!(r"\??\{target_text}");
        let sub_units: Vec<u16> = substitute.encode_utf16().collect();
        let print_units: Vec<u16> = target_text.encode_utf16().collect();
        let sub_len = (sub_units.len() * 2) as u16;
        let print_len = (print_units.len() * 2) as u16;
        // REPARSE_DATA_BUFFER (mount point form): tag, data length,
        // reserved, the four name offset/length words, then the
        // substitute and print names, each NUL-terminated. The data
        // length counts everything after its own field.
        let data_length = 8u16 + sub_len + 2 + print_len + 2;
        let mut buffer: Vec<u8> = Vec::with_capacity(8 + data_length as usize);
        buffer.extend_from_slice(&IO_REPARSE_TAG_MOUNT_POINT.to_le_bytes());
        buffer.extend_from_slice(&data_length.to_le_bytes());
        buffer.extend_from_slice(&0u16.to_le_bytes()); // Reserved
        buffer.extend_from_slice(&0u16.to_le_bytes()); // SubstituteNameOffset
        buffer.extend_from_slice(&sub_len.to_le_bytes());
        buffer.extend_from_slice(&(sub_len + 2).to_le_bytes()); // PrintNameOffset
        buffer.extend_from_slice(&print_len.to_le_bytes());
        for unit in sub_units.iter().chain(std::iter::once(&0)) {
            buffer.extend_from_slice(&unit.to_le_bytes());
        }
        for unit in print_units.iter().chain(std::iter::once(&0)) {
            buffer.extend_from_slice(&unit.to_le_bytes());
        }

        let handle = open_reparse_point(link, GENERIC_WRITE)?;
        let mut returned: u32 = 0;
        let ok = unsafe {
            DeviceIoControl(
                handle,
                FSCTL_SET_REPARSE_POINT,
                buffer.as_ptr() as *const c_void,
                buffer.len() as u32,
                std::ptr::null_mut(),
                0,
                &mut returned,
                std::ptr::null_mut(),
            )
        };
        unsafe {
            CloseHandle(handle);
        }
        if ok == 0 {
            return Err(last_error("could not set the junction"));
        }
        Ok(())
    }

    /// Where a junction at `link` points (the recorded substitute
    /// name, `\??\` prefix stripped). `None` for anything that is
    /// not a mount point or cannot be read.
    pub fn junction_target(link: &Path) -> Option<String> {
        let handle = open_reparse_point(link, GENERIC_READ).ok()?;
        let mut buffer = vec![0u8; 16 * 1024];
        let mut returned: u32 = 0;
        let ok = unsafe {
            DeviceIoControl(
                handle,
                FSCTL_GET_REPARSE_POINT,
                std::ptr::null(),
                0,
                buffer.as_mut_ptr() as *mut c_void,
                buffer.len() as u32,
                &mut returned,
                std::ptr::null_mut(),
            )
        };
        unsafe {
            CloseHandle(handle);
        }
        if ok == 0 || (returned as usize) < 16 {
            return None;
        }
        // The name slice is bounded twice over: by what the driver
        // actually returned (and the buffer), and by the
        // ReparseDataLength extent (bytes 4..6, counting everything
        // after the 8-byte header). A name that would run past
        // either is malformed — never read, never clamped into a
        // plausible-looking wrong answer.
        let limit = (returned as usize).min(buffer.len());
        let data_length = u16::from_le_bytes(buffer[4..6].try_into().ok()?) as usize;
        let extent = (8 + data_length).min(limit);
        let tag = u32::from_le_bytes(buffer[0..4].try_into().ok()?);
        if tag != IO_REPARSE_TAG_MOUNT_POINT {
            return None;
        }
        // Mount-point form: name offsets/lengths at bytes 8..16,
        // the path buffer at byte 16.
        let name_offset = u16::from_le_bytes(buffer[8..10].try_into().ok()?) as usize;
        let name_length = u16::from_le_bytes(buffer[10..12].try_into().ok()?) as usize;
        let start = 16 + name_offset;
        let end = start + name_length;
        if end > extent {
            return None;
        }
        let bytes = buffer.get(start..end)?;
        let units: Vec<u16> = bytes
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        let text = String::from_utf16_lossy(&units);
        Some(text.strip_prefix(r"\??\").unwrap_or(&text).to_string())
    }

    /// Remove a junction. `RemoveDirectoryW` on a junction removes
    /// the link's own (empty) directory entry — the Windows shell
    /// semantics are that the link is deleted, never its target —
    /// which is exactly the guarantee undo relies on.
    pub fn remove_junction(link: &Path) -> std::io::Result<()> {
        let wide = wide_null(link);
        let ok = unsafe { RemoveDirectoryW(wide.as_ptr()) };
        if ok == 0 {
            Err(std::io::Error::last_os_error())
        } else {
            Ok(())
        }
    }

    pub struct RobocopyOutcome {
        /// Process exit code; robocopy treats 0..=7 as success
        /// variants and >= 8 as failure. -1 when the OS reported
        /// no code.
        pub code: i32,
        /// The tail of robocopy's output, for error messages.
        pub tail: String,
    }

    /// Copy a tree with robocopy as an argv vector. Quiet flags
    /// keep the output small; reader threads drain the pipes
    /// anyway, so a huge tree cannot deadlock on a full pipe.
    /// There is deliberately no timeout: a copy's duration is
    /// bounded by its size, and killing one midway would strand a
    /// partial copy (the caller cleans that up on failure codes).
    ///
    /// `/XJ` excludes junctions (and symlinks to directories) from
    /// the copy: the verification measure (`walk_tree`) skips links
    /// the same way, so copy and measure agree — links inside the
    /// tree are not moved; they stay behind with the original
    /// tree's removal semantics applying to the link itself.
    pub fn run_robocopy(source: &Path, dest: &Path) -> Result<RobocopyOutcome, String> {
        let mut command = Command::new("robocopy");
        command
            .args([
                source.as_os_str(),
                dest.as_os_str(),
                "/E".as_ref(),
                "/COPY:DAT".as_ref(),
                "/DCOPY:DAT".as_ref(),
                "/XJ".as_ref(),
                "/R:1".as_ref(),
                "/W:1".as_ref(),
                "/NP".as_ref(),
                "/NFL".as_ref(),
                "/NDL".as_ref(),
                "/NJH".as_ref(),
                "/NJS".as_ref(),
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            command.creation_flags(CREATE_NO_WINDOW);
        }
        let mut child = command
            .spawn()
            .map_err(|e| format!("could not start robocopy: {e}"))?;
        let mut out_pipe = child.stdout.take();
        let out_thread = std::thread::spawn(move || {
            let mut buf = Vec::new();
            if let Some(pipe) = out_pipe.as_mut() {
                let _ = pipe.read_to_end(&mut buf);
            }
            buf
        });
        let mut err_pipe = child.stderr.take();
        let err_thread = std::thread::spawn(move || {
            let mut buf = Vec::new();
            if let Some(pipe) = err_pipe.as_mut() {
                let _ = pipe.read_to_end(&mut buf);
            }
            buf
        });
        let status = child
            .wait()
            .map_err(|e| format!("could not wait for robocopy: {e}"))?;
        let stdout = String::from_utf8_lossy(&out_thread.join().unwrap_or_default()).into_owned();
        let stderr = String::from_utf8_lossy(&err_thread.join().unwrap_or_default()).into_owned();
        let combined = format!("{stdout}\n{stderr}");
        let trimmed = combined.trim();
        let chars: Vec<char> = trimmed.chars().collect();
        let start = chars.len().saturating_sub(300);
        Ok(RobocopyOutcome {
            code: status.code().unwrap_or(-1),
            tail: chars[start..].iter().collect(),
        })
    }
}
