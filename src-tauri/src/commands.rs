//! Typed Tauri commands — the only bridge between the frontend and the
//! `diskpilot-core` engine.
//!
//! The webview has no filesystem or shell access (see
//! `capabilities/default.json`); everything the UI can do is one of the
//! commands below. Commands never accept raw paths for destructive work:
//! cleanup plans are built from recommendation IDs and the core resolves
//! and re-validates the paths itself. Long-running work runs on blocking
//! worker threads via `tauri::async_runtime::spawn_blocking`.
//!
//! Engine state (last scan, index, recommendations, planner, cancel
//! flag) is the core's own `diskpilot_core::AppState`, held by the shell
//! alongside settings, history and the assistant key store.
//!
//! Shell-side compositions (documented, deliberate):
//!
//! - `scan://partial` is part of the frontend event contract but is not
//!   emitted: the scanner reports progress only (current path + running
//!   file/byte counts). `scan://progress` and `scan://finished` are
//!   emitted.
//! - `get_environment` is composed from `std::env` and the core's
//!   `model::home_dir_string()`; the core has no environment module.
//! - `get_scan_comparison` diffs the current summary against the
//!   previous scan's summary, which the shell keeps by rotating
//!   `snapshot.json` to `previous_snapshot.json` on every scan.
//! - Recycle Bin listing/emptying lives in `crate::recycle_bin`: the
//!   core has no Recycle Bin *inventory* API (its safety gate protects
//!   `$Recycle.Bin` from plan-based cleanup). The shell parses the
//!   `$I…` metadata read-only and, on empty, removes exactly the listed
//!   `$R…` payloads after re-validating each ID. The same module
//!   restores single items from their metadata and empties the whole
//!   bin through the shell API (`SHEmptyRecycleBinW`).
//! - OneDrive status is computed by the core from the scan index (no
//!   second walk of the folder); `export_report` writes the frontend's
//!   HTML through the dialog plugin's Rust-side save dialog.
//! - `search_files` / `run_search_filters` compose the core's query
//!   parser and index search; the one filter dimension the index
//!   cannot carry — the download source — is applied in the shell by
//!   probing each hit's recorded provenance (capped at 500 hits).
//! - `remove_duplicate_copies` composes core pieces:
//!   `duplicates::verify_identical` re-verification per copy,
//!   `safety::authorize_destructive`, and
//!   `cleanup::RealFileOps::move_to_trash`.
//! - `reveal_in_explorer` uses a containment check (target must be the
//!   scanned root or inside it) rather than `safety::validate`: the
//!   gate is destructive-op oriented and rejects the root itself and
//!   profile top-level folders, both legitimate reveal targets.

use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};
use tauri_plugin_opener::OpenerExt;

use diskpilot_core::assistant::{
    AssistantSession, ChatMessage as CoreChatMessage, HttpAssistantClient, Provider, ToolContext,
};
use diskpilot_core::cleanup::{execute_plan, ExecuteMode, FileOps, RealFileOps};
use diskpilot_core::error::CoreError;
use diskpilot_core::forecast::{self, Forecast, HistoryPoint};
use diskpilot_core::history::HistoryStore;
use diskpilot_core::snapshot_diff::{diff_snapshots, BiggestChanges, SnapshotView};
use diskpilot_core::model::{
    now_ms, AppEntry, Category, Classification, CleanupFailure, CleanupPlan, CleanupResult,
    DiskInfo, DuplicateGroup, HistoryEntry, Recommendation, ScanSummary, SecretFinding, ToolReport,
};
use diskpilot_core::safety::SafetyContext;
use diskpilot_core::scanner::{ProgressCallback, ScanProgress, Scanner};
use diskpilot_core::{
    apps, assistant, devtools, disk, docker, duplicates, explorer, media_meta, provenance, safety,
    scanner, search, secrets, snapshot,
};

use crate::recycle_bin::RecycleBinItem;
use crate::{Settings, ShellState};

type CmdResult<T> = Result<T, String>;

fn err(e: CoreError) -> String {
    e.to_string()
}

/// Run a fallible core call on a blocking worker thread.
async fn blocking<T, F>(f: F) -> CmdResult<T>
where
    F: FnOnce() -> Result<T, CoreError> + Send + 'static,
    T: Send + 'static,
{
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| format!("background task failed: {e}"))?
        .map_err(err)
}

/// Run an infallible core call on a blocking worker thread.
async fn blocking_value<T, F>(f: F) -> CmdResult<T>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| format!("background task failed: {e}"))
}

/// Open (once) and borrow the SQLite history store.
fn with_history<T>(
    state: &ShellState,
    f: impl FnOnce(&HistoryStore) -> Result<T, CoreError>,
) -> CmdResult<T> {
    let mut guard = state
        .history
        .lock()
        .map_err(|_| "history lock poisoned".to_string())?;
    if guard.is_none() {
        let opened = HistoryStore::open(&state.history_file()).map_err(err)?;
        *guard = Some(opened);
    }
    let history = guard.as_ref().expect("history just opened");
    f(history).map_err(err)
}

/// Scan progress event payload (`scan://progress`).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ScanProgressPayload {
    current_path: String,
    files_seen: u64,
    bytes_seen: u64,
}

/// Recycle Bin progress event payload (`recycle-bin://progress`).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct RecycleBinProgressPayload {
    completed: usize,
    total: usize,
    current_name: String,
    freed_bytes: u64,
}

// ---------------------------------------------------------------------------
// Environment & disk
// ---------------------------------------------------------------------------

/// Static facts about this machine and the app build.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvironmentInfo {
    pub os: String,
    pub arch: String,
    pub home_dir: String,
    pub app_version: String,
}

#[tauri::command]
pub fn get_environment() -> EnvironmentInfo {
    EnvironmentInfo {
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        home_dir: diskpilot_core::model::home_dir_string(),
        app_version: env!("CARGO_PKG_VERSION").to_string(),
    }
}

/// Total / used / available bytes for the volume holding `path`
/// (defaults to the configured scan root). Refreshes the tray ring.
#[tauri::command]
pub fn get_disk_info(
    app: AppHandle,
    state: State<'_, ShellState>,
    path: Option<String>,
) -> DiskInfo {
    let root = match path {
        Some(p) if !p.is_empty() => p,
        _ => state.scan_root(),
    };
    let info = disk::disk_info_for(Path::new(&root));
    crate::tray::update_tray(&app, &info);
    info
}

/// Every mounted drive (volume) with its total and free space,
/// probed by the core. Blocking: unresponsive volumes can take a
/// moment to answer.
#[tauri::command]
pub async fn get_drives() -> CmdResult<Vec<diskpilot_core::drives::DriveInfo>> {
    blocking_value(diskpilot_core::drives::list_drives).await
}

// ---------------------------------------------------------------------------
// Scanning
// ---------------------------------------------------------------------------

