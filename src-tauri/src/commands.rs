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
//! - Recycle Bin listing/emptying lives in `crate::recycle_bin`: the
//!   core has no Recycle Bin *inventory* API (its safety gate protects
//!   `$Recycle.Bin` from plan-based cleanup). The shell parses the
//!   `$I…` metadata read-only and, on empty, removes exactly the listed
//!   `$R…` payloads after re-validating each ID.
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
use diskpilot_core::history::HistoryStore;
use diskpilot_core::model::{
    now_ms, AppEntry, Category, Classification, CleanupFailure, CleanupPlan, CleanupResult,
    DiskInfo, DuplicateGroup, HistoryEntry, Recommendation, ScanSummary, SecretFinding, ToolReport,
};
use diskpilot_core::safety::SafetyContext;
use diskpilot_core::scanner::{ProgressCallback, ScanProgress, Scanner};
use diskpilot_core::{
    apps, assistant, devtools, disk, docker, duplicates, explorer, safety, scanner, secrets,
    snapshot,
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

// ---------------------------------------------------------------------------
// Scanning
// ---------------------------------------------------------------------------

/// Start a scan of `root` (defaults to the configured scan root).
///
/// Emits `scan://progress` (throttled by the core to ~120 ms) and
/// `scan://finished` with the final summary; resolves with the summary.
/// Only one scan runs at a time. The finished scan is recorded to
/// History and persisted as the launch snapshot.
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
        .with_progress(callback);
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

    let _ = snapshot::save_snapshot(&summary, &state.snapshot_file());
    let _ = with_history(&state, |history| history.record_scan(&summary).map(|_| ()));

    // Keep the tray ring honest about the scanned volume.
    let info = disk::disk_info_for(Path::new(&summary.root));
    crate::tray::update_tray(&app, &info);

    let _ = app.emit("scan://finished", summary.clone());
    Ok(summary)
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
            .record_cleanup("duplicates", &result, Some(&group_hash))
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
    let output = blocking_value(move || docker::execute_action(&action))
        .await?
        .map_err(|e| e)?;
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
        .load(provider)
        .map_err(|e| e)?
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
    state.key_store.save(provider, &key).map_err(|e| e)
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
    state.key_store.delete(provider).map_err(|e| e)
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
    let validated = PathBuf::from(&path);

    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer.exe")
            .arg("/select,")
            .arg(&validated)
            .spawn()
            .map_err(|e| format!("could not open Explorer: {e}"))?;
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg("-R")
            .arg(&validated)
            .spawn()
            .map_err(|e| format!("could not open Finder: {e}"))?;
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let target = if validated.is_dir() {
            validated.clone()
        } else {
            validated
                .parent()
                .map(|p| p.to_path_buf())
                .unwrap_or(validated.clone())
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
