//! Persist the last scan summary as JSON so the UI can show the previous
//! state instantly on launch.

use crate::error::CoreResult;
use crate::model::ScanSummary;
use std::path::Path;

pub fn save_snapshot(summary: &ScanSummary, path: &Path) -> CoreResult<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(summary)?;
    std::fs::write(path, json)?;
    Ok(())
}

pub fn load_snapshot(path: &Path) -> CoreResult<Option<ScanSummary>> {
    if !path.exists() {
        return Ok(None);
    }
    let text = std::fs::read_to_string(path)?;
    let summary: ScanSummary = serde_json::from_str(&text)?;
    Ok(Some(summary))
}

pub fn default_snapshot_path() -> std::path::PathBuf {
    let base = dirs::data_local_dir()
        .or_else(dirs::data_dir)
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    base.join("DiskPilot").join("last-scan.json")
}
