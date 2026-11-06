//! Directory listing backed by the scan index (no fresh filesystem walk).

use crate::model::{Category, DirNode};
use crate::scanner::DirIndex;

#[derive(Debug, Clone)]
pub struct ExplorerEntry {
    pub name: String,
    pub path: String,
    pub bytes: u64,
    pub is_dir: bool,
    pub category: Category,
}

pub fn list_directory(index: &DirIndex, path: &str) -> Vec<ExplorerEntry> {
    let mut out: Vec<ExplorerEntry> = index
        .children
        .get(path)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|c| ExplorerEntry {
            name: c.name,
            path: c.path,
            bytes: c.bytes,
            is_dir: c.is_dir,
            category: c.category,
        })
        .collect();
    out.sort_by(|a, b| b.bytes.cmp(&a.bytes).then(a.name.cmp(&b.name)));
    out
}

pub fn dir_node(index: &DirIndex, path: &str) -> Option<DirNode> {
    let bytes = *index.dir_sizes.get(path)?;
    let children = index.children.get(path).cloned().unwrap_or_default();
    Some(DirNode {
        path: path.to_string(),
        name: std::path::Path::new(path)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| path.to_string()),
        bytes,
        category: index
            .dir_category
            .get(path)
            .copied()
            .unwrap_or(Category::Unknown),
        file_count: children.iter().filter(|c| !c.is_dir).count() as u64,
        dir_count: children.iter().filter(|c| c.is_dir).count() as u64,
    })
}

pub fn breadcrumbs(path: &str) -> Vec<(String, String)> {
    let p = std::path::Path::new(path);
    let mut crumbs: Vec<(String, String)> = Vec::new();
    let mut acc = std::path::PathBuf::new();
    for comp in p.components() {
        acc.push(comp.as_os_str());
        crumbs.push((
            comp.as_os_str().to_string_lossy().to_string(),
            acc.to_string_lossy().to_string(),
        ));
    }
    crumbs
}