/// Start a scan of `root` (defaults to the configured scan root).
///
/// Emits `scan://progress` (throttled by the core to ~120 ms) and
/// `scan://finished` with the final summary; resolves with the summary.
/// Only one scan runs at a time. The finished scan is recorded to
/// History and persisted as the launch snapshot; the snapshot it
/// replaces is rotated to `previous_snapshot.json` first, so the
/// scan-over-scan comparison always has the prior scan available.
/// A root that no longer exists is an error, not an empty scan; a
/// root that is a file scans as a single-file tree (supported by the
/// core scanner).
#[tauri::command]
pub async fn start_scan(
    app: AppHandle,
    state: State<'_, ShellState>,
    root: Option<String>,
) -> CmdResult<ScanSummary> {
    let root_path = match root {
        Some(r) if !r.is_empty() => r,
        _ => state.scan_root(),
    };
    run_scan(app, &state, root_path, false).await
}

/// The one scan routine, shared by the manual command above and the
/// scheduler in `crate::watch`. A scheduled run differs only in the
/// `scheduled` flag: after it completes, the alert checks run (manual
/// scans never notify). Everything else — progress/finished events,
/// snapshot rotation, History recording, the scan-history append —
/// is identical, so a scheduled scan is indistinguishable in the app
/// from a manual one.
pub(crate) async fn run_scan(
    app: AppHandle,
    state: &ShellState,
    root_path: String,
    scheduled: bool,
) -> CmdResult<ScanSummary> {
    if !Path::new(&root_path).exists() {
        return Err(format!("That folder no longer exists: {root_path}"));
    }

    let exclusions = state
        .settings
        .lock()
        .map(|settings| settings.exclusions.clone())
        .unwrap_or_default();

    let cancel = {
        let mut engine = state
            .engine
            .lock()
            .map_err(|_| "engine lock poisoned".to_string())?;
        if engine.scanning {
            return Err("A scan is already running.".to_string());
        }
        engine.scanning = true;
        engine.scan_cancel.store(false, Ordering::SeqCst);
        engine.scan_cancel.clone()
    };

    let progress_app = app.clone();
    let callback: ProgressCallback = Arc::new(move |progress: ScanProgress| {
        let _ = progress_app.emit(
            "scan://progress",
            ScanProgressPayload {
                current_path: progress.path.clone(),
                files_seen: progress.files_seen,
                bytes_seen: progress.bytes_seen,
            },
        );
    });

    let scanner = Scanner::new(&root_path)
        .with_cancel(cancel)
        .with_progress(callback)
        .with_exclusions(exclusions);
    let scan_result = tauri::async_runtime::spawn_blocking(move || scanner.scan()).await;

    let (summary, index) = match scan_result {
        Ok(pair) => pair,
        Err(e) => {
            if let Ok(mut engine) = state.engine.lock() {
                engine.scanning = false;
            }
            return Err(format!("scan task failed: {e}"));
        }
    };

    // store_scan derives the recommendations and clears `scanning`.
    if let Ok(mut engine) = state.engine.lock() {
        engine.store_scan(summary.clone(), index);
    }

    // Rotate the outgoing snapshot to the previous-scan slot before the
    // new one replaces it, and mirror the rotation in shell state.
    let snapshot_file = state.snapshot_file();
    if snapshot_file.exists() {
        let previous_file = state.previous_snapshot_file();
        if std::fs::copy(&snapshot_file, &previous_file).is_ok() {
            if let Ok(mut previous) = state.previous_summary.lock() {
                let restored = snapshot::load_snapshot(&previous_file).ok().flatten();
                if restored.is_some() {
                    *previous = restored;
                }
            }
        }
    }

    let _ = snapshot::save_snapshot(&summary, &snapshot_file);
    let _ = with_history(state, |history| history.record_scan(&summary).map(|_| ()));

    // Append to the scan-history time series (forecast + alerts).
    // Best-effort inside `scan_history::append`: it never fails here.
    crate::scan_history::append(state, &summary);

    // Keep the tray ring honest about the scanned volume, and the
    // tray's Recent downloads submenu current.
    let info = disk::disk_info_for(Path::new(&summary.root));
    crate::tray::update_tray(&app, &info);
    crate::tray::refresh_recent_downloads(&app);

    let _ = app.emit("scan://finished", summary.clone());

    if scheduled {
        send_scheduled_scan_alerts(&app, state, &summary);
    }
    Ok(summary)
}

/// Notifications after a SCHEDULED scan only (manual scans never
/// notify): low free space on the scanned drive, and total growth
/// beyond the configured threshold since the previous scan of the
/// same root. Sending is best-effort — a notification failure must
/// not fail the scan.
fn send_scheduled_scan_alerts(app: &AppHandle, state: &ShellState, summary: &ScanSummary) {
    use tauri_plugin_notification::NotificationExt;

    let settings = match state.settings.lock() {
        Ok(settings) => settings.clone(),
        Err(_) => return,
    };

    if let Some(free) = summary.drive_free_bytes {
        if (free as f64) < settings.low_space_alert_gb * 1e9 {
            let body = format!(
                "Only {:.1} GB free on {}. DiskPilot's Overview shows what is using the space.",
                free as f64 / 1e9,
                summary.root
            );
            let _ = app
                .notification()
                .builder()
                .title("DiskPilot: low disk space")
                .body(body)
                .show();
        }
    }

    // Growth is measured against the previous history entry for the
    // same root; the last entry is the scan that just finished.
    let history = crate::scan_history::load(state);
    let previous = history
        .iter()
        .rev()
        .skip(1)
        .find(|entry| same_scan_root(&entry.root_path, &summary.root));
    if let Some(previous) = previous {
        let growth = summary.total_bytes as i64 - previous.total_bytes as i64;
        if growth > 0 && (growth as f64) > settings.growth_alert_gb * 1e9 {
            let body = format!(
                "+{:.1} GB since the last check — open DiskPilot to see what grew",
                growth as f64 / 1e9
            );
            let _ = app
                .notification()
                .builder()
                .title("DiskPilot: disk usage grew")
                .body(body)
                .show();
        }
    }
}

/// Cancel the running scan, if any. The scanner stops at its next
/// progress checkpoint.
#[tauri::command]
pub fn cancel_scan(state: State<'_, ShellState>) -> CmdResult<()> {
    let engine = state
        .engine
        .lock()
        .map_err(|_| "engine lock poisoned".to_string())?;
    engine.scan_cancel.store(true, Ordering::SeqCst);
    Ok(())
}

/// The most recent scan summary (or the restored snapshot at launch).
/// `None` until the first scan has ever run.
#[tauri::command]
pub fn get_scan_summary(state: State<'_, ShellState>) -> CmdResult<Option<ScanSummary>> {
    let engine = state
        .engine
        .lock()
        .map_err(|_| "engine lock poisoned".to_string())?;
    Ok(engine.summary.clone())
}

/// One category's change between the previous scan and the current
/// one. `delta_bytes` is signed: negative means the category shrank.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CategoryDelta {
    pub category: Category,
    pub current_bytes: u64,
    pub previous_bytes: u64,
    pub delta_bytes: i64,
}

