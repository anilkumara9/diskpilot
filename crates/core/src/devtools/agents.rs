//! AI coding agent analyzers, driven by the location data table.

use super::agent_data::{agent_specs, AgentDataKind};
use super::{child_dirs, file_name_str, make_item, DevPaths};
use crate::model::*;
use std::path::PathBuf;

/// Decode a Claude Code style project folder name into a candidate project
/// path. Names encode the absolute path with separators replaced by '-',
/// e.g. "-home-user-proj" or "C--Users-user-proj".
pub fn decode_project_dir_name(name: &str) -> Option<PathBuf> {
    if let Some(rest) = name.strip_prefix(|c| c == '-') {
        let candidate = format!("/{}", rest.replace('-', "/"));
        return Some(PathBuf::from(candidate));
    }
    // Windows drive form: "C--Users-name-proj" -> "C:\Users\name\proj"
    let bytes = name.as_bytes();
    if bytes.len() > 3 && bytes[1] == b'-' && bytes[2] == b'-' {
        let drive = (bytes[0] as char).to_string();
        let rest = &name[3..];
        let candidate = format!("{}:\\{}", drive, rest.replace('-', "\\"));
        return Some(PathBuf::from(candidate));
    }
    None
}

pub fn analyze_all(paths: &DevPaths) -> Vec<ToolReport> {
    let mut reports = Vec::new();
    for spec in agent_specs() {
        let home = paths.home.join(spec.home_rel);
        if !home.exists() {
            continue;
        }
        let mut items: Vec<ToolItem> = Vec::new();
        // Installed versions folder (Claude Code keeps version folders).
        let versions_dir = home.join("versions");
        if versions_dir.exists() {
            for v in child_dirs(&versions_dir) {
                items.push(make_item(
                    format!("Version {}", file_name_str(&v)),
                    &v,
                    ToolItemState::Informational,
                    "Installed version of the CLI.",
                ));
            }
        }
        for entry in spec.entries {
            let p = home.join(entry.rel);
            if !p.exists() {
                continue;
            }
            if entry.kind == AgentDataKind::Sessions && p.is_dir() {
                // Per-project session folders: orphan when the project is gone.
                let children = child_dirs(&p);
                if children.is_empty() {
                    items.push(make_item(
                        entry.kind.label(),
                        &p,
                        ToolItemState::Current,
                        entry.detail,
                    ));
                } else {
                    for child in children {
                        let state = match decode_project_dir_name(&file_name_str(&child)) {
                            Some(proj) if !proj.exists() => ToolItemState::Orphaned,
                            Some(_) => ToolItemState::Current,
                            None => ToolItemState::Informational,
                        };
                        items.push(make_item(
                            format!("{}: {}", entry.kind.label(), file_name_str(&child)),
                            &child,
                            state,
                            entry.detail,
                        ));
                    }
                }
                continue;
            }
            let state = match entry.kind {
                AgentDataKind::Cache | AgentDataKind::Logs | AgentDataKind::Updates => {
                    ToolItemState::Informational
                }
                AgentDataKind::Checkpoints | AgentDataKind::ShellSnapshots => ToolItemState::Stale,
                _ => ToolItemState::Informational,
            };
            items.push(make_item(entry.kind.label(), &p, state, entry.detail));
        }
        if !items.is_empty() {
            reports.push(ToolReport {
                tool: spec.tool.to_string(),
                sections: vec![ToolSection {
                    title: format!("{} data", spec.tool),
                    items,
                }],
            });
        }
    }
    reports
}

/// Claude desktop app caches (Chromium based) under roaming/local data.
pub fn analyze_claude_app(paths: &DevPaths) -> Option<ToolReport> {
    let base = paths.roaming.join("Claude");
    if !base.exists() {
        return None;
    }
    let mut items = Vec::new();
    for rel in ["Cache", "Code Cache", "GPUCache", "logs", "claude-code"] {
        let p = base.join(rel);
        if p.exists() {
            items.push(make_item(
                rel,
                &p,
                ToolItemState::Informational,
                "Desktop app data measured read-only.",
            ));
        }
    }
    let updates = paths.local.join("AnthropicClaude");
    if updates.exists() {
        items.push(make_item(
            "App files and updates",
            &updates,
            ToolItemState::Informational,
            "Desktop app install and update staging.",
        ));
    }
    if items.is_empty() {
        None
    } else {
        Some(ToolReport {
            tool: "Claude app".to_string(),
            sections: vec![ToolSection {
                title: "Claude app data".to_string(),
                items,
            }],
        })
    }
}
