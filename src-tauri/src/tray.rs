//! System tray: a live free-space ring, a navigation menu with a
//! Recent downloads submenu, and hide-to-tray behaviour (wired up in
//! `lib.rs` via `on_window_event`).
//!
//! Menu entries that target a page emit a `tray://navigate` event with a
//! `{ "target": … }` payload; the frontend listens and routes. Possible
//! targets: `"cleanup"`, `"duplicates"`, `"empty-recycle-bin"`. Recent
//! downloads entries (`recent-download-<n>` ids) never reach the
//! frontend: the menu handler resolves the id to the path the core
//! reported and reveals it in Explorer directly.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Mutex, OnceLock};

use tauri::{
    menu::{IsMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager,
};

use diskpilot_core::model::DiskInfo;

use crate::tray_icon;

pub const TRAY_ID: &str = "diskpilot-tray";

/// Icon edge length used for the tray (rendered per update, cheap at 32px).
const TRAY_ICON_SIZE: u32 = 32;

/// How many recent downloads the submenu lists.
const RECENT_DOWNLOAD_LIMIT: usize = 10;

/// Menu id prefix for Recent downloads entries; the suffix is the
/// entry's position in the current listing.
const RECENT_ID_PREFIX: &str = "recent-download-";

/// Paths behind the current Recent downloads menu ids. The menu only
/// carries labels, so the handler resolves ids through this registry;
/// it is rewritten every time the menu is (re)built.
fn recent_paths() -> &'static Mutex<HashMap<String, String>> {
    static PATHS: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();
    PATHS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// The newest files in the Downloads folder right now, as
/// `(menu id, display name, path)` triples.
fn recent_download_entries() -> Vec<(String, String, String)> {
    let dir = crate::commands::downloads_dir();
    if !dir.is_dir() {
        return Vec::new();
    }
    diskpilot_core::provenance::recent_downloads(&dir, RECENT_DOWNLOAD_LIMIT)
        .into_iter()
        .enumerate()
        .map(|(i, file)| (format!("{RECENT_ID_PREFIX}{i}"), file.name, file.path))
        .collect()
}

/// Build the full tray menu, including the Recent downloads submenu
/// for the Downloads folder as it is right now.
fn build_menu(app: &AppHandle) -> tauri::Result<Menu<tauri::Wry>> {
    let open = MenuItem::with_id(app, "open", "Open DiskPilot", true, None::<&str>)?;
    let cleanup = MenuItem::with_id(app, "cleanup", "Cleanup", true, None::<&str>)?;
    let duplicates = MenuItem::with_id(app, "duplicates", "Duplicates", true, None::<&str>)?;
    let empty_bin = MenuItem::with_id(
        app,
        "empty-recycle-bin",
        "Empty Recycle Bin",
        true,
        None::<&str>,
    )?;
    let separator = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;

    let entries = recent_download_entries();
    if let Ok(mut registry) = recent_paths().lock() {
        registry.clear();
        for (id, _name, path) in &entries {
            registry.insert(id.clone(), path.clone());
        }
    }
    let recent_menu = if entries.is_empty() {
        let none = MenuItem::with_id(
            app,
            "recent-downloads-none",
            "No recent downloads",
            false,
            None::<&str>,
        )?;
        let items: Vec<&dyn IsMenuItem<_>> = vec![&none];
        Submenu::with_items(app, "Recent downloads", true, &items)?
    } else {
        let items: Vec<MenuItem<tauri::Wry>> = entries
            .iter()
            .map(|(id, name, _path)| MenuItem::with_id(app, id, name, true, None::<&str>))
            .collect::<tauri::Result<Vec<_>>>()?;
        let refs: Vec<&dyn IsMenuItem<_>> = items
            .iter()
            .map(|item| item as &dyn IsMenuItem<_>)
            .collect();
        Submenu::with_items(app, "Recent downloads", true, &refs)?
    };

    Menu::with_items(
        app,
        &[
            &open,
            &cleanup,
            &duplicates,
            &recent_menu,
            &empty_bin,
            &separator,
            &quit,
        ],
    )
}

/// Build the tray icon and its menu. Called once from `setup`.
pub fn build_tray(app: &AppHandle) -> tauri::Result<()> {
    let menu = build_menu(app)?;

    TrayIconBuilder::with_id(TRAY_ID)
        .icon(tray_icon::image(0.0, TRAY_ICON_SIZE))
        .tooltip("DiskPilot")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => show_main(app),
            "cleanup" => navigate(app, "cleanup"),
            "duplicates" => navigate(app, "duplicates"),
            "empty-recycle-bin" => navigate(app, "empty-recycle-bin"),
            "quit" => app.exit(0),
            id if id.starts_with(RECENT_ID_PREFIX) => reveal_recent(app, id),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}

/// Reveal the file behind a Recent downloads menu entry in Explorer.
/// The path comes from the registry `build_menu` filled — i.e. from
/// the core's own listing of the user's Downloads folder — so it goes
/// through the same file-manager spawn as `reveal_in_explorer`,
/// without that command's webview-facing containment check.
fn reveal_recent(app: &AppHandle, id: &str) {
    let path = recent_paths()
        .lock()
        .ok()
        .and_then(|registry| registry.get(id).cloned());
    if let Some(path) = path {
        show_main(app);
        let _ = crate::commands::reveal_path_in_explorer(Path::new(&path));
    }
}

/// Rebuild the tray menu so the Recent downloads submenu reflects
/// the Downloads folder as it is now. Called after each completed
/// scan (the initial menu is built with current data at startup).
pub fn refresh_recent_downloads(app: &AppHandle) {
    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        return;
    };
    if let Ok(menu) = build_menu(app) {
        let _ = tray.set_menu(Some(menu));
    }
}

/// Refresh the tray ring and tooltip from fresh disk information.
pub fn update_tray(app: &AppHandle, info: &DiskInfo) {
    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        return;
    };
    let used_fraction = if info.total_bytes > 0 {
        info.used_bytes as f64 / info.total_bytes as f64
    } else {
        0.0
    };
    let _ = tray.set_icon(Some(tray_icon::image(used_fraction, TRAY_ICON_SIZE)));
    let _ = tray.set_tooltip(Some(format!(
        "DiskPilot — {} available",
        format_decimal_gb(info.available_bytes)
    )));
}

/// Decimal gigabytes, matching the frontend's byte formatting (1 GB = 10^9).
fn format_decimal_gb(bytes: u64) -> String {
    format!("{:.1} GB", bytes as f64 / 1_000_000_000.0)
}

/// Show, unminimise and focus the main window.
pub fn show_main(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

/// Show the window and ask the frontend to navigate to `target`.
fn navigate(app: &AppHandle, target: &str) {
    show_main(app);
    let _ = app.emit("tray://navigate", serde_json::json!({ "target": target }));
}
