//! The scan-history time series, persisted as `scan_history.json`
//! next to the snapshot in the app data directory.
//!
//! One entry per completed scan (manual or scheduled), oldest first,
//! capped at [`MAX_ENTRIES`]. It powers three features: the history
//! chart, the core's space forecast, and the scheduled-scan growth
//! alert. Writes are best-effort: a history file that cannot be read
//! or written must never fail a scan, so every failure here is logged
//! to stderr and swallowed.

use std::collections::HashMap;

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
    let path = state.scan_history_file();
    match std::fs::read_to_string(&path) {
        Ok(text) => serde_json::from_str(&text).unwrap_or_else(|e| {
            eprintln!("diskpilot: ignoring unreadable scan history {}: {e}", path.display());
            Vec::new()
        }),
        Err(_) => Vec::new(),
    }
}

/// Append a completed scan to the series, dropping the oldest
/// entries past the cap. Non-fatal by design: failures are logged,
/// never propagated.
pub fn append(state: &ShellState, summary: &ScanSummary) {
    let path = state.scan_history_file();
    let mut entries = load(state);
    entries.push(ScanHistoryEntry::from_summary(summary));
    if entries.len() > MAX_ENTRIES {
        let excess = entries.len() - MAX_ENTRIES;
        entries.drain(..excess);
    }
    let write = serde_json::to_string_pretty(&entries)
        .map_err(|e| e.to_string())
        .and_then(|text| std::fs::write(&path, text).map_err(|e| e.to_string()));
    if let Err(e) = write {
        eprintln!("diskpilot: could not write scan history {}: {e}", path.display());
    }
}