/// The current scan compared with the previous scan of the same root
/// (shell DTO). Deltas are signed: a negative `total_bytes_delta`
/// means space was freed overall.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanComparison {
    /// When the previous scan finished.
    pub previous_scanned_at_ms: i64,
    pub total_bytes_delta: i64,
    pub file_count_delta: i64,
    /// Every category present in either scan, biggest absolute change
    /// first. A category missing from one scan counts as 0 there.
    pub categories: Vec<CategoryDelta>,
}

/// Compare the current scan with the previous one. `None` when there
/// is no previous scan yet, or when the two scans cover different
/// roots (a root change makes a delta meaningless).
#[tauri::command]
pub fn get_scan_comparison(state: State<'_, ShellState>) -> CmdResult<Option<ScanComparison>> {
    let current = {
        let engine = state
            .engine
            .lock()
            .map_err(|_| "engine lock poisoned".to_string())?;
        engine.summary.clone()
    };
    let previous = state
        .previous_summary
        .lock()
        .map_err(|_| "previous summary lock poisoned".to_string())?
        .clone();
    let (Some(current), Some(previous)) = (current, previous) else {
        return Ok(None);
    };
    if !same_scan_root(&current.root, &previous.root) {
        return Ok(None);
    }

    let mut seen: Vec<Category> = Vec::new();
    for category in current
        .category_totals
        .keys()
        .chain(previous.category_totals.keys())
    {
        if !seen.contains(category) {
            seen.push(*category);
        }
    }
    let mut categories: Vec<CategoryDelta> = seen
        .into_iter()
        .map(|category| {
            let current_bytes = current.category_totals.get(&category).copied().unwrap_or(0);
            let previous_bytes = previous
                .category_totals
                .get(&category)
                .copied()
                .unwrap_or(0);
            CategoryDelta {
                category,
                current_bytes,
                previous_bytes,
                delta_bytes: current_bytes as i64 - previous_bytes as i64,
            }
        })
        .collect();
    categories.sort_by_key(|c| std::cmp::Reverse(c.delta_bytes.unsigned_abs()));

    Ok(Some(ScanComparison {
        previous_scanned_at_ms: previous.finished_at_ms,
        total_bytes_delta: current.total_bytes as i64 - previous.total_bytes as i64,
        file_count_delta: current.file_count as i64 - previous.file_count as i64,
        categories,
    }))
}

/// The scan-history time series (oldest → newest), from
/// `scan_history.json`. Empty when no scan has completed yet.
#[tauri::command]
pub fn get_scan_history(
    state: State<'_, ShellState>,
) -> CmdResult<Vec<crate::scan_history::ScanHistoryEntry>> {
    Ok(crate::scan_history::load(&state))
}

/// Space forecast for the configured scan root: the core's
/// least-squares fit over the scan history for that root. An empty
/// (or too-thin) history yields the zeroed forecast.
#[tauri::command]
pub fn get_space_forecast(state: State<'_, ShellState>) -> CmdResult<Forecast> {
    let root = state
        .settings
        .lock()
        .map_err(|_| "settings lock poisoned".to_string())?
        .scan_root
        .clone();
    let points: Vec<HistoryPoint> = crate::scan_history::load(&state)
        .into_iter()
        .filter(|entry| same_scan_root(&entry.root_path, &root))
        .map(|entry| HistoryPoint {
            scanned_at_ms: entry.scanned_at_ms,
            total_bytes: entry.total_bytes,
            drive_free_bytes: entry.drive_free_bytes,
        })
        .collect();
    Ok(forecast::forecast(&points, now_ms()))
}

/// Biggest directory-level changes between the persisted current and
/// previous snapshots (`snapshot.json` vs `previous_snapshot.json`).
/// All four lists are empty when there is no previous snapshot, no
/// current snapshot, or the two cover different roots (a root change
/// makes a directory diff meaningless, same rule as the comparison).
#[tauri::command]
pub fn get_biggest_changes(
    state: State<'_, ShellState>,
    limit: u32,
) -> CmdResult<BiggestChanges> {
    let limit = if limit == 0 { 8 } else { limit as usize };
    let current = snapshot::load_snapshot(&state.snapshot_file())
        .ok()
        .flatten();
    let previous = snapshot::load_snapshot(&state.previous_snapshot_file())
        .ok()
        .flatten();
    let (Some(current), Some(previous)) = (current, previous) else {
        return Ok(BiggestChanges::default());
    };
    if !same_scan_root(&current.root, &previous.root) {
        return Ok(BiggestChanges::default());
    }
    Ok(diff_snapshots(
        &SnapshotView::from_summary(&current),
        &SnapshotView::from_summary(&previous),
        limit,
    ))
}

/// Whether launch-at-login is currently registered.
#[tauri::command]
pub fn get_autostart() -> bool {
    crate::watch::get_autostart_impl()
}

/// Register or unregister launch-at-login. Resolves with the state
/// now in effect (`false` off Windows, where there is no
/// registration to write).
#[tauri::command]
pub fn set_autostart(enabled: bool) -> CmdResult<bool> {
    crate::watch::set_autostart_impl(enabled)
}

/// Root comparison for the scan-over-scan delta: trailing separators
/// are ignored, and the comparison is case-insensitive on Windows
/// (mirroring how Windows treats paths). Shared with the scheduler
/// and the forecast/history commands.
pub(crate) fn same_scan_root(a: &str, b: &str) -> bool {
    let normalise = |s: &str| {
        let trimmed = s.trim_end_matches(['\\', '/']);
        if cfg!(windows) {
            trimmed.to_lowercase()
        } else {
            trimmed.to_string()
        }
    };
    normalise(a) == normalise(b)
}

/// One entry of a directory listing (shell-side DTO mirroring the core's
/// `explorer::ExplorerEntry`, plus the folder classification when the
/// entry is a directory).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DirectoryEntry {
    pub name: String,
    pub path: String,
    pub bytes: u64,
    pub is_dir: bool,
    pub category: Category,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub classification: Option<Classification>,
}

/// List one directory level of the scanned tree for the Storage
/// explorer, from the scan index (no new filesystem walk).
#[tauri::command]
pub fn list_directory(
    state: State<'_, ShellState>,
    path: String,
) -> CmdResult<Vec<DirectoryEntry>> {
    let engine = state
        .engine
        .lock()
        .map_err(|_| "engine lock poisoned".to_string())?;
    let index = engine
        .index
        .as_ref()
        .ok_or_else(|| "No scan index yet — run a scan first.".to_string())?;
    let entries = explorer::list_directory(index, &path)
        .into_iter()
        .map(|e| {
            let classification = if e.is_dir {
                index.dir_classification.get(&e.path).cloned()
            } else {
                None
            };
            DirectoryEntry {
                name: e.name,
                path: e.path,
                bytes: e.bytes,
                is_dir: e.is_dir,
                category: e.category,
                classification,
            }
        })
        .collect();
    Ok(entries)
}

// ---------------------------------------------------------------------------
// Recommendations & cleanup
// ---------------------------------------------------------------------------

/// Recommendations for the current scan, derived by the core when the
/// scan was stored. Empty until a scan has run in this session (the
/// restored snapshot has no index, so it carries no recommendations).
#[tauri::command]
pub fn get_recommendations(state: State<'_, ShellState>) -> CmdResult<Vec<Recommendation>> {
    let engine = state
        .engine
        .lock()
        .map_err(|_| "engine lock poisoned".to_string())?;
    Ok(engine.recommendations.clone())
}

