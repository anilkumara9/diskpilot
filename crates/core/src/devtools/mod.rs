pub mod agent_data;
pub mod agents;
pub mod dotnet;
pub mod editors;
pub mod models;
pub mod node;
pub mod toolchains;

use crate::model::*;
use crate::scanner::measure_path;
use rayon::prelude::*;
use std::path::{Path, PathBuf};

/// Resolved base locations for the analyzers. On Windows these are the user
/// profile and the Roaming / Local application-data folders.
#[derive(Debug, Clone)]
pub struct DevPaths {
    pub home: PathBuf,
    pub roaming: PathBuf,
    pub local: PathBuf,
    pub scan_root: PathBuf,
}

impl DevPaths {
    pub fn from_env() -> Self {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        let roaming = dirs::config_dir().unwrap_or_else(|| home.join("AppData").join("Roaming"));
        let local = dirs::data_local_dir().unwrap_or_else(|| home.join("AppData").join("Local"));
        Self {
            scan_root: home.clone(),
            home,
            roaming,
            local,
        }
    }
    /// Deterministic layout used by tests: everything under one root.
    pub fn under(root: &Path) -> Self {
        Self {
            home: root.to_path_buf(),
            roaming: root.join("AppData").join("Roaming"),
            local: root.join("AppData").join("Local"),
            scan_root: root.to_path_buf(),
        }
    }
}

pub fn make_item(
    name: impl Into<String>,
    path: &Path,
    state: ToolItemState,
    detail: impl Into<String>,
) -> ToolItem {
    ToolItem {
        name: name.into(),
        path: path.to_string_lossy().to_string(),
        bytes: measure_path(path),
        state,
        detail: detail.into(),
    }
}

pub fn child_dirs(path: &Path) -> Vec<PathBuf> {
    let Ok(read) = std::fs::read_dir(path) else {
        return Vec::new();
    };
    let mut out: Vec<PathBuf> = read
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            std::fs::symlink_metadata(p)
                .map(|m| m.is_dir() && !m.file_type().is_symlink())
                .unwrap_or(false)
        })
        .collect();
    out.sort();
    out
}

pub fn child_entries(path: &Path) -> Vec<PathBuf> {
    let Ok(read) = std::fs::read_dir(path) else {
        return Vec::new();
    };
    let mut out: Vec<PathBuf> = read
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            std::fs::symlink_metadata(p)
                .map(|m| !m.file_type().is_symlink())
                .unwrap_or(false)
        })
        .collect();
    out.sort();
    out
}

/// Find directories with one of `names` under `root`, up to `max_depth`,
/// without following links and without descending into a matched directory.
pub fn find_dirs_named(root: &Path, names: &[&str], max_depth: usize) -> Vec<PathBuf> {
    let mut out = Vec::new();
    find_dirs_inner(root, names, max_depth, 0, &mut out);
    out.sort();
    out
}

fn find_dirs_inner(
    dir: &Path,
    names: &[&str],
    max_depth: usize,
    depth: usize,
    out: &mut Vec<PathBuf>,
) {
    if depth > max_depth {
        return;
    }
    for child in child_dirs(dir) {
        let name = child
            .file_name()
            .map(|n| n.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        if names.iter().any(|n| *n == name) {
            out.push(child);
            continue;
        }
        // Never descend into dependency or VCS internals while searching.
        if matches!(name.as_str(), "node_modules" | ".git" | "target") {
            continue;
        }
        find_dirs_inner(&child, names, max_depth, depth + 1, out);
    }
}

pub fn file_name_str(p: &Path) -> String {
    p.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| p.to_string_lossy().to_string())
}

/// One analyzer job for [`all_reports`]: maps the resolved paths to the
/// reports that analyzer produces (zero, one, or several).
type AnalyzerJob = Box<dyn Fn(&DevPaths) -> Vec<ToolReport> + Sync>;

pub fn all_reports(paths: &DevPaths) -> Vec<ToolReport> {
    // The analyzers are independent and read-only over `paths`, so they run
    // in parallel. Each job maps to its reports through an indexed parallel
    // map, which preserves the job order — the report order (and therefore
    // the UI order) is identical to the serial version.
    let jobs: Vec<AnalyzerJob> = vec![
        Box::new(editors::analyze_all),
        Box::new(agents::analyze_all),
        Box::new(|p| node::analyze(p).into_iter().collect()),
        Box::new(|p| toolchains::analyze_rust(p).into_iter().collect()),
        Box::new(|p| toolchains::analyze_python(p).into_iter().collect()),
        Box::new(|p| dotnet::analyze(p).into_iter().collect()),
        Box::new(|p| models::analyze(p).into_iter().collect()),
    ];
    let parts: Vec<Vec<ToolReport>> = jobs.par_iter().map(|job| job(paths)).collect();
    let mut reports: Vec<ToolReport> = parts.into_iter().flatten().collect();
    reports.retain(|r| !r.is_empty());
    reports
}
