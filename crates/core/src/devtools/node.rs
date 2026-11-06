//! Node.js analyzer: per-project node_modules with manifest freshness,
//! package-manager caches and nvm-windows versions.

use super::{child_dirs, file_name_str, find_dirs_named, make_item, DevPaths};
use crate::model::*;
use std::path::Path;

fn manifest_age_days(project_dir: &Path) -> Option<i64> {
    for name in [
        "package.json",
        "pnpm-lock.yaml",
        "package-lock.json",
        "yarn.lock",
    ] {
        let p = project_dir.join(name);
        if let Ok(md) = std::fs::metadata(&p) {
            if let Ok(modified) = md.modified() {
                if let Ok(elapsed) = modified.elapsed() {
                    return Some((elapsed.as_secs() / 86_400) as i64);
                }
            }
        }
    }
    None
}

pub fn analyze(paths: &DevPaths) -> Option<ToolReport> {
    let mut sections: Vec<ToolSection> = Vec::new();

    let modules = find_dirs_named(&paths.scan_root, &["node_modules"], 4);
    if !modules.is_empty() {
        let items: Vec<ToolItem> = modules
            .iter()
            .map(|m| {
                let project = m.parent().unwrap_or(m.as_path());
                let age = manifest_age_days(project);
                let detail = match age {
                    Some(d) => format!(
                        "Dependencies for {}; project manifest last changed {} day(s) ago.",
                        project.display(),
                        d
                    ),
                    None => format!("Dependencies for {}.", project.display()),
                };
                make_item(
                    file_name_str(project),
                    m,
                    ToolItemState::Informational,
                    detail,
                )
            })
            .collect();
        sections.push(ToolSection {
            title: "node_modules by project".to_string(),
            items,
        });
    }

    let mut caches: Vec<ToolItem> = Vec::new();
    let cache_candidates: Vec<(&str, std::path::PathBuf)> = vec![
        ("npm cache", paths.local.join("npm-cache")),
        ("npm cache (~/.npm)", paths.home.join(".npm")),
        ("pnpm store", paths.local.join("pnpm").join("store")),
        ("pnpm store (~/.pnpm-store)", paths.home.join(".pnpm-store")),
        ("Yarn cache", paths.local.join("Yarn").join("Cache")),
        (
            "Yarn cache (~/.yarn)",
            paths.home.join(".yarn").join("cache"),
        ),
        (
            "Bun cache",
            paths.home.join(".bun").join("install").join("cache"),
        ),
    ];
    for (label, p) in cache_candidates {
        if p.exists() {
            caches.push(make_item(label, &p, ToolItemState::Informational, "Package manager cache or store; pruned with the tool's own command where one exists."));
        }
    }
    if !caches.is_empty() {
        sections.push(ToolSection {
            title: "Package manager caches".to_string(),
            items: caches,
        });
    }

    // nvm-windows: NVM_HOME env or %APPDATA%\nvm; versions are folders named
    // like v20.11.0; the active one comes from settings.txt / the symlink.
    let nvm_home = std::env::var_os("NVM_HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| paths.roaming.join("nvm"));
    if nvm_home.exists() {
        let current = read_nvm_current(&nvm_home);
        let mut items = Vec::new();
        for dir in child_dirs(&nvm_home) {
            let name = file_name_str(&dir);
            if name.starts_with('v')
                && name[1..]
                    .chars()
                    .next()
                    .map(|c| c.is_ascii_digit())
                    .unwrap_or(false)
            {
                let is_current = current.as_deref() == Some(name.as_str());
                items.push(make_item(
                    name.clone(),
                    &dir,
                    if is_current {
                        ToolItemState::Current
                    } else {
                        ToolItemState::Informational
                    },
                    if is_current {
                        "Currently selected Node.js version."
                    } else {
                        "Installed Node.js version (not currently selected)."
                    },
                ));
            }
        }
        if !items.is_empty() {
            sections.push(ToolSection {
                title: "nvm-windows versions".to_string(),
                items,
            });
        }
    }

    if sections.is_empty() {
        None
    } else {
        Some(ToolReport {
            tool: "Node.js".to_string(),
            sections,
        })
    }
}

fn read_nvm_current(nvm_home: &Path) -> Option<String> {
    let settings = std::fs::read_to_string(nvm_home.join("settings.txt")).ok()?;
    // settings.txt holds lines like "root: ..." / "path: ..."; the current
    // version is resolved through the symlink target when available.
    let link = nvm_home.parent()?.join("nodejs");
    if let Ok(target) = std::fs::read_link(link) {
        if let Some(name) = target.file_name() {
            return Some(name.to_string_lossy().to_string());
        }
    }
    let _ = settings;
    None
}
