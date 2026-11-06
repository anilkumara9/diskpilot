//! The scan-history time series, persisted as `scan_history.json`
//! next to the snapshot in the app data directory.
//!
//! One entry per completed scan (manual or scheduled), oldest first,
//! capped at [`MAX_ENTRIES`]. It powers three features: the history
//! chart, the core's space forecast, and the scheduled-scan growth
//! alert. Writes are best-effort: a history file that cannot be read
//! or written must never fail a scan, so every failure here is logged
//! to stderr and swallowed.
//!
//! Durability rules: appends write through a temp file + rename, so
//! a crash mid-write keeps the previous series; loads are tolerant —
//! one malformed entry is skipped and the valid entries survive,
//! instead of a single bad record discarding the whole file.

use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use diskpilot_core::model::{Category, ScanSummary};

use crate::ShellState;

/// How many scans the time series keeps. At one scan a day this is
/// four months of trend — plenty for the forecast's 30-day window.
pub const MAX_ENTRIES: usize = 120;

/// One completed scan in the time series. Category keys are the
/// categories' serde names — the same keys a snapshot's
/// `categoryTotals` map uses.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanHistoryEntry {
    /// When the scan finished (the summary's `finished_at_ms`).
    pub scanned_at_ms: i64,
    pub root_path: String,
    pub total_bytes: u64,
    pub file_count: u64,
    /// Free space on the root's drive at scan end, when known.
    pub drive_free_bytes: Option<u64>,
    pub categories: HashMap<String, u64>,
}

impl ScanHistoryEntry {
    pub fn from_summary(summary: &ScanSummary) -> Self {
        let categories = summary
            .category_totals
            .iter()
            .map(|(category, bytes)| (category_key(*category), *bytes))
            .collect();
        Self {
            scanned_at_ms: summary.finished_at_ms,
            root_path: summary.root.clone(),
            total_bytes: summary.total_bytes,
            file_count: summary.file_count,
            drive_free_bytes: summary.drive_free_bytes,
            categories,
        }
    }
}

/// The key serde uses for a category inside a snapshot's
/// `categoryTotals` map: its camelCase serde name.
fn category_key(category: Category) -> String {
    match serde_json::to_value(category) {
        Ok(serde_json::Value::String(key)) => key,
        _ => "unknown".to_string(),
    }
}

/// Load the time series (oldest → newest). A missing or unreadable
/// file reads as an empty series.
pub fn load(state: &ShellState) -> Vec<ScanHistoryEntry> {
    load_from(&state.scan_history_file())
}

fn load_from(path: &Path) -> Vec<ScanHistoryEntry> {
    match std::fs::read_to_string(path) {
        Ok(text) => parse_series(&text),
        Err(_) => Vec::new(),
    }
}

/// Parse the persisted series tolerantly: the file must be a JSON
/// array, but each entry is decoded on its own — a malformed entry
/// is skipped (and counted in the log) while the valid series
/// survives. A file that is not a JSON array at all is fully
/// corrupt and reads as an empty series.
fn parse_series(text: &str) -> Vec<ScanHistoryEntry> {
    let Ok(serde_json::Value::Array(items)) = serde_json::from_str::<serde_json::Value>(text)
    else {
        eprintln!("diskpilot: ignoring unreadable scan history (not a JSON array)");
        return Vec::new();
    };
    let total = items.len();
    let entries: Vec<ScanHistoryEntry> = items
        .into_iter()
        .filter_map(|item| serde_json::from_value(item).ok())
        .collect();
    if entries.len() < total {
        eprintln!(
            "diskpilot: skipped {} malformed scan history entries",
            total - entries.len()
        );
    }
    entries
}

/// Append a completed scan to the series, dropping the oldest
/// entries past the cap. Non-fatal by design: failures are logged,
/// never propagated.
pub fn append(state: &ShellState, summary: &ScanSummary) {
    append_to(&state.scan_history_file(), summary);
}

fn append_to(path: &Path, summary: &ScanSummary) {
    let mut entries = load_from(path);
    entries.push(ScanHistoryEntry::from_summary(summary));
    if entries.len() > MAX_ENTRIES {
        let excess = entries.len() - MAX_ENTRIES;
        entries.drain(..excess);
    }
    let write = serde_json::to_string_pretty(&entries)
        .map_err(|e| e.to_string())
        .and_then(|text| {
            // Temp file + rename (the settings/OCR-cache pattern): a
            // crash mid-write leaves the previous series intact.
            let tmp = path.with_extension("json.tmp");
            std::fs::write(&tmp, text)
                .and_then(|_| std::fs::rename(&tmp, path))
                .map_err(|e| e.to_string())
        });
    if let Err(e) = write {
        eprintln!(
            "diskpilot: could not write scan history {}: {e}",
            path.display()
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use diskpilot_core::model::ScanSummary;

    fn entry_json(scanned_at_ms: i64) -> serde_json::Value {
        serde_json::json!({
            "scannedAtMs": scanned_at_ms,
            "rootPath": "C:\\",
            "totalBytes": 100,
            "fileCount": 3,
            "driveFreeBytes": null,
            "categories": {},
        })
    }

    #[test]
    fn one_malformed_entry_does_not_nuke_the_series() {
        let text = serde_json::to_string(&serde_json::json!([
            entry_json(1),
            { "scannedAtMs": "not-a-number" },
            entry_json(2),
            42,
            entry_json(3),
        ]))
        .unwrap();
        let entries = parse_series(&text);
        let stamps: Vec<i64> = entries.iter().map(|e| e.scanned_at_ms).collect();
        assert_eq!(stamps, vec![1, 2, 3]);
    }

    #[test]
    fn a_fully_corrupt_file_reads_as_empty() {
        assert!(parse_series("{ not json").is_empty());
        assert!(parse_series("{\"not\": \"an array\"}").is_empty());
        assert!(parse_series("").is_empty());
    }

    #[test]
    fn append_writes_atomically_and_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("scan_history.json");
        let summary = ScanSummary {
            root: "C:\\".to_string(),
            finished_at_ms: 1234,
            ..Default::default()
        };
        append_to(&path, &summary);
        append_to(&path, &summary);
        let entries = load_from(&path);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].scanned_at_ms, 1234);
        assert!(
            !path.with_extension("json.tmp").exists(),
            "the temp file is renamed away, not left behind"
        );
    }
}