/// Clean Up card candidates (old installers, forgotten downloads, old
/// screenshots, big old videos) for the current scan, computed by the
/// core when the scan was stored. Empty until a scan has run.
#[tauri::command]
pub fn get_cleanup_cards(
    state: State<'_, ShellState>,
) -> CmdResult<Vec<diskpilot_core::cards::CandidateGroup>> {
    let engine = state
        .engine
        .lock()
        .map_err(|_| "engine lock poisoned".to_string())?;
    Ok(engine.cleanup_cards.clone())
}

/// Build a cleanup plan from Clean Up card candidate IDs only — never
/// paths. The core planner resolves the IDs against the engine's own
/// stored card groups, validates every path through the safety gate,
/// and freezes a single-use plan that expires after 10 minutes. Card
/// plans are Trash-only: the core never allows permanent deletion for
/// them. Execution reuses `execute_cleanup_plan` — card plan IDs live
/// in the same planner as recommendation plan IDs.
#[tauri::command]
pub fn build_card_cleanup_plan(
    state: State<'_, ShellState>,
    candidate_ids: Vec<String>,
) -> CmdResult<CleanupPlan> {
    let ctx = SafetyContext::new(state.scan_root());
    let mut engine = state
        .engine
        .lock()
        .map_err(|_| "engine lock poisoned".to_string())?;
    let groups = engine.cleanup_cards.clone();
    engine
        .planner
        .build_plan_for_candidates(&candidate_ids, &groups, &ctx, now_ms())
        .map_err(err)
}

/// Build a cleanup plan from recommendation IDs only — never paths.
/// The core planner resolves the IDs against its own recommendations,
/// validates every path through the safety gate, and freezes the result
/// into a single-use plan that expires after 10 minutes. The action
/// (Recycle Bin vs. permanent) is chosen at execution time; the plan
/// records whether permanent is allowed at all.
#[tauri::command]
pub fn build_cleanup_plan(
    state: State<'_, ShellState>,
    recommendation_ids: Vec<String>,
) -> CmdResult<CleanupPlan> {
    let ctx = SafetyContext::new(state.scan_root());
    let mut engine = state
        .engine
        .lock()
        .map_err(|_| "engine lock poisoned".to_string())?;
    let recs = engine.recommendations.clone();
    Ok(engine
        .planner
        .build_plan(&recommendation_ids, &recs, &ctx, now_ms()))
}

/// Execute a previously built plan. Taking the plan from the core
/// planner enforces existence, expiry and single use. Every path is
/// re-validated by the core immediately before it is acted on, and the
/// result is recorded to History. `mode` is `"permanent"` or anything
/// else for the default Move to Recycle Bin; the core refuses permanent
/// execution unless the plan allows it.
#[tauri::command]
pub async fn execute_cleanup_plan(
    state: State<'_, ShellState>,
    plan_id: String,
    mode: Option<String>,
) -> CmdResult<CleanupResult> {
    let plan = {
        let mut engine = state
            .engine
            .lock()
            .map_err(|_| "engine lock poisoned".to_string())?;
        engine.planner.take_plan(&plan_id, now_ms()).map_err(err)?
    };
    let mode = match mode.as_deref() {
        Some("permanent") => ExecuteMode::PermanentDelete,
        _ => ExecuteMode::MoveToTrash,
    };
    let ctx = SafetyContext::new(state.scan_root());
    let result = blocking(move || execute_plan(&plan, mode, &ctx, &RealFileOps)).await?;
    let _ = with_history(&state, |history| {
        history
            .record_cleanup("cleanup", &result, Some(&plan_id))
            .map(|_| ())
    });
    Ok(result)
}

/// Cleanup history, newest first.
#[tauri::command]
pub fn get_history(
    state: State<'_, ShellState>,
    limit: Option<usize>,
) -> CmdResult<Vec<HistoryEntry>> {
    with_history(&state, |history| history.list_history(limit.unwrap_or(200)))
}

// ---------------------------------------------------------------------------
// Duplicates
// ---------------------------------------------------------------------------

/// Find duplicate files under `root` (defaults to the scan root):
/// grouped by size, then partial hash, then full SHA-256.
#[tauri::command]
pub async fn find_duplicates(
    state: State<'_, ShellState>,
    root: Option<String>,
) -> CmdResult<Vec<DuplicateGroup>> {
    let root_path = match root {
        Some(r) if !r.is_empty() => r,
        _ => state.scan_root(),
    };
    blocking_value(move || duplicates::find_duplicates(Path::new(&root_path))).await
}

/// Move the non-keeper copies of one duplicate group to the Recycle
/// Bin. Each copy is re-verified byte-for-byte against the keeper
/// immediately before the move (`duplicates::verify_identical`), must
/// pass the safety gate, and the keeper itself is refused. The outcome
/// is recorded to History.
#[tauri::command]
pub async fn remove_duplicate_copies(
    state: State<'_, ShellState>,
    group_hash: String,
    keeper_path: String,
    remove_paths: Vec<String>,
) -> CmdResult<CleanupResult> {
    let ctx = SafetyContext::new(state.scan_root());
    let keeper = keeper_path.clone();
    let group_hash_for_history = group_hash.clone();
    let result = blocking_value(move || {
        let mut succeeded: Vec<String> = Vec::new();
        let mut failed: Vec<CleanupFailure> = Vec::new();
        let mut reclaimed: u64 = 0;
        for path_str in &remove_paths {
            let path = PathBuf::from(path_str);
            let fail = |message: String| CleanupFailure {
                path: path_str.clone(),
                message,
            };
            if path_str == &keeper {
                failed.push(fail("the keeper copy is never removed".to_string()));
                continue;
            }
            if !duplicates::verify_identical(Path::new(&keeper), &path) {
                failed.push(fail(
                    "contents no longer match the keeper — skipped".to_string(),
                ));
                continue;
            }
            if let Err(e) = safety::authorize_destructive(&path, &ctx) {
                failed.push(fail(e.to_string()));
                continue;
            }
            let bytes = scanner::allocated_size_for_path(&path);
            match RealFileOps.move_to_trash(&path) {
                Ok(()) => {
                    succeeded.push(path_str.clone());
                    reclaimed += bytes;
                }
                Err(message) => failed.push(fail(message)),
            }
        }
        CleanupResult {
            plan_id: format!("duplicates-{group_hash}"),
            succeeded,
            failed,
            reclaimed_bytes: reclaimed,
            finished_at_ms: now_ms(),
        }
    })
    .await?;
    let _ = with_history(&state, |history| {
        history
            .record_cleanup("duplicates", &result, Some(&group_hash_for_history))
            .map(|_| ())
    });
    Ok(result)
}

// ---------------------------------------------------------------------------
// Docker
// ---------------------------------------------------------------------------

