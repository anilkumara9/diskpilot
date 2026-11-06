//! Cleanup executor: the only module that removes anything. Every item is
//! re-validated through the safety gate immediately before acting.

use crate::error::CoreResult;
use crate::model::*;
use crate::safety::{self, SafetyContext};
use std::path::{Path, PathBuf};

/// File operations abstraction so execution is testable without touching a
/// real Recycle Bin or deleting real fixture trees permanently.
pub trait FileOps {
    fn move_to_trash(&self, path: &Path) -> Result<(), String>;
    fn permanent_delete(&self, path: &Path) -> Result<(), String>;
    fn exists(&self, path: &Path) -> bool {
        path.exists()
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct RealFileOps;

impl FileOps for RealFileOps {
    fn move_to_trash(&self, path: &Path) -> Result<(), String> {
        trash::delete(path).map_err(|e| e.to_string())
    }
    fn permanent_delete(&self, path: &Path) -> Result<(), String> {
        symlink_safe_delete(path)
    }
}

/// Delete a file or directory tree without ever following symlinks: a link
/// itself is removed, its target is never touched.
pub fn symlink_safe_delete(path: &Path) -> Result<(), String> {
    let md = std::fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    let ft = md.file_type();
    if ft.is_symlink() || !ft.is_dir() {
        return std::fs::remove_file(path).map_err(|e| e.to_string());
    }
    let entries: Vec<PathBuf> = std::fs::read_dir(path)
        .map_err(|e| e.to_string())?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .collect();
    for entry in entries {
        symlink_safe_delete(&entry)?;
    }
    std::fs::remove_dir(path).map_err(|e| e.to_string())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecuteMode {
    MoveToTrash,
    PermanentDelete,
}

pub fn execute_plan(
    plan: &CleanupPlan,
    mode: ExecuteMode,
    ctx: &SafetyContext,
    ops: &dyn FileOps,
) -> CoreResult<CleanupResult> {
    if mode == ExecuteMode::PermanentDelete && !plan.allow_permanent {
        return Err(crate::error::CoreError::SafetyRejected(
            "permanent delete is only allowed when every item is regenerable".to_string(),
        ));
    }
    let mut succeeded: Vec<String> = Vec::new();
    let mut failed: Vec<CleanupFailure> = Vec::new();
    let mut reclaimed: u64 = 0;
    for item in &plan.items {
        let path = PathBuf::from(&item.path);
        // Re-validate at the moment of action.
        if let Err(e) = safety::authorize_destructive(&path, ctx) {
            failed.push(CleanupFailure {
                path: item.path.clone(),
                message: e.to_string(),
            });
            continue;
        }
        let result = match mode {
            ExecuteMode::MoveToTrash => ops.move_to_trash(&path),
            ExecuteMode::PermanentDelete => ops.permanent_delete(&path),
        };
        match result {
            Ok(()) => {
                succeeded.push(item.path.clone());
                reclaimed += item.bytes;
            }
            Err(message) => failed.push(CleanupFailure {
                path: item.path.clone(),
                message,
            }),
        }
    }
    Ok(CleanupResult {
        plan_id: plan.id.clone(),
        succeeded,
        failed,
        reclaimed_bytes: reclaimed,
        finished_at_ms: now_ms(),
    })
}
