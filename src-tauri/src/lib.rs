//! DiskPilot desktop shell.
//!
//! The shell is deliberately thin: it owns the window, the system tray,
//! persisted settings and the bridge state between the typed Tauri
//! commands and the `diskpilot-core` engine. All analysis and every
//! destructive operation live in the core crate behind its safety gate;
//! the webview itself has no filesystem or shell access.

mod commands;
mod live;
mod maintenance;
mod ocr;
mod recycle_bin;
mod relocate_io;
mod scan_history;
mod smart_io;
mod startup;
mod tray;
mod tray_icon;
mod watch;
mod winget_io;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use tauri::{Emitter, Manager};

use diskpilot_core::assistant::KeyStore;
use diskpilot_core::history::HistoryStore;
use diskpilot_core::model::ScanSummary;
use diskpilot_core::snapshot;
use diskpilot_core::AppState;

/// User preferences, persisted as JSON at `<app data>/settings.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    /// Folder the scanner starts from (default: the user profile folder).
    pub scan_root: String,
    /// `"system"`, `"light"` or `"dark"`.
    pub theme: String,
    /// The Assistant is optional and disabled by default.
    pub assistant_enabled: bool,
    /// `"anthropic"`, `"openai"` or `"openrouter"`.
    pub assistant_provider: String,
    /// Folders and patterns excluded from scans, matched by the core
    /// scanner (a folder or file name, or a wildcard like `*.iso`).
    /// `#[serde(default)]` keeps settings files written before this
    /// field existed loading unchanged.
    #[serde(default)]
    pub exclusions: Vec<String>,
    /// Natural-language searches the user bookmarked from the Storage
    /// page, oldest first. Same `#[serde(default)]` treatment.
    #[serde(default)]
    pub saved_searches: Vec<String>,
    /// Scheduled scans: `"off"` (default), `"daily"` or `"weekly"`.
    /// The scheduler in `crate::watch` runs due scans in the
    /// background and raises the alerts below after them.
    #[serde(default = "default_auto_scan")]
    pub auto_scan: String,
    /// After a scheduled scan, notify when the scanned drive's free
    /// space drops below this many GB (decimal).
    #[serde(default = "default_low_space_alert_gb")]
    pub low_space_alert_gb: f64,
    /// After a scheduled scan, notify when total usage grew by more
    /// than this many GB (decimal) since the previous scan.
    #[serde(default = "default_growth_alert_gb")]
    pub growth_alert_gb: f64,
    /// Launch DiskPilot at login (per-user Run key on Windows).
    /// Applied by `set_autostart` when the setting changes and
    /// re-applied at every startup, so a registry entry lost outside
    /// the app self-heals.
    #[serde(default)]
    pub auto_start: bool,
    /// Whether the first-run onboarding has been completed. The
    /// frontend sets it once the welcome flow finishes (or is
    /// dismissed); `#[serde(default)]` keeps older settings files —
    /// written before onboarding existed — at `false`.
    #[serde(default)]
    pub onboarded: bool,
    /// Recognize text in screenshots and images after each scan, so
    /// file search can match words that appear only inside a picture
    /// (see `crate::ocr`). On by default; the engine is on-device
    /// (Windows' built-in OCR) and the pass is fully cached.
    #[serde(default = "default_ocr_enabled")]
    pub ocr_enabled: bool,
    /// Keep the in-memory index fresh between scans by following
    /// the NTFS USN change journal while DiskPilot runs (see
    /// `crate::live`). On by default; off means no polling and no
    /// journal checkpoints are kept. The feature quietly stays
    /// inactive when the volume cannot be opened (it needs the
    /// same administrator volume access as the MFT scan path).
    #[serde(default = "default_live_index")]
    pub live_index: bool,
    /// Automatic maintenance (see `crate::maintenance`): when
    /// enabled, the watch scheduler periodically purges Recycle Bin
    /// items older than the retention age and clears the scan's
    /// temporary-files recommendation through the ordinary planner
    /// gate. `#[serde(default)]` keeps older settings files loading
    /// with maintenance off.
    #[serde(default)]
    pub auto_maintenance: AutoMaintenanceSettings,
    /// When the last automatic-maintenance run finished (epoch ms);
    /// the due check in the core is computed against this stamp.
    #[serde(default)]
    pub last_maintenance_ms: Option<i64>,
}