/// Docker overview as sent to the frontend (shell-owned DTO): every
/// field of the core's `DockerOverview`, plus the Docker Desktop WSL
/// virtual-disk facts the core exposes separately (`docker::vhdx_size`
/// and the compaction steps for that file). The frontend derives its
/// display state from `installed` / `running`. `vhdx_path`,
/// `vhdx_bytes` and `vhdx_compaction_steps` are `None` when no virtual
/// disk exists on this machine — no size is ever invented.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DockerOverviewDto {
    pub installed: bool,
    pub running: bool,
    pub detail: String,
    pub images: Vec<diskpilot_core::model::DockerImage>,
    pub containers: Vec<diskpilot_core::model::DockerContainer>,
    pub volumes: Vec<diskpilot_core::model::DockerVolume>,
    pub images_total_bytes: u64,
    pub images_reclaimable_bytes: u64,
    pub containers_total_bytes: u64,
    pub containers_reclaimable_bytes: u64,
    pub volumes_total_bytes: u64,
    pub volumes_reclaimable_bytes: u64,
    pub build_cache_bytes: u64,
    pub build_cache_reclaimable_bytes: u64,
    pub vhdx_path: Option<String>,
    pub vhdx_bytes: Option<u64>,
    pub vhdx_compaction_steps: Option<Vec<String>>,
}

/// Docker state and usage overview (`docker system df -v` under the
/// hood): not installed / not running / ready, with image, container,
/// volume and build-cache usage, plus the WSL virtual-disk size when
/// Docker Desktop's disk image is present.
#[tauri::command]
pub async fn get_docker_overview() -> CmdResult<DockerOverviewDto> {
    blocking_value(|| {
        let overview = docker::get_overview();
        let home = diskpilot_core::model::home_dir_string();
        let (vhdx_path, vhdx_bytes, vhdx_compaction_steps) =
            match docker::vhdx_size(Path::new(&home)) {
                Some((path, bytes)) => {
                    let path_string = path.to_string_lossy().to_string();
                    let steps = docker::actions::wsl_compaction_steps(&path_string);
                    (Some(path_string), Some(bytes), Some(steps))
                }
                None => (None, None, None),
            };
        DockerOverviewDto {
            installed: overview.installed,
            running: overview.running,
            detail: overview.detail,
            images: overview.images,
            containers: overview.containers,
            volumes: overview.volumes,
            images_total_bytes: overview.images_total_bytes,
            images_reclaimable_bytes: overview.images_reclaimable_bytes,
            containers_total_bytes: overview.containers_total_bytes,
            containers_reclaimable_bytes: overview.containers_reclaimable_bytes,
            volumes_total_bytes: overview.volumes_total_bytes,
            volumes_reclaimable_bytes: overview.volumes_reclaimable_bytes,
            build_cache_bytes: overview.build_cache_bytes,
            build_cache_reclaimable_bytes: overview.build_cache_reclaimable_bytes,
            vhdx_path,
            vhdx_bytes,
            vhdx_compaction_steps,
        }
    })
    .await
}

/// A Docker action as sent by the frontend (shell-side DTO; the core's
/// `DockerAction` is a closed enum and is constructed here).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DockerActionRequest {
    /// `"removeStoppedContainer"`, `"removeUnusedImage"` or
    /// `"pruneBuildCache"`.
    pub kind: String,
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub reference: Option<String>,
}

fn to_docker_action(request: &DockerActionRequest) -> CmdResult<docker::DockerAction> {
    match request.kind.as_str() {
        "removeStoppedContainer" => Ok(docker::DockerAction::RemoveStoppedContainer {
            id: request
                .id
                .clone()
                .ok_or_else(|| "container id is required".to_string())?,
            name: request.name.clone().unwrap_or_default(),
        }),
        "removeUnusedImage" => Ok(docker::DockerAction::RemoveUnusedImage {
            id: request
                .id
                .clone()
                .ok_or_else(|| "image id is required".to_string())?,
            reference: request.reference.clone().unwrap_or_default(),
        }),
        "pruneBuildCache" => Ok(docker::DockerAction::PruneBuildCache),
        other => Err(format!("unknown Docker action: {other}")),
    }
}

/// Preview of a Docker action: the exact command and what it does.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DockerActionPreview {
    pub command: String,
    pub description: String,
}

/// Preview one Docker action. Nothing is executed; the action is also
/// re-checked against a fresh overview so a stale preview fails here,
/// not at execution time.
#[tauri::command]
pub async fn docker_preview_action(action: DockerActionRequest) -> CmdResult<DockerActionPreview> {
    let action = to_docker_action(&action)?;
    let preview = DockerActionPreview {
        command: action.preview_command(),
        description: action.describe(),
    };
    let overview = blocking_value(docker::get_overview).await?;
    action.recheck(&overview).map_err(err)?;
    Ok(preview)
}

/// Result of an executed Docker action.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DockerActionResult {
    pub command: String,
    pub output: String,
}

/// Execute one Docker action. The core re-checks it against a fresh
/// overview and its argument-safety invariants before running anything.
#[tauri::command]
pub async fn docker_execute_action(action: DockerActionRequest) -> CmdResult<DockerActionResult> {
    let action = to_docker_action(&action)?;
    let command = action.preview_command();
    let output = blocking_value(move || docker::execute_action(&action)).await??;
    Ok(DockerActionResult { command, output })
}

// ---------------------------------------------------------------------------
// Developer tools, applications, secrets
// ---------------------------------------------------------------------------

/// Per-tool storage reports for the Developer Tools page.
#[tauri::command]
pub async fn get_devtools_reports() -> CmdResult<Vec<ToolReport>> {
    blocking_value(|| devtools::all_reports(&devtools::DevPaths::from_env())).await
}

/// Installed applications with install size and AppData footprint,
/// including leftover data folders from uninstalled apps.
#[tauri::command]
pub async fn get_applications() -> CmdResult<Vec<AppEntry>> {
    blocking_value(apps::get_applications).await
}

/// Scan AI-assistant transcripts on this machine (Claude Code, Codex,
/// Gemini CLI locations under the profile) for known credential
/// formats. Fully offline; findings are masked and nothing is modified.
#[tauri::command]
pub async fn scan_transcript_secrets() -> CmdResult<Vec<SecretFinding>> {
    blocking_value(|| {
        let home = PathBuf::from(diskpilot_core::model::home_dir_string());
        let mut findings = Vec::new();
        for dir in [".claude", ".codex", ".gemini"] {
            let root = home.join(dir);
            if root.is_dir() {
                findings.extend(secrets::scan_directory(&root));
            }
        }
        findings
    })
    .await
}

// ---------------------------------------------------------------------------
// Recycle Bin
// ---------------------------------------------------------------------------

/// Current Recycle Bin contents (what an empty would remove), read from
/// the `$Recycle.Bin` metadata — see `crate::recycle_bin`.
#[tauri::command]
pub async fn get_recycle_bin_items() -> CmdResult<Vec<RecycleBinItem>> {
    blocking_value(crate::recycle_bin::list_items).await
}

