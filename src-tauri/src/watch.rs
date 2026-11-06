//! The "watch" features: scheduled scans with alerts, launch-at-login
//! (autostart), and the global Quick Search shortcut.
//!
//! The scheduler is a plain background thread: it wakes once a minute
//! after startup (a catch-up check, so a scan missed while the app was
//! closed happens soon after launch) and then every 30 minutes. When
//! a scan is due it runs through the very same routine as the manual
//! scan command, flagged as scheduled — only scheduled scans raise
//! notifications.

use std::time::Duration;

use tauri::{AppHandle, Manager};

use diskpilot_core::model::now_ms;

use crate::commands;
use crate::scan_history;
use crate::ShellState;

/// First scheduler check: one minute after startup.
const STARTUP_DELAY: Duration = Duration::from_secs(60);
/// How often the scheduler re-checks whether a scan is due.
const CHECK_INTERVAL: Duration = Duration::from_secs(30 * 60);
/// A "daily" scan is due 20 hours after the last one (not a strict
/// 24 h, so an evening scan does not drift later every day).
const DAILY_DUE_MS: i64 = 20 * 3_600_000;
/// A "weekly" scan is due after 6.5 days, for the same reason.
const WEEKLY_DUE_MS: i64 = 561_600_000;

/// Spawn the scheduler thread. Called once from app setup.
pub fn spawn_scheduler(app: AppHandle) {
    std::thread::spawn(move || {
        std::thread::sleep(STARTUP_DELAY);
        loop {
            tauri::async_runtime::block_on(check_and_run(&app));
            std::thread::sleep(CHECK_INTERVAL);
        }
    });
}

/// Run a scan of the configured root when one is due. Quiet by
/// design: every "not due / disabled / already scanning" outcome is a
/// no-op, and a failed scan is logged, never surfaced — the user did
/// not ask for this run.
async fn check_and_run(app: &AppHandle) {
    let state = app.state::<ShellState>();
    let (auto_scan, root) = match state.settings.lock() {
        Ok(settings) => (settings.auto_scan.clone(), settings.scan_root.clone()),
        Err(_) => return,
    };
    let interval_ms = match auto_scan.as_str() {
        "daily" => DAILY_DUE_MS,
        "weekly" => WEEKLY_DUE_MS,
        _ => return,
    };
    let scanning = state
        .engine
        .lock()
        .map(|engine| engine.scanning)
        .unwrap_or(true);
    if scanning {
        return;
    }
    let last_scan_ms = scan_history::load(&state)
        .iter()
        .filter(|entry| commands::same_scan_root(&entry.root_path, &root))
        .map(|entry| entry.scanned_at_ms)
        .max();
    let due = match last_scan_ms {
        None => true,
        Some(last) => now_ms() - last >= interval_ms,
    };
    if !due {
        return;
    }
    if let Err(e) = commands::run_scan(app.clone(), &state, root, true).await {
        eprintln!("diskpilot: scheduled scan failed: {e}");
    }
}

// ---------------------------------------------------------------------------
// Autostart (launch at login)
// ---------------------------------------------------------------------------

/// Register or unregister DiskPilot in the per-user Run key. Off
/// Windows there is no registration to write: this returns
/// `Ok(false)` so the UI can present the setting as unavailable.
#[cfg(windows)]
pub fn set_autostart_impl(enabled: bool) -> Result<bool, String> {
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;

    const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let (key, _) = hkcu
        .create_subkey(RUN_KEY)
        .map_err(|e| format!("could not open the Run registry key: {e}"))?;
    if enabled {
        let exe = std::env::current_exe()
            .map_err(|e| format!("could not locate the DiskPilot executable: {e}"))?;
        key.set_value("DiskPilot", &format!("\"{}\"", exe.to_string_lossy()))
            .map_err(|e| format!("could not set the autostart entry: {e}"))?;
        Ok(true)
    } else {
        match key.delete_value("DiskPilot") {
            Ok(()) => Ok(false),
            // Already absent is exactly the state being asked for.
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(format!("could not remove the autostart entry: {e}")),
        }
    }
}

#[cfg(not(windows))]
pub fn set_autostart_impl(_enabled: bool) -> Result<bool, String> {
    Ok(false)
}

/// Whether the autostart entry is currently registered.
#[cfg(windows)]
pub fn get_autostart_impl() -> bool {
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;

    const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
    RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey(RUN_KEY)
        .ok()
        .and_then(|key| key.get_value::<String, _>("DiskPilot").ok())
        .is_some()
}

#[cfg(not(windows))]
pub fn get_autostart_impl() -> bool {
    false
}

// ---------------------------------------------------------------------------
// Quick Search window
// ---------------------------------------------------------------------------

/// Toggle the Quick Search window: hidden → show and focus,
/// visible → hide.
pub fn toggle_quicksearch(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("quicksearch") {
        if window.is_visible().unwrap_or(false) {
            let _ = window.hide();
        } else {
            let _ = window.show();
            let _ = window.set_focus();
        }
    }
}

/// Create the Quick Search window, hidden. It loads the same app
/// bundle with `?window=quicksearch` in the URL; the frontend renders
/// the compact search surface for that query. The window shares the
/// app's managed state, so its search commands read the same scan
/// index as the main window.
pub fn create_quicksearch_window(app: &AppHandle) -> tauri::Result<()> {
    let window = tauri::WebviewWindowBuilder::new(
        app,
        "quicksearch",
        tauri::WebviewUrl::App("index.html?window=quicksearch".into()),
    )
    .title("DiskPilot Quick Search")
    .inner_size(680.0, 520.0)
    .resizable(false)
    .decorations(false)
    .always_on_top(true)
    .skip_taskbar(true)
    .visible(false)
    .center()
    .build()?;
    let _ = window;
    Ok(())
}
