//! Cursor and VS Code analyzers: per-project workspace storage (including
//! projects that were deleted), extensions versus leftover folders, caches,
//! logs and stale backups. Databases are measured read-only, never opened
//! for writing.

use super::{child_dirs, child_entries, file_name_str, make_item, DevPaths};
use crate::model::*;
use crate::scanner::measure_path;
use std::path::PathBuf;

struct EditorSpec {
    name: &'static str,
    roaming_folder: &'static str,
    home_folder: &'static str,
}

fn specs() -> Vec<EditorSpec> {
    vec![
        EditorSpec {
            name: "Cursor",
            roaming_folder: "Cursor",
            home_folder: ".cursor",
        },
        EditorSpec {
            name: "VS Code",
            roaming_folder: "Code",
            home_folder: ".vscode",
        },
    ]
}

/// Extract the folder URI from a workspaceStorage workspace.json and decode
/// it to a local path. Returns None when the file is unreadable or has no
/// folder (multi-root / untitled workspaces are treated as current).
pub fn workspace_folder_from_json(text: &str) -> Option<PathBuf> {
    let value: serde_json::Value = serde_json::from_str(text).ok()?;
    let uri = value
        .get("folder")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .or_else(|| {
            value
                .get("workspace")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        })?;
    uri_to_path(&uri)
}

pub fn uri_to_path(uri: &str) -> Option<PathBuf> {
    let rest = uri.strip_prefix("file://")?;
    let decoded = percent_decode(rest);
    // file:///C:/Users/... -> C:\Users\...
    let trimmed = decoded.trim_start_matches('/');
    if trimmed.len() > 2 && trimmed.as_bytes()[1] == b':' {
        return Some(PathBuf::from(trimmed.replace('/', "\\")));
    }
    Some(PathBuf::from(format!(
        "/{}",
        trimmed.trim_start_matches('/')
    )))
}

fn percent_decode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v as char);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

fn analyze_editor(paths: &DevPaths, spec: &EditorSpec) -> Option<ToolReport> {
    let roaming = paths.roaming.join(spec.roaming_folder);
    let home_ext = paths.home.join(spec.home_folder);
    if !roaming.exists() && !home_ext.exists() {
        return None;
    }
    let mut sections: Vec<ToolSection> = Vec::new();

    // --- Workspace storage, per project ---
    let ws_root = roaming.join("User").join("workspaceStorage");
    if ws_root.exists() {
        let mut items = Vec::new();
        for entry in child_dirs(&ws_root) {
            let meta = entry.join("workspace.json");
            let state = if meta.exists() {
                match std::fs::read_to_string(&meta)
                    .ok()
                    .and_then(|t| workspace_folder_from_json(&t))
                {
                    Some(folder) if !folder.exists() => ToolItemState::Orphaned,
                    Some(_) => ToolItemState::Current,
                    None => ToolItemState::Informational,
                }
            } else {
                ToolItemState::Informational
            };
            let detail = match state {
                ToolItemState::Orphaned => {
                    "Workspace state for a project folder that no longer exists."
                }
                ToolItemState::Current => "Workspace state for an existing project.",
                _ => "Workspace state (project could not be determined).",
            };
            items.push(make_item(file_name_str(&entry), &entry, state, detail));
        }
        // Chat database size (read-only measurement only).
        let chat_db = roaming
            .join("User")
            .join("globalStorage")
            .join("state.vscdb");
        if chat_db.exists() {
            let count_note = "Chat and global state database, measured read-only; never modified.";
            items.push(ToolItem {
                name: "state.vscdb (chat/global state)".to_string(),
                path: chat_db.to_string_lossy().to_string(),
                bytes: measure_path(&chat_db),
                state: ToolItemState::Informational,
                detail: count_note.to_string(),
            });
        }
        if !items.is_empty() {
            sections.push(ToolSection {
                title: "Workspace storage".to_string(),
                items,
            });
        }
    }

    // --- Extensions vs leftover folders ---
    let ext_root = home_ext.join("extensions");
    if ext_root.exists() {
        let obsolete: Vec<String> = std::fs::read_to_string(ext_root.join(".obsolete"))
            .ok()
            .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
            .and_then(|v| {
                v.as_object()
                    .map(|o| o.keys().cloned().collect::<Vec<String>>())
            })
            .unwrap_or_default();
        let mut items = Vec::new();
        for entry in child_dirs(&ext_root) {
            let name = file_name_str(&entry);
            let state = if obsolete.iter().any(|o| name.starts_with(o)) {
                ToolItemState::Stale
            } else {
                ToolItemState::Current
            };
            let detail = if state == ToolItemState::Stale {
                "Extension folder left behind after uninstall or update."
            } else {
                "Installed extension."
            };
            items.push(make_item(name, &entry, state, detail));
        }
        if !items.is_empty() {
            sections.push(ToolSection {
                title: "Extensions".to_string(),
                items,
            });
        }
    }

    // --- Caches, logs, backups, update downloads ---
    let mut misc = Vec::new();
    for rel in [
        "CachedData",
        "Cache",
        "Code Cache",
        "GPUCache",
        "logs",
        "Crashpad",
    ] {
        let p = roaming.join(rel);
        if p.exists() {
            misc.push(make_item(
                rel,
                &p,
                ToolItemState::Informational,
                "Editor cache or logs.",
            ));
        }
    }
    let backups = roaming.join("Backups");
    if backups.exists() {
        misc.push(make_item(
            "Backups",
            &backups,
            ToolItemState::Stale,
            "Hot-exit backups; stale when old.",
        ));
    }
    let user_dir = roaming.join("User");
    if user_dir.exists() {
        for entry in child_entries(&user_dir) {
            let name = file_name_str(&entry);
            let lower = name.to_lowercase();
            if lower.contains("backup") && (lower.ends_with(".vscdb") || lower.ends_with(".json")) {
                misc.push(make_item(
                    name,
                    &entry,
                    ToolItemState::Stale,
                    "Stale state database backup.",
                ));
            }
        }
    }
    // Leftover update downloads live beside the install on Windows.
    for cand in [
        paths
            .local
            .join(format!("{}-updater", spec.name.to_lowercase())),
        paths.local.join("cursor-updater"),
        paths.local.join("vscode-updater"),
    ] {
        if cand.exists() {
            misc.push(make_item(
                "Update downloads",
                &cand,
                ToolItemState::Stale,
                "Downloaded update packages left after install.",
            ));
        }
    }
    if !misc.is_empty() {
        sections.push(ToolSection {
            title: "Caches, logs and backups".to_string(),
            items: misc,
        });
    }

    // Copilot extension storage inside this editor (also after uninstall:
    // the storage folder can outlive the extension).
    let copilot_storage = roaming
        .join("User")
        .join("globalStorage")
        .join("github.copilot");
    if copilot_storage.exists() {
        sections.push(ToolSection {
            title: "Copilot storage".to_string(),
            items: vec![make_item(
                "github.copilot",
                &copilot_storage,
                ToolItemState::Informational,
                "Copilot extension storage inside this editor.",
            )],
        });
    }

    if sections.is_empty() {
        None
    } else {
        Some(ToolReport {
            tool: spec.name.to_string(),
            sections,
        })
    }
}

pub fn analyze_all(paths: &DevPaths) -> Vec<ToolReport> {
    let mut out = Vec::new();
    for spec in specs() {
        if let Some(r) = analyze_editor(paths, &spec) {
            out.push(r);
        }
    }
    if let Some(r) = super::agents::analyze_claude_app(paths) {
        out.push(r);
    }
    out
}