/// Remove exactly `items` (as returned by `get_recycle_bin_items`), one
/// by one, emitting `recycle-bin://progress`. Each item ID is
/// re-validated against the `$Recycle.Bin` layout before its payload is
/// removed; the paired `$I…` metadata is removed with it. Stop with
/// `cancel_empty_recycle_bin`. The result is recorded to History.
#[tauri::command]
pub async fn empty_recycle_bin(
    app: AppHandle,
    state: State<'_, ShellState>,
    items: Vec<RecycleBinItem>,
) -> CmdResult<CleanupResult> {
    state.recycle_bin_cancel.store(false, Ordering::SeqCst);
    let cancel = state.recycle_bin_cancel.clone();
    let total = items.len();
    let result = blocking_value(move || {
        let mut succeeded: Vec<String> = Vec::new();
        let mut failed: Vec<CleanupFailure> = Vec::new();
        let mut freed: u64 = 0;
        for (i, item) in items.iter().enumerate() {
            if cancel.load(Ordering::SeqCst) {
                for remaining in &items[i..] {
                    failed.push(CleanupFailure {
                        path: remaining.original_path.clone(),
                        message: "stopped by user".to_string(),
                    });
                }
                break;
            }
            let fail = |message: String| CleanupFailure {
                path: item.original_path.clone(),
                message,
            };
            match crate::recycle_bin::validate_item_id(&item.id) {
                Some((payload, info)) => {
                    let outcome = RealFileOps
                        .permanent_delete(&payload)
                        .and_then(|_| std::fs::remove_file(&info).map_err(|e| e.to_string()));
                    match outcome {
                        Ok(()) => {
                            succeeded.push(item.original_path.clone());
                            freed += item.bytes;
                        }
                        Err(message) => failed.push(fail(message)),
                    }
                }
                None => failed.push(fail(
                    "item is no longer in the Recycle Bin — skipped".to_string(),
                )),
            }
            let _ = app.emit(
                "recycle-bin://progress",
                RecycleBinProgressPayload {
                    completed: i + 1,
                    total,
                    current_name: item.name.clone(),
                    freed_bytes: freed,
                },
            );
        }
        CleanupResult {
            plan_id: "recycle-bin".to_string(),
            succeeded,
            failed,
            reclaimed_bytes: freed,
            finished_at_ms: now_ms(),
        }
    })
    .await?;
    let _ = with_history(&state, |history| {
        history
            .record_cleanup("empty-recycle-bin", &result, None)
            .map(|_| ())
    });
    Ok(result)
}

/// Ask a running Empty Recycle Bin operation to stop after the current
/// item.
#[tauri::command]
pub fn cancel_empty_recycle_bin(state: State<'_, ShellState>) -> CmdResult<()> {
    state.recycle_bin_cancel.store(true, Ordering::SeqCst);
    Ok(())
}

/// Restore one Recycle Bin item (by the id `get_recycle_bin_items`
/// returned) to its original location. Refuses when the destination
/// already exists; recreates the original folder when it is gone.
#[tauri::command]
pub async fn restore_recycle_item(id: String) -> CmdResult<()> {
    blocking_value(move || crate::recycle_bin::restore_item(&id)).await?
}

// ---------------------------------------------------------------------------
// Cloud storage & reports
// ---------------------------------------------------------------------------

/// The OneDrive folder on this PC and how much of it the current scan
/// found stored locally (files occupying allocated bytes). `None`
/// when no OneDrive folder exists or no scan has run yet — the status
/// is computed from the scan index, never by re-walking the folder.
#[tauri::command]
pub fn get_cloud_status(
    state: State<'_, ShellState>,
) -> CmdResult<Option<diskpilot_core::cloud::CloudStatus>> {
    let Some(root) = diskpilot_core::cloud::detect_onedrive_root() else {
        return Ok(None);
    };
    let engine = state
        .engine
        .lock()
        .map_err(|_| "engine lock poisoned".to_string())?;
    let Some(index) = engine.index.as_ref() else {
        return Ok(None);
    };
    let root = root.to_string_lossy().into_owned();
    Ok(Some(diskpilot_core::cloud::cloud_status(&root, &index.files)))
}

/// Media totals (photos / videos / screenshots + biggest files)
/// over the current scan index, classified with the same kind rules
/// as natural-language search. `None` when no scan has run yet.
#[tauri::command]
pub fn get_media_summary(
    state: State<'_, ShellState>,
) -> CmdResult<Option<diskpilot_core::media::MediaSummary>> {
    let engine = state
        .engine
        .lock()
        .map_err(|_| "engine lock poisoned".to_string())?;
    let Some(index) = engine.index.as_ref() else {
        return Ok(None);
    };
    Ok(Some(diskpilot_core::media::media_summary(&index.files)))
}

/// Installed Steam and Epic games with their disk usage, read from
/// the launchers' manifest files at call time — no scan needed. A
/// machine with neither launcher gets an empty report, not an error.
/// Runs on a blocking thread: a manifest without a recorded size
/// falls back to measuring the game's install folder.
#[tauri::command]
pub async fn get_games() -> CmdResult<diskpilot_core::games::GamesReport> {
    tauri::async_runtime::spawn_blocking(diskpilot_core::games::games_report)
        .await
        .map_err(|e| format!("background task failed: {e}"))
}

/// Ask OneDrive to free up the local copies of `paths` (Files
/// On-Demand dehydration — nothing is deleted). The core refuses the
/// whole call when any path sits outside the OneDrive folder.
#[tauri::command]
pub async fn free_up_space(
    paths: Vec<String>,
) -> CmdResult<diskpilot_core::cloud::FreeUpSpaceResult> {
    tauri::async_runtime::spawn_blocking(move || diskpilot_core::cloud::free_up_space(&paths))
        .await
        .map_err(|e| format!("background task failed: {e}"))?
}

/// Save an HTML storage report through the native save dialog
/// (default name `DiskPilot-Report.html`). Resolves with the chosen
/// path, or `None` when the user cancelled. The dialog runs on the
/// Rust side, so no extra capability permission is involved.
#[tauri::command]
pub async fn export_report(app: AppHandle, html: String) -> CmdResult<Option<String>> {
    use tauri_plugin_dialog::DialogExt;
    blocking_value(move || -> Result<Option<String>, String> {
        let Some(file) = app
            .dialog()
            .file()
            .set_file_name("DiskPilot-Report.html")
            .add_filter("HTML", &["html"])
            .blocking_save_file()
        else {
            return Ok(None);
        };
        let path = file.into_path().map_err(|e| e.to_string())?;
        std::fs::write(&path, html.as_bytes()).map_err(|e| e.to_string())?;
        Ok(Some(path.to_string_lossy().into_owned()))
    })
    .await?
}

// ---------------------------------------------------------------------------
// Downloads & provenance
// ---------------------------------------------------------------------------

/// The user's Downloads folder, derived from the profile layout the
/// same way the core derives its per-user locations from the home
/// directory. The Downloads page and the tray's Recent downloads
/// submenu both read from here.
pub(crate) fn downloads_dir() -> PathBuf {
    PathBuf::from(diskpilot_core::model::home_dir_string()).join("Downloads")
}

