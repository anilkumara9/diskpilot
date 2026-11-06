//! DiskPilot desktop shell.
//!
//! The shell is deliberately thin: it owns the window, the system tray,
//! persisted settings and the bridge state between the typed Tauri
//! commands and the `diskpilot-core` engine. All analysis and every
//! destructive operation live in the core crate behind its safety gate;
//! the webview itself has no filesystem or shell access.

mod commands;
mod recycle_bin;
mod scan_history;
mod tray;
mod tray_icon;
mod watch;

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use tauri::Manager;

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
}

fn default_auto_scan() -> String {
    "off".to_string()
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
fn persist_settings(dir: &Path, settings: &Settings) -> std::io::Result<()> {
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

/// Build and run the application.
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
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
            commands::cancel_empty_recycle_bin,
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