/// Automatic-maintenance preferences, persisted inside
/// [`Settings`]. Serialized camelCase like the rest of the settings
/// payload: `{ enabled, frequency: "daily" | "weekly", minAgeDays }`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoMaintenanceSettings {
    /// Off by default: nothing is ever removed automatically until
    /// the user turns maintenance on.
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_maintenance_frequency")]
    pub frequency: diskpilot_core::maintenance::MaintenanceFrequency,
    /// Recycle Bin items deleted longer ago than this are purged;
    /// the core floors the value at 1 day at selection time.
    #[serde(default = "default_maintenance_min_age_days")]
    pub min_age_days: u32,
}

fn default_maintenance_frequency() -> diskpilot_core::maintenance::MaintenanceFrequency {
    diskpilot_core::maintenance::MaintenanceFrequency::Weekly
}

fn default_maintenance_min_age_days() -> u32 {
    30
}

impl Default for AutoMaintenanceSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            frequency: default_maintenance_frequency(),
            min_age_days: default_maintenance_min_age_days(),
        }
    }
}

fn default_auto_scan() -> String {
    "off".to_string()
}

fn default_live_index() -> bool {
    true
}

fn default_ocr_enabled() -> bool {
    true
}

fn default_low_space_alert_gb() -> f64 {
    10.0
}

fn default_growth_alert_gb() -> f64 {
    5.0
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            scan_root: home_dir_string(),
            theme: "system".to_string(),
            assistant_enabled: false,
            assistant_provider: "anthropic".to_string(),
            exclusions: Vec::new(),
            saved_searches: Vec::new(),
            auto_scan: default_auto_scan(),
            low_space_alert_gb: default_low_space_alert_gb(),
            growth_alert_gb: default_growth_alert_gb(),
            auto_start: false,
            onboarded: false,
            ocr_enabled: default_ocr_enabled(),
            live_index: default_live_index(),
            auto_maintenance: AutoMaintenanceSettings::default(),
            last_maintenance_ms: None,
        }
    }
}

/// Shared state for the shell, managed by Tauri and reached from commands.
pub struct ShellState {
    /// Application data directory (settings, SQLite history, snapshot).
    pub app_data_dir: PathBuf,
    /// Current settings (mirrored to `settings.json` on every save).
    pub settings: Mutex<Settings>,
    /// The core engine state: last scan summary + directory index,
    /// derived recommendations, the cleanup planner (single-use,
    /// 10-minute plans) and the scan cancellation flag. At launch the
    /// summary is restored from the snapshot; the index is not
    /// persisted, so the explorer and recommendations need a fresh scan
    /// after a restart.
    pub engine: Mutex<AppState>,
    /// The scan before the current one, restored from
    /// `previous_snapshot.json` at launch and rotated forward on every
    /// completed scan. Powers the scan-over-scan comparison; `None`
    /// until two scans of the same root exist.
    pub previous_summary: Mutex<Option<ScanSummary>>,
    /// Assistant API-key store (Windows Credential Manager via the
    /// core's keyring-backed store). Keys can be saved, checked and
    /// deleted through commands — never read back into the webview.
    pub key_store: Box<dyn KeyStore + Send + Sync>,
    /// Cancellation flag for a running Empty Recycle Bin operation.
    pub recycle_bin_cancel: Arc<AtomicBool>,
    /// SQLite history store, opened lazily on first use.
    pub history: Mutex<Option<HistoryStore>>,
    /// Scan generation for the OCR pass: bumped every time a scan
    /// starts, so a pass still running from the previous scan stops
    /// cooperatively instead of writing into the new scan's index.
    pub ocr_generation: Arc<AtomicU64>,
    /// Live-index runtime status (stale flag + last-poll outcome);
    /// the durable journal checkpoints live on disk (see
    /// `crate::live`).
    pub live: Mutex<live::LiveStatus>,
    /// The duplicate groups most recently served by
    /// `find_duplicates`. `remove_duplicate_copies` authorizes
    /// against THIS report — keeper and copies must be members of
    /// the named group — never against paths the webview supplies
    /// freehand.
    pub duplicates_report: Mutex<Option<Vec<diskpilot_core::model::DuplicateGroup>>>,
    /// The exclusion patterns the last completed scan actually ran
    /// with, captured at scan start. The live index applies journal
    /// changes against this snapshot, not the current settings:
    /// patterns edited after the scan must not retroactively change
    /// what the existing index means (see `crate::live`).
    pub scan_exclusions: Mutex<Vec<String>>,
    /// Bumped every time the in-memory index is wholesale replaced
    /// (a scan stores its result; a live batch installs its
    /// rebuild). The live applier snapshots the index under the
    /// engine lock tagged with this generation and installs its
    /// result only when the generation is unchanged, so a scan that
    /// completes mid-apply is never overwritten by stale work.
    pub index_generation: Arc<AtomicU64>,
    /// A folder the user asked Windows Explorer to analyze
    /// (`DiskPilot --scan <path>`, from the context-menu entry):
    /// seeded from the process arguments at launch and replaced by
    /// the single-instance handoff on later launches. The frontend
    /// claims it once via `get_launch_intent`.
    pub launch_intent: Mutex<Option<String>>,
}