/// The most recent files in the Downloads folder (newest first, at
/// most 25), each with the download source Windows recorded for it
/// when there is one. An absent Downloads folder reads as an empty
/// list — the page shows its empty state.
#[tauri::command]
pub async fn get_recent_downloads() -> CmdResult<Vec<provenance::RecentFile>> {
    blocking_value(|| {
        let dir = downloads_dir();
        if !dir.is_dir() {
            return Vec::new();
        }
        provenance::recent_downloads(&dir, 25)
    })
    .await
}

/// Downloads grouped by source: the zone data Windows writes on
/// downloaded files (Zone.Identifier) names the host each file came
/// from, and the core groups the folder by it. Platforms without zone
/// data return no groups — provenance is never guessed.
#[tauri::command]
pub async fn get_download_provenance() -> CmdResult<Vec<provenance::ProvenanceGroup>> {
    blocking_value(|| {
        let dir = downloads_dir();
        if !dir.is_dir() {
            return Vec::new();
        }
        provenance::scan_provenance(&dir)
    })
    .await
}

/// Image metadata for one file (pixel dimensions, camera make/model,
/// capture time) read from the file's EXIF block. `None` when the
/// file has no readable metadata.
#[tauri::command]
pub async fn get_media_info(path: String) -> CmdResult<Option<media_meta::MediaInfo>> {
    blocking_value(move || media_meta::media_info(Path::new(&path))).await
}

// ---------------------------------------------------------------------------
// Natural-language search over the scan index
// ---------------------------------------------------------------------------

/// Hits kept in a single response; `total_matches` always reports the
/// full count so the UI can say "the first 500 of N".
const SEARCH_HIT_LIMIT: usize = 500;

/// Response of `search_files`: how the query was understood (chips),
/// the parsed filters echoed back as JSON (so the client can drop one
/// dimension and re-run through `run_search_filters` without the
/// shell re-parsing anything), and the first page of hits.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResponse {
    pub chips: Vec<search::QueryChip>,
    pub filters: serde_json::Value,
    pub hits: Vec<search::SearchHit>,
    pub total_matches: usize,
}

/// Response of `run_search_filters`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchHitsResponse {
    pub hits: Vec<search::SearchHit>,
    pub total_matches: usize,
}

/// The scan index a search runs over. Searching needs a scan in this
/// session (the index is not persisted across restarts), like the
/// explorer.
fn search_index(state: &ShellState) -> CmdResult<scanner::DirIndex> {
    let engine = state
        .engine
        .lock()
        .map_err(|_| "engine lock poisoned".to_string())?;
    engine
        .index
        .clone()
        .ok_or_else(|| "No scan index yet — run a scan first.".to_string())
}

/// Apply the one dimension the index cannot know about: when the
/// query named a source, keep only hits whose recorded download
/// source matches it. Reading a file's zone data is one probe per
/// hit, so this runs only when the hit set is already small; a broad
/// result set keeps its hits unfiltered rather than probing
/// thousands of files.
fn apply_source_filter(hits: Vec<search::SearchHit>, source: &str) -> Vec<search::SearchHit> {
    if hits.len() > SEARCH_HIT_LIMIT {
        return hits;
    }
    let needle = source.to_lowercase();
    hits.into_iter()
        .filter(|hit| {
            provenance::download_source(Path::new(&hit.path))
                .map(|recorded| recorded.label.to_lowercase().contains(&needle))
                .unwrap_or(false)
        })
        .collect()
}

/// Run parsed filters over the current index, applying the source
/// filter in the shell and capping the transported hits.
async fn execute_search(
    state: &ShellState,
    filters: search::SearchFilters,
) -> CmdResult<SearchHitsResponse> {
    let index = search_index(state)?;
    let source = filters
        .source
        .clone()
        .filter(|value| !value.trim().is_empty());
    let mut hits = blocking_value(move || search::run_search(&index, &filters)).await?;
    if let Some(source) = source {
        hits = blocking_value(move || apply_source_filter(hits, &source)).await?;
    }
    let total_matches = hits.len();
    hits.truncate(SEARCH_HIT_LIMIT);
    Ok(SearchHitsResponse {
        hits,
        total_matches,
    })
}

/// Parse a natural-language query ("pdfs from last week over 5 MB")
/// with the core parser and run it over the current scan index. The
/// response's chips are the parser's own reading of the query, shown
/// verbatim so the user can see — and correct — how it was understood.
#[tauri::command]
pub async fn search_files(
    state: State<'_, ShellState>,
    query: String,
) -> CmdResult<SearchResponse> {
    let parsed = search::parse_query(&query, now_ms());
    let filters_json = serde_json::to_value(&parsed.filters)
        .map_err(|e| format!("the parsed search could not be prepared: {e}"))?;
    let result = execute_search(&state, parsed.filters).await?;
    Ok(SearchResponse {
        chips: parsed.chips,
        filters: filters_json,
        hits: result.hits,
        total_matches: result.total_matches,
    })
}

/// Re-run a search from filters the client adjusted (a chip removed
/// in the "Understood as" row). The filters are the exact JSON
/// `search_files` returned, deserialized back into the core's filter
/// type — the shell never interprets them field by field.
#[tauri::command]
pub async fn run_search_filters(
    state: State<'_, ShellState>,
    filters: serde_json::Value,
) -> CmdResult<SearchHitsResponse> {
    let filters: search::SearchFilters = serde_json::from_value(filters)
        .map_err(|e| format!("those search filters could not be read: {e}"))?;
    execute_search(&state, filters).await
}

/// Look-alike photo groups under the scan root: perceptual hashes in
/// the core group near-identical images (re-saves, resized copies,
/// burst neighbours). Deliberately read-only in the UI — only exact
/// copies go through the duplicate cleanup flow, where every copy is
/// re-verified byte-for-byte; similar images have no such proof, so
/// they are presented for review with Reveal actions only.
#[tauri::command]
pub async fn get_similar_images(
    state: State<'_, ShellState>,
) -> CmdResult<Vec<duplicates::SimilarGroup>> {
    let root_path = state.scan_root();
    blocking_value(move || duplicates::find_similar_images(Path::new(&root_path))).await
}

// ---------------------------------------------------------------------------
// Assistant
// ---------------------------------------------------------------------------

/// One chat message, as sent by the frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatMessageDto {
    /// `"user"` or `"assistant"`.
    pub role: String,
    pub content: String,
}

/// The assistant's reply.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssistantReply {
    pub content: String,
    /// Recommendations the assistant highlighted for user review.
    pub highlighted_recommendation_ids: Vec<String>,
}

fn provider_from_str(value: &str) -> CmdResult<Provider> {
    match value {
        "anthropic" => Ok(Provider::Anthropic),
        "openai" => Ok(Provider::OpenAI),
        "openrouter" => Ok(Provider::OpenRouter),
        other => Err(format!("unknown assistant provider: {other}")),
    }
}

