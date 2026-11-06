pub mod rules;

use crate::model::{Category, Classification, Confidence};
use rules::{Rule, RuleScope};
use std::path::{Component, Path};

/// Abstraction over filesystem probing so classification logic stays testable.
pub trait Probe {
    fn exists(&self, path: &Path) -> bool;
    fn is_dir(&self, path: &Path) -> bool;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct FsProbe;
impl Probe for FsProbe {
    fn exists(&self, path: &Path) -> bool {
        path.exists()
    }
    fn is_dir(&self, path: &Path) -> bool {
        path.is_dir()
    }
}

fn components_lower(path: &Path) -> Vec<String> {
    path.components()
        .filter_map(|c| match c {
            Component::Normal(s) => Some(s.to_string_lossy().to_lowercase()),
            Component::RootDir => Some("/".to_string()),
            Component::Prefix(p) => Some(p.as_os_str().to_string_lossy().to_lowercase()),
            _ => None,
        })
        .collect()
}

fn rule_matches_component(rule: &Rule, comp: &str) -> bool {
    rule.names.contains(&comp)
}

fn rule_matches_path(rule: &Rule, norm: &str) -> bool {
    rule.path_fragments
        .iter()
        .any(|f| !f.is_empty() && norm.contains(f))
}

fn to_classification(rule: &Rule) -> Classification {
    Classification::new(
        rule.category,
        rule.id,
        rule.label,
        rule.explanation,
        rule.confidence,
    )
}

pub fn find_rule_for_component(comp: &str) -> Option<Rule> {
    let lower = comp.to_lowercase();
    rules::rules()
        .into_iter()
        .find(|r| rule_matches_component(r, &lower))
}

/// Classify a path on its own (deepest matching component wins, then path
/// fragments, then extension refinement, then fallback).
pub fn classify_path(path: &Path) -> Classification {
    classify_path_with_probe(path, &FsProbe)
}

pub fn classify_path_with_probe<P: Probe>(path: &Path, _probe: &P) -> Classification {
    classify_inner(path)
}

fn normalised_from(path: &Path, skip: usize) -> String {
    let comps: Vec<String> = path
        .components()
        .map(|c| c.as_os_str().to_string_lossy().to_lowercase())
        .collect();
    let kept: Vec<&String> = comps.iter().skip(skip).collect();
    kept.iter()
        .map(|s| s.as_str())
        .collect::<Vec<_>>()
        .join("/")
}

fn classify_inner(path: &Path) -> Classification {
    classify_inner_from(path, 0)
}

/// Like `classify_inner`, but the first `skip` path components are ignored:
/// ancestors above the scan root must never classify the tree beneath them
/// (a scan rooted inside a folder literally named "tmp" or "target" is not
/// itself a cache or build output).
pub fn classify_inner_from(path: &Path, skip: usize) -> Classification {
    let all_rules = rules::rules();
    let comps_all = components_lower(path);
    let comps: Vec<String> = comps_all.into_iter().skip(skip).collect();
    // Deepest component first: a leaf deep in the tree beats a container above it.
    for comp in comps.iter().rev() {
        for rule in &all_rules {
            if rule_matches_component(rule, comp) {
                return to_classification(rule);
            }
        }
    }
    let norm = normalised_from(path, skip);
    for rule in &all_rules {
        if rule_matches_path(rule, &norm) {
            return to_classification(rule);
        }
    }
    // Single-component filename rules (swap files etc. are in names table by filename)
    if let Some(name) = path.file_name().map(|n| n.to_string_lossy().to_lowercase()) {
        for rule in &all_rules {
            if rule.names.iter().any(|n| *n == name) {
                return to_classification(rule);
            }
        }
    }
    if let Some(ext) = extension_classification(path, None) {
        return ext;
    }
    Classification::unknown()
}

/// Extension refinement, applied when a file sits inside a container (or has
/// no folder classification at all). Returns None to keep inherited class.
pub fn extension_classification(
    path: &Path,
    inherited: Option<&Classification>,
) -> Option<Classification> {
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())?;
    if rules::binary_extensions().contains(&ext.as_str()) {
        // Only refine inside containers / unknown; never override a leaf.
        let base = Classification::new(
            Category::BinaryData,
            "ext.binary",
            "Binary data",
            "Large binary or archive format identified by its file extension.",
            Confidence::Medium,
        );
        return Some(match inherited {
            Some(parent) => base.inherited(parent.rule_id.clone()),
            None => base,
        });
    }
    if rules::log_extensions().contains(&ext.as_str()) {
        let base = Classification::new(
            Category::Logs,
            "ext.log",
            "Log file",
            "Log or dump file identified by its file extension.",
            Confidence::High,
        );
        return Some(match inherited {
            Some(parent) => base.inherited(parent.rule_id.clone()),
            None => base,
        });
    }
    None
}

/// Classify a directory entry given the classification inherited from its
/// parent. Leaf parents claim the whole subtree; container parents allow a
/// deeper rule to win.
pub fn classify_child(
    path: &Path,
    is_dir: bool,
    parent: Option<&(Classification, RuleScope)>,
) -> (Classification, RuleScope) {
    classify_child_from(path, is_dir, parent, 0)
}

pub fn classify_child_from(
    path: &Path,
    is_dir: bool,
    parent: Option<&(Classification, RuleScope)>,
    skip: usize,
) -> (Classification, RuleScope) {
    if let Some((parent_class, RuleScope::Leaf)) = parent {
        return (parent_class.clone(), RuleScope::Leaf);
    }
    let own = classify_inner_from(path, skip);
    if own.category != Category::Unknown {
        let scope = scope_of(&own.rule_id);
        // For files inside a container, extension refinement may apply.
        if !is_dir && scope == RuleScope::Container {
            if let Some(refined) = extension_classification(path, Some(&own)) {
                return (refined, RuleScope::Leaf);
            }
        }
        return (own, scope);
    }
    if !is_dir {
        if let Some((parent_class, _)) = parent {
            if let Some(refined) = extension_classification(path, Some(parent_class)) {
                return (refined, RuleScope::Leaf);
            }
        }
    }
    match parent {
        Some((parent_class, scope)) => (parent_class.clone(), *scope),
        None => (own, RuleScope::Container),
    }
}

pub fn scope_of(rule_id: &str) -> RuleScope {
    rules::rules()
        .into_iter()
        .find(|r| r.id == rule_id)
        .map(|r| r.scope)
        .unwrap_or(RuleScope::Container)
}

pub fn is_leaf(rule_id: &str) -> bool {
    scope_of(rule_id) == RuleScope::Leaf
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn leaf_node_modules() {
        let c = classify_path(&PathBuf::from("/home/u/proj/node_modules/pkg/index.js"));
        assert_eq!(c.category, Category::BuildArtifacts);
    }
    #[test]
    fn downloads_container() {
        let c = classify_path(&PathBuf::from("/home/u/Downloads/file.zip"));
        // deepest component is the file; folder rule wins via components scan
        assert!(c.category == Category::Downloads || c.category == Category::BinaryData);
    }
}
