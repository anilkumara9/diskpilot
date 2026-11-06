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

/// The rule table, built once. `rules::rules()` constructs a fresh
/// vector on every call; classification runs per file, so the hot
/// paths share this cached copy. (The public `rules()` stays as is
/// for callers that want an owned table.)
fn rules_ref() -> &'static [Rule] {
    static RULES: std::sync::OnceLock<Vec<Rule>> = std::sync::OnceLock::new();
    RULES.get_or_init(rules::rules)
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
    rules_ref()
        .iter()
        .find(|r| rule_matches_component(r, &lower))
        .cloned()
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
    let all_rules = rules_ref();
    let comps_all = components_lower(path);
    let comps: Vec<String> = comps_all.into_iter().skip(skip).collect();
    // Deepest component first: a leaf deep in the tree beats a container above it.
    if let Some(m) = component_scan(all_rules, &comps) {
        return m;
    }
    let norm = normalised_from(path, skip);
    if let Some(m) = fragment_scan(all_rules, &norm) {
        return m;
    }
    // Single-component filename rules (swap files etc. are in names table by filename)
    if let Some(name) = path.file_name().map(|n| n.to_string_lossy().to_lowercase()) {
        for rule in all_rules {
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

/// Phase 1 of [`classify_inner_from`]: the deepest component (after
/// the skip horizon) that any rule names, rules in table order per
/// component.
fn component_scan(all_rules: &[Rule], comps_after_skip: &[String]) -> Option<Classification> {
    for comp in comps_after_skip.iter().rev() {
        for rule in all_rules {
            if rule_matches_component(rule, comp) {
                return Some(to_classification(rule));
            }
        }
    }
    None
}

/// Phase 1 for a single component: the first rule naming it.
fn component_match_one(all_rules: &[Rule], comp_lower: &str) -> Option<Classification> {
    all_rules
        .iter()
        .find(|r| rule_matches_component(r, comp_lower))
        .map(to_classification)
}

/// Phase 2 of [`classify_inner_from`]: the first rule (table order)
/// with a path fragment contained in the normalised path.
fn fragment_scan(all_rules: &[Rule], norm: &str) -> Option<Classification> {
    all_rules
        .iter()
        .find(|r| rule_matches_path(r, norm))
        .map(to_classification)
}

/// The longest path fragment in the table, in bytes. A fragment
/// match in a child path that is not already a match in the parent
/// must intersect the appended `/name` tail, so it is fully visible
/// in a window of this many bytes before the tail.
fn max_fragment_bytes() -> usize {
    static MAX: std::sync::OnceLock<usize> = std::sync::OnceLock::new();
    *MAX.get_or_init(|| {
        rules_ref()
            .iter()
            .flat_map(|r| r.path_fragments.iter())
            .map(|f| f.len())
            .max()
            .unwrap_or(0)
    })
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
    resolve_child(own, path, is_dir, parent)
}

/// Everything [`classify_child_from`] does once the child's own
/// standalone classification (`classify_inner_from` of its path) is
/// known: leaf inheritance, extension refinement, container
/// inheritance. Shared by the plain and memoized entry points so
/// they cannot drift apart.
fn resolve_child(
    own: Classification,
    path: &Path,
    is_dir: bool,
    parent: Option<&(Classification, RuleScope)>,
) -> (Classification, RuleScope) {
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
    rules_ref()
        .iter()
        .find(|r| r.id == rule_id)
        .map(|r| r.scope)
        .unwrap_or(RuleScope::Container)
}

pub fn is_leaf(rule_id: &str) -> bool {
    scope_of(rule_id) == RuleScope::Leaf
}

// ---------------------------------------------------------------------------
// Memoized per-directory classification (the scanner's fast path)
// ---------------------------------------------------------------------------

/// The classification state of one directory, memoized so its
/// children classify in ~O(1) amortized work each.
///
/// Classifying a child from scratch rebuilds the child's full
/// lower-cased component vector and normalised path string and
/// re-scans every ancestor component against every rule — on a
/// 2,000-deep tree that made a scan ~200× slower than the same
/// files laid out flat. But a child's standalone classification is
/// almost entirely determined by its parent's:
///
/// - Phase 1 (deepest named component wins) checks the child's own
///   name first; if no rule names it, the parent's phase-1 result
///   *is* the child's — the scan would walk the same ancestors and
///   stop at the same component.
/// - Phase 2 (path fragments) can only gain a match through the
///   appended `/name` tail (any match fully inside the parent's
///   normalised path was already the parent's phase-2 result), so
///   only a bounded window around the boundary needs re-checking,
///   plus the parent's winner at its table position.
/// - Phases 3–4 (filename rules, extension) depend on the child's
///   own name alone.
///
/// The memo carries exactly those intermediates down the walk, so
/// results are identical to [`classify_child_from`] on the full
/// path — the test suite asserts that equivalence over a corpus of
/// trees — while a directory's children cost one name scan each.
#[derive(Debug, Clone)]
pub struct ClassifyMemo {
    /// Total component count of the directory's path (the depth
    /// the phases compare against `skip`; the components themselves
    /// are not retained).
    depth: usize,
    /// Components above this directory never classify the tree.
    skip: usize,
    /// `normalised_from(path, skip)` for the directory.
    norm: String,
    /// Phase-1 result over the directory's after-skip components.
    component_match: Option<Classification>,
    /// Phase-2 result over the directory's normalised path.
    fragment_match: Option<Classification>,
    /// `classify_inner_from(path, skip)` for the directory itself.
    inner: Classification,
}

impl ClassifyMemo {
    /// The memo for a scan root, computed with the full (unmemoized)
    /// classification work — once per scan.
    pub fn root(path: &Path, skip: usize) -> Self {
        let all_rules = rules_ref();
        let comps = components_lower(path);
        let after_skip: Vec<String> = comps.iter().skip(skip).cloned().collect();
        let norm = normalised_from(path, skip);
        Self {
            depth: comps.len(),
            skip,
            fragment_match: fragment_scan(all_rules, &norm),
            norm,
            component_match: component_scan(all_rules, &after_skip),
            inner: classify_inner_from(path, skip),
        }
    }

    /// The directory's own standalone classification — identical to
    /// `classify_inner_from(path, skip)` for its path.
    pub fn classification(&self) -> Classification {
        self.inner.clone()
    }

    /// The phase outcomes for a child named `name_lower` (already
    /// lower-cased) of this memo's directory: (phase 1, phase 2,
    /// the child's normalised path).
    fn child_phases(&self, name_lower: &str) -> (Option<Classification>, Option<Classification>, String) {
        let all_rules = rules_ref();
        if self.depth < self.skip {
            // The child (at depth + 1 <= skip) is still at or
            // above the skip horizon: it has no kept components.
            // (The first-kept-component case — this memo sitting
            // exactly AT the horizon — flows through the general
            // path below, where the empty `norm` makes it scan the
            // child's name alone.)
            return (None, None, String::new());
        }
        // Phase 1: the child's own name is the deepest component;
        // if no rule names it, the parent's result stands.
        let component_match = component_match_one(all_rules, name_lower)
            .or_else(|| self.component_match.clone());
        // Phase 2: the parent's winner still matches (its fragment
        // lies inside the parent's normalised path, a prefix of the
        // child's); an earlier rule can only newly match through
        // the boundary, so scan in table order with the parent's
        // winner honoured at its position and every other rule
        // tested against the boundary window.
        if self.norm.is_empty() {
            // The parent sits exactly at the skip horizon; the
            // child is the first kept component, so its normalised
            // path is its name alone — scan it directly.
            let norm = name_lower.to_string();
            return (
                component_match,
                fragment_scan(all_rules, &norm),
                norm,
            );
        }
        let window = {
            let max = max_fragment_bytes();
            let mut start = self.norm.len().saturating_sub(max.saturating_sub(1));
            while start > 0 && !self.norm.is_char_boundary(start) {
                start -= 1;
            }
            format!("{}/{name_lower}", &self.norm[start..])
        };
        let fragment_match = all_rules
            .iter()
            .find(|rule| {
                self.fragment_match
                    .as_ref()
                    .is_some_and(|m| m.rule_id == rule.id)
                    || rule_matches_path(rule, &window)
            })
            .map(to_classification);
        let norm = format!("{}/{name_lower}", self.norm);
        (component_match, fragment_match, norm)
    }

    /// Standalone classification of a child of this memo's
    /// directory, from the phases: identical to
    /// `classify_inner_from(child_path, skip)`.
    fn child_inner(&self, name: &std::ffi::OsStr) -> Classification {
        let name_lower = name.to_string_lossy().to_lowercase();
        let (component_match, fragment_match, _) = self.child_phases(&name_lower);
        inner_from_phases(component_match, fragment_match, &name_lower, name)
    }

    /// The memo for a child directory named `name`.
    pub fn child(&self, name: &std::ffi::OsStr) -> ClassifyMemo {
        let name_lower = name.to_string_lossy().to_lowercase();
        let (component_match, fragment_match, norm) = self.child_phases(&name_lower);
        let inner = inner_from_phases(
            component_match.clone(),
            fragment_match.clone(),
            &name_lower,
            name,
        );
        ClassifyMemo {
            depth: self.depth + 1,
            skip: self.skip,
            norm,
            component_match,
            fragment_match,
            inner,
        }
    }
}

/// Phases 3–5 of `classify_inner_from` for a path whose first two
/// phases are known and whose file name is `name` (the extension
/// and filename rules look at the last component alone).
fn inner_from_phases(
    component_match: Option<Classification>,
    fragment_match: Option<Classification>,
    name_lower: &str,
    name: &std::ffi::OsStr,
) -> Classification {
    if let Some(m) = component_match {
        return m;
    }
    if let Some(m) = fragment_match {
        return m;
    }
    for rule in rules_ref() {
        if rule.names.contains(&name_lower) {
            return to_classification(rule);
        }
    }
    if let Some(ext) = extension_classification(Path::new(name), None) {
        return ext;
    }
    Classification::unknown()
}

/// Classify a child *file* of the memo's directory without
/// building the child's memo (files have no descendants to reuse
/// it). Identical results to [`classify_child_from`] on the full
/// path.
pub fn classify_file_memo(
    parent_memo: &ClassifyMemo,
    name: &std::ffi::OsStr,
    child_path: &Path,
    parent: Option<&(Classification, RuleScope)>,
) -> (Classification, RuleScope) {
    if let Some((parent_class, RuleScope::Leaf)) = parent {
        return (parent_class.clone(), RuleScope::Leaf);
    }
    let own = parent_memo.child_inner(name);
    resolve_child(own, child_path, false, parent)
}

/// Classify a child *directory* of the memo's directory, returning
/// the child's own memo alongside so the walk can descend with it.
/// Identical results to [`classify_child_from`] on the full path.
pub fn classify_dir_memo(
    parent_memo: &ClassifyMemo,
    name: &std::ffi::OsStr,
    child_path: &Path,
    parent: Option<&(Classification, RuleScope)>,
) -> (Classification, RuleScope, ClassifyMemo) {
    let child_memo = parent_memo.child(name);
    if let Some((parent_class, RuleScope::Leaf)) = parent {
        return (parent_class.clone(), RuleScope::Leaf, child_memo);
    }
    let own = child_memo.classification();
    let (class, scope) = resolve_child(own, child_path, true, parent);
    (class, scope, child_memo)
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

    fn key(c: &Classification) -> (Category, String, Option<String>) {
        (c.category, c.rule_id.clone(), c.inherited_from.clone())
    }

    /// The memoized classification the scanner uses must agree with
    /// the from-scratch classification on every node of a tree —
    /// including the fragment that only completes across a
    /// parent/child boundary (`pnpm` + `store` → "pnpm/store") and
    /// extension refinement inside containers.
    #[test]
    fn memo_matches_full_classification_over_a_tree() {
        let root = PathBuf::from("/home/u/proj");
        let skip = 3; // root components minus one, as Scanner::base_skip
        let root_memo = ClassifyMemo::root(&root, skip);
        assert_eq!(
            key(&root_memo.classification()),
            key(&classify_inner_from(&root, skip))
        );
        let root_class = classify_inner_from(&root, skip);
        let root_scope = scope_of(&root_class.rule_id);

        let child_dirs = [
            "cache",
            "store",
            "src",
            "logs",
            "temp",
            "node_modules",
            "berry",
            "x",
        ];
        let child_files = [
            "index.js",
            "model.gguf",
            "x.log",
            "archive.zip",
            "pagefile.sys",
            "data.parquet",
            "readme.md",
        ];
        let trees: Vec<Vec<&str>> = vec![
            vec![],
            vec!["node_modules"],
            vec!["node_modules", "pkg"],
            vec!["target"],
            vec![".git"],
            vec!["Downloads"],
            vec!["yarn"],
            vec!["yarn", "berry"],
            vec!["pnpm"],
            vec!["appdata"],
            vec!["appdata", "local"],
            vec!["appdata", "local", "temp"],
            vec!["sub"],
            vec!["sub", "deep"],
            vec!["docker"],
            vec![".cargo"],
            vec![".cargo", "registry"],
        ];
        for segs in &trees {
            // Walk the old chain and the memo chain down together.
            let mut path = root.clone();
            let mut memo = root_memo.clone();
            let mut parent = (root_class.clone(), root_scope);
            for seg in segs {
                path.push(seg);
                let (old_class, old_scope) = classify_child_from(
                    &path,
                    true,
                    Some(&(parent.0.clone(), parent.1)),
                    skip,
                );
                let (memo_class, memo_scope, child_memo) = classify_dir_memo(
                    &memo,
                    std::ffi::OsStr::new(seg),
                    &path,
                    Some(&(parent.0.clone(), parent.1)),
                );
                assert_eq!(
                    (key(&old_class), old_scope),
                    (key(&memo_class), memo_scope),
                    "dir {path:?}"
                );
                assert_eq!(
                    key(&child_memo.classification()),
                    key(&classify_inner_from(&path, skip)),
                    "inner {path:?}"
                );
                memo = child_memo;
                parent = (old_class, old_scope);
            }
            // Files under this directory, both entry points.
            for name in child_files {
                let file_path = path.join(name);
                let (old_class, old_scope) = classify_child_from(
                    &file_path,
                    false,
                    Some(&(parent.0.clone(), parent.1)),
                    skip,
                );
                let (memo_class, memo_scope) = classify_file_memo(
                    &memo,
                    std::ffi::OsStr::new(name),
                    &file_path,
                    Some(&(parent.0.clone(), parent.1)),
                );
                assert_eq!(
                    (key(&old_class), old_scope),
                    (key(&memo_class), memo_scope),
                    "file {file_path:?}"
                );
            }
            // One more directory level of children, both entry points.
            for name in child_dirs {
                let dir_path = path.join(name);
                let (old_class, old_scope) = classify_child_from(
                    &dir_path,
                    true,
                    Some(&(parent.0.clone(), parent.1)),
                    skip,
                );
                let (memo_class, memo_scope, _) = classify_dir_memo(
                    &memo,
                    std::ffi::OsStr::new(name),
                    &dir_path,
                    Some(&(parent.0.clone(), parent.1)),
                );
                assert_eq!(
                    (key(&old_class), old_scope),
                    (key(&memo_class), memo_scope),
                    "child dir {dir_path:?}"
                );
            }
        }
    }
}