/// Ask the assistant a question. The assistant is read-only: its fixed
/// tool set runs over the current scan data (paths shown with home as
/// `~`, private folders only as totals, file contents never read), and
/// it can only highlight existing recommendations. The API key comes
/// from the credential store at call time and is never exposed to the
/// webview; requests go directly to the provider over HTTPS with no
/// redirects (the core's HTTP client enforces both).
#[tauri::command]
pub async fn assistant_ask(
    state: State<'_, ShellState>,
    message: String,
    history: Vec<ChatMessageDto>,
) -> CmdResult<AssistantReply> {
    let (enabled, provider_name) = {
        let settings = state
            .settings
            .lock()
            .map_err(|_| "settings lock poisoned".to_string())?;
        (
            settings.assistant_enabled,
            settings.assistant_provider.clone(),
        )
    };
    if !enabled {
        return Err("The assistant is disabled. Enable it in Settings.".to_string());
    }
    let provider = provider_from_str(&provider_name)?;
    let info = assistant::provider::provider_info(provider);
    let api_key = state
        .key_store
        .load(provider)?
        .ok_or_else(|| format!("No API key saved for {}. Add one in Settings.", info.label))?;

    // Snapshot the engine data so the tool context can borrow owned
    // values inside the blocking worker.
    let (summary, index, recommendations) = {
        let engine = state
            .engine
            .lock()
            .map_err(|_| "engine lock poisoned".to_string())?;
        (
            engine.summary.clone(),
            engine.index.clone(),
            engine.recommendations.clone(),
        )
    };
    let home = diskpilot_core::model::home_dir_string();
    let prior: Vec<CoreChatMessage> = history
        .into_iter()
        .map(|m| CoreChatMessage {
            role: m.role,
            content: m.content,
        })
        .collect();
    let model = info.default_model.to_string();

    let reply = tauri::async_runtime::spawn_blocking(move || {
        let context = ToolContext {
            summary: summary.as_ref(),
            index: index.as_ref(),
            recommendations: &recommendations,
            docker: None,
            devtools: &[],
            home,
        };
        let mut session = AssistantSession::new(provider, context);
        session.messages = prior;
        let mut client = HttpAssistantClient {
            provider,
            api_key,
            model,
        };
        let text = session.ask(&mut client, &message)?;
        Ok::<AssistantReply, String>(AssistantReply {
            content: text,
            highlighted_recommendation_ids: session.highlighted_recommendations.clone(),
        })
    })
    .await
    .map_err(|e| format!("assistant task failed: {e}"))??;
    Ok(reply)
}

/// Store an assistant provider API key in Windows Credential Manager.
/// Keys are sanity-checked against the provider's known prefix first.
#[tauri::command]
pub fn save_api_key(state: State<'_, ShellState>, provider: String, key: String) -> CmdResult<()> {
    let provider = provider_from_str(&provider)?;
    if !assistant::provider::key_looks_valid(provider, &key) {
        let info = assistant::provider::provider_info(provider);
        return Err(format!(
            "That does not look like a {} API key (expected the {}… prefix).",
            info.label, info.key_prefix_hint
        ));
    }
    state.key_store.save(provider, &key)
}

/// Whether an API key is stored for `provider`. Never returns the key.
#[tauri::command]
pub fn has_api_key(state: State<'_, ShellState>, provider: String) -> bool {
    match provider_from_str(&provider) {
        Ok(provider) => state.key_store.has(provider),
        Err(_) => false,
    }
}

/// Delete the stored API key for `provider`.
#[tauri::command]
pub fn delete_api_key(state: State<'_, ShellState>, provider: String) -> CmdResult<()> {
    let provider = provider_from_str(&provider)?;
    state.key_store.delete(provider)
}

// ---------------------------------------------------------------------------
// Settings
// ---------------------------------------------------------------------------

/// Current settings.
#[tauri::command]
pub fn get_settings(state: State<'_, ShellState>) -> CmdResult<Settings> {
    let settings = state
        .settings
        .lock()
        .map_err(|_| "settings lock poisoned".to_string())?;
    Ok(settings.clone())
}

/// Replace and persist settings.
#[tauri::command]
pub fn save_settings(state: State<'_, ShellState>, settings: Settings) -> CmdResult<()> {
    crate::persist_settings(&state.app_data_dir, &settings)
        .map_err(|e| format!("could not save settings: {e}"))?;
    let mut slot = state
        .settings
        .lock()
        .map_err(|_| "settings lock poisoned".to_string())?;
    *slot = settings;
    Ok(())
}

// ---------------------------------------------------------------------------
// Shell integration
// ---------------------------------------------------------------------------

/// Case-insensitive containment check used by Reveal (Windows paths are
/// case-insensitive; on other platforms the comparison is exact).
fn is_inside_root(path: &str, root: &str) -> bool {
    let normalise = |s: &str| {
        let trimmed = s.trim_end_matches(['\\', '/']);
        if cfg!(windows) {
            trimmed.to_lowercase()
        } else {
            trimmed.to_string()
        }
    };
    let (path, root) = (normalise(path), normalise(root));
    path == root || path.starts_with(&format!("{root}\\")) || path.starts_with(&format!("{root}/"))
}

/// Reveal `path` in Windows Explorer (`explorer.exe /select,`, with the
/// path passed as a separate argument). The target must be the scanned
/// root or inside it (see the module header for why this is a
/// containment check rather than the destructive-op safety gate).
#[tauri::command]
pub fn reveal_in_explorer(state: State<'_, ShellState>, path: String) -> CmdResult<()> {
    if path.contains('\0') || path.split(['\\', '/']).any(|c| c == "..") {
        return Err("Invalid path.".to_string());
    }
    let root = state.scan_root();
    if !is_inside_root(&path, &root) {
        return Err("Only locations inside the scanned root can be revealed.".to_string());
    }
    reveal_path_in_explorer(&PathBuf::from(&path))
}

/// Open the platform file manager with `path` selected (or its parent
/// folder opened, where selection is unavailable). Shared by
/// `reveal_in_explorer` — which gates webview-supplied paths with the
/// containment check above — and the tray's Recent downloads submenu,
/// whose paths come from the core's own scan of the user's Downloads
/// folder rather than from the webview.
pub(crate) fn reveal_path_in_explorer(path: &Path) -> CmdResult<()> {
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer.exe")
            .arg("/select,")
            .arg(path)
            .spawn()
            .map_err(|e| format!("could not open Explorer: {e}"))?;
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg("-R")
            .arg(path)
            .spawn()
            .map_err(|e| format!("could not open Finder: {e}"))?;
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let target = if path.is_dir() {
            path.to_path_buf()
        } else {
            path.parent()
                .map(|p| p.to_path_buf())
                .unwrap_or_else(|| path.to_path_buf())
        };
        std::process::Command::new("xdg-open")
            .arg(&target)
            .spawn()
            .map_err(|e| format!("could not open the file manager: {e}"))?;
    }
    Ok(())
}

/// Open an external URL with the system handler. Only `https:`, `http:`
/// and `mailto:` URLs are accepted.
#[tauri::command]
pub fn open_external(app: AppHandle, url: String) -> CmdResult<()> {
    if !(url.starts_with("https://") || url.starts_with("http://") || url.starts_with("mailto:")) {
        return Err("Only web and mail links can be opened.".to_string());
    }
    app.opener()
        .open_url(&url, None::<&str>)
        .map_err(|e| format!("could not open link: {e}"))
}