impl ShellState {
    /// Path of the settings file inside the app data directory.
    pub fn settings_file(&self) -> PathBuf {
        self.app_data_dir.join("settings.json")
    }

    /// Path of the SQLite history database.
    pub fn history_file(&self) -> PathBuf {
        self.app_data_dir.join("diskpilot.sqlite3")
    }

    /// Path of the persisted last-scan snapshot.
    pub fn snapshot_file(&self) -> PathBuf {
        self.app_data_dir.join("snapshot.json")
    }

    /// Path of the snapshot the last-scan snapshot replaced on the most
    /// recent scan (the "previous scan" of the comparison).
    pub fn previous_snapshot_file(&self) -> PathBuf {
        self.app_data_dir.join("previous_snapshot.json")
    }

    /// Path of the scan-history time series (`scan_history.json`):
    /// one entry per completed scan, powering the space forecast and
    /// the scheduled-scan alerts.
    pub fn scan_history_file(&self) -> PathBuf {
        self.app_data_dir.join("scan_history.json")
    }

    /// Path of the USN journal checkpoints (`usn_checkpoints.json`):
    /// per-volume journal positions the live index resumes from
    /// (see `crate::live`).
    pub fn usn_checkpoints_file(&self) -> PathBuf {
        self.app_data_dir.join("usn_checkpoints.json")
    }

    /// Path of the OCR text cache (`ocr_cache.json`): recognized
    /// text keyed by file path + mtime + size, so an unchanged image
    /// is never recognized twice (see `crate::ocr`).
    pub fn ocr_cache_file(&self) -> PathBuf {
        self.app_data_dir.join("ocr_cache.json")
    }

    /// Root the safety contexts are built against: the scanned root when
    /// a scan exists, otherwise the configured scan root.
    pub fn scan_root(&self) -> String {
        if let Ok(engine) = self.engine.lock() {
            if let Some(summary) = engine.summary.as_ref() {
                return summary.root.clone();
            }
        }
        if let Ok(settings) = self.settings.lock() {
            return settings.scan_root.clone();
        }
        home_dir_string()
    }
}

fn home_dir_string() -> String {
    let from_core = diskpilot_core::model::home_dir_string();
    if from_core.is_empty() {
        "C:\\".to_string()
    } else {
        from_core
    }
}

/// Load settings from `<dir>/settings.json`, falling back to defaults.
fn load_settings(dir: &Path) -> Settings {
    let path = dir.join("settings.json");
    match std::fs::read_to_string(&path) {
        Ok(text) => serde_json::from_str(&text).unwrap_or_default(),
        Err(_) => Settings::default(),
    }
}

/// Persist settings to `<dir>/settings.json` (written via a temp file so a
/// crash mid-write cannot corrupt the previous settings).
pub(crate) fn persist_settings(dir: &Path, settings: &Settings) -> std::io::Result<()> {
    let text = serde_json::to_string_pretty(settings)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    let tmp = dir.join("settings.json.tmp");
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, dir.join("settings.json"))
}

/// The Quick Search toggle shortcut: Ctrl+Shift+Space.
fn quicksearch_shortcut() -> tauri_plugin_global_shortcut::Shortcut {
    tauri_plugin_global_shortcut::Shortcut::new(
        Some(
            tauri_plugin_global_shortcut::Modifiers::CONTROL
                | tauri_plugin_global_shortcut::Modifiers::SHIFT,
        ),
        tauri_plugin_global_shortcut::Code::Space,
    )
}

/// Extract a `--scan <path>` launch argument (the Explorer
/// context-menu entry starts DiskPilot this way), if one is present
/// and names a non-empty path. The path is passed through
/// `normalize_scan_path` first: Explorer's `%1` substitution can
/// mangle it on the way in.
fn scan_path_from_args(args: &[String]) -> Option<String> {
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        if arg == "--scan" {
            return iter
                .next()
                .cloned()
                .filter(|path| !path.is_empty())
                .map(|path| normalize_scan_path(&path));
        }
    }
    None
}

