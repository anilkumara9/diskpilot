//! System tray: a live free-space ring, a navigation menu, and
//! hide-to-tray behaviour (wired up in `lib.rs` via `on_window_event`).
//!
//! Menu entries that target a page emit a `tray://navigate` event with a
//! `{ "target": … }` payload; the frontend listens and routes. Possible
//! targets: `"cleanup"`, `"duplicates"`, `"empty-recycle-bin"`.

use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager,
};

use diskpilot_core::model::DiskInfo;

use crate::tray_icon;

pub const TRAY_ID: &str = "diskpilot-tray";

/// Icon edge length used for the tray (rendered per update, cheap at 32px).
const TRAY_ICON_SIZE: u32 = 32;

/// Build the tray icon and its menu. Called once from `setup`.
pub fn build_tray(app: &AppHandle) -> tauri::Result<()> {
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
    let menu = Menu::with_items(
        app,
        &[&open, &cleanup, &duplicates, &empty_bin, &separator, &quit],
    )?;

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