/// Repair the manglings a shell-supplied path can arrive with.
///
/// The important one is the drive root: the context menu runs
/// `DiskPilot --scan "%1"`, and for a drive root `%1` is `C:\` —
/// the trailing backslash escapes the closing quote under the
/// standard command-line parsing rules, so the argument that
/// actually arrives is the three characters `C:"`. That exact
/// shape (drive letter, colon, quote) is converted back to the
/// root path `C:\`.
///
/// More generally, a single stray trailing quote is stripped when
/// the path as spelled does not exist but the stripped spelling
/// does. Anything else is returned untouched.
fn normalize_scan_path(raw: &str) -> String {
    let bytes = raw.as_bytes();
    if bytes.len() == 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && bytes[2] == b'"'
    {
        return format!("{}:\\", &raw[..2]);
    }
    if let Some(stripped) = raw.strip_suffix('"') {
        if !stripped.is_empty() && !Path::new(raw).exists() && Path::new(stripped).exists() {
            return stripped.to_string();
        }
    }
    raw.to_string()
}

/// Build and run the application.
pub fn run() {
    tauri::Builder::default()
        // Single instance FIRST: a second launch (e.g. another
        // Explorer "Analyze with DiskPilot") hands its `--scan`
        // path to the running instance — stored as the launch
        // intent and announced with a `launch-scan` event — and
        // brings the main window forward instead of starting over.
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            if let Some(path) = scan_path_from_args(&argv) {
                if let Some(state) = app.try_state::<ShellState>() {
                    if let Ok(mut slot) = state.launch_intent.lock() {
                        *slot = Some(path.clone());
                    }
                }
                let _ = app.emit("launch-scan", path);
            }
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, shortcut, event| {
                    if event.state() == tauri_plugin_global_shortcut::ShortcutState::Pressed
                        && *shortcut == quicksearch_shortcut()
                    {
                        watch::toggle_quicksearch(app);
                    }
                })
                .build(),
        )
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&data_dir)?;

            let settings = load_settings(&data_dir);
            let scan_root = settings.scan_root.clone();

            // Explorer context-menu launches arrive as
            // `DiskPilot --scan <path>`; the frontend claims the
            // path once via `get_launch_intent`.
            let launch_intent = scan_path_from_args(&std::env::args().collect::<Vec<String>>());

            // Re-apply the autostart setting so a registration removed
            // outside the app self-heals on the next launch.
            if let Err(e) = watch::set_autostart_impl(settings.auto_start) {
                eprintln!("diskpilot: could not apply the autostart setting: {e}");
            }

            // Engine state, with the last scan's summary restored from
            // the snapshot so the Overview is populated immediately.
            let mut engine = AppState::new();
            engine.summary = snapshot::load_snapshot(&data_dir.join("snapshot.json"))
                .ok()
                .flatten();
            let previous_summary =
                snapshot::load_snapshot(&data_dir.join("previous_snapshot.json"))
                    .ok()
                    .flatten();

            app.manage(ShellState {
                app_data_dir: data_dir,
                settings: Mutex::new(settings),
                engine: Mutex::new(engine),
                previous_summary: Mutex::new(previous_summary),
                key_store: diskpilot_core::assistant::provider::default_key_store(),
                recycle_bin_cancel: Arc::new(AtomicBool::new(false)),
                history: Mutex::new(None),
                ocr_generation: Arc::new(AtomicU64::new(0)),
                live: Mutex::new(live::LiveStatus::default()),
                duplicates_report: Mutex::new(None),
                scan_exclusions: Mutex::new(Vec::new()),
                index_generation: Arc::new(AtomicU64::new(0)),
                launch_intent: Mutex::new(launch_intent),
            });

            tray::build_tray(app.handle())?;

            // Seed the tray ring with the current free space of the scan
            // root's volume.
            let info = diskpilot_core::disk::disk_info_for(Path::new(&scan_root));
            tray::update_tray(app.handle(), &info);

            // Quick Search: a hidden popup window toggled by the
            // global shortcut. A failure to create it or to register
            // the shortcut costs the feature, not the app.
            if let Err(e) = watch::create_quicksearch_window(app.handle()) {
                eprintln!("diskpilot: could not create the Quick Search window: {e}");
            }
            {
                use tauri_plugin_global_shortcut::GlobalShortcutExt;
                if let Err(e) = app.global_shortcut().register(quicksearch_shortcut()) {
                    eprintln!("diskpilot: could not register the Quick Search shortcut: {e}");
                }
            }

            // Scheduled scans + alerts (see `crate::watch`).
            watch::spawn_scheduler(app.handle().clone());

            // USN live index poller (see `crate::live`): keeps the
            // in-memory index fresh between scans while the app runs.
            live::spawn_poller(app.handle().clone());

            Ok(())
        })
        .on_window_event(|window, event| {
            // Closing the window keeps DiskPilot running in the tray;
            // Quit in the tray menu is the way out.
            if window.label() == "main" {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
            // Quick Search behaves like a launcher popup: it hides
            // when it loses focus.
            if window.label() == "quicksearch" {
                if let tauri::WindowEvent::Focused(false) = event {
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_environment,
            commands::get_disk_info,
            commands::get_drives,
            commands::start_scan,
            commands::cancel_scan,
            commands::get_scan_summary,
            commands::get_scan_comparison,
            commands::get_scan_history,
            commands::get_space_forecast,
            commands::get_biggest_changes,
            commands::get_autostart,
            commands::set_autostart,
            commands::list_directory,
            commands::get_recommendations,
            commands::get_cleanup_cards,
            commands::build_card_cleanup_plan,
            commands::build_cleanup_plan,
            commands::execute_cleanup_plan,
            commands::get_history,
            commands::restore_cleanup_run,
            commands::find_duplicates,
            commands::remove_duplicate_copies,
            commands::get_recent_downloads,
            commands::get_download_provenance,
            commands::get_media_info,
            commands::search_files,
            commands::run_search_filters,
            commands::get_similar_images,
            commands::get_docker_overview,
            commands::docker_preview_action,
            commands::docker_execute_action,
            commands::get_devtools_reports,
            commands::get_applications,
            commands::get_recycle_bin_items,
            commands::empty_recycle_bin,
            commands::restore_recycle_item,
            commands::cancel_empty_recycle_bin,
            commands::get_cloud_status,
            commands::get_media_summary,
            commands::get_games,
            commands::get_startup_items,
            commands::set_startup_item_enabled,
            commands::get_compression_candidates,
            commands::set_folder_compression,
            commands::get_extension_stats,
            commands::get_extension_files,
            commands::free_up_space,
            commands::export_report,
            commands::export_scan_csv,
            commands::list_app_junk,
            commands::clean_app_junk,
            smart_io::get_drive_health,
            winget_io::list_available_updates,
            winget_io::update_software,
            relocate_io::relocate_folder,
            relocate_io::undo_relocation,
            commands::get_explorer_integration,
            commands::set_explorer_integration,
            commands::get_launch_intent,
            commands::scan_transcript_secrets,
            commands::assistant_ask,
            commands::save_api_key,
            commands::has_api_key,
            commands::delete_api_key,
            commands::get_settings,
            commands::save_settings,
            commands::reveal_in_explorer,
            commands::open_external,
        ])
        .run(tauri::generate_context!())
        .expect("error while running DiskPilot");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_repairs_mangled_drive_root() {
        assert_eq!(normalize_scan_path("C:\""), "C:\\");
        assert_eq!(normalize_scan_path("d:\""), "d:\\");
        assert_eq!(normalize_scan_path("Z:\""), "Z:\\");
    }

    #[test]
    fn normalize_strips_stray_trailing_quote_when_clean_path_exists() {
        let dir = std::env::temp_dir();
        let mangled = format!("{}\"", dir.display());
        assert_eq!(normalize_scan_path(&mangled), dir.to_string_lossy());
    }

    #[test]
    fn normalize_keeps_stray_quote_when_clean_path_does_not_exist() {
        let raw = "definitely-not-a-real-place\"";
        assert_eq!(normalize_scan_path(raw), raw);
    }

    #[test]
    fn normalize_leaves_ordinary_paths_alone() {
        assert_eq!(normalize_scan_path("C:\\Games"), "C:\\Games");
        assert_eq!(normalize_scan_path("D:\\"), "D:\\");
        assert_eq!(normalize_scan_path(""), "");
    }

    #[test]
    fn scan_path_from_args_extracts_and_normalizes() {
        let args: Vec<String> = ["diskpilot", "--scan", "C:\""]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(scan_path_from_args(&args).as_deref(), Some("C:\\"));
        let bare: Vec<String> = ["diskpilot"].iter().map(|s| s.to_string()).collect();
        assert_eq!(scan_path_from_args(&bare), None);
        let empty: Vec<String> = ["diskpilot", "--scan", ""]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(scan_path_from_args(&empty), None);
    }
}
