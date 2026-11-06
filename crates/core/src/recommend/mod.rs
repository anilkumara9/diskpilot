pub mod policies;

use crate::classifier::{self, rules::RuleScope};
use crate::model::*;
use crate::scanner::DirIndex;
use std::collections::HashMap;
use std::path::Path;

pub const MIN_RECOMMEND_BYTES: u64 = 1024;

fn slug(path: &str) -> String {
    path.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string()
}

/// Path segments split on either separator, matching how the cards
/// module reads paths (index paths use the host separator, but the
/// comparison below is separator-agnostic).
fn path_segments(path: &str) -> Vec<&str> {
    path.split(['/', '\\']).filter(|s| !s.is_empty()).collect()
}

/// The scan's top-level `Windows.old` directory, if any: a direct
/// child of the scan root whose name matches `windows.old`
/// case-insensitively. Returns the stored path string and its
/// allocated size. A `Windows.old` nested deeper is not the previous
/// installation root and does not qualify.
fn root_windows_old(summary: &ScanSummary, index: &DirIndex) -> Option<(String, u64)> {
    let root_segs = path_segments(&summary.root);
    let mut found: Option<(String, u64)> = None;
    for (path, bytes) in &index.dir_sizes {
        let segs = path_segments(path);
        if segs.len() == root_segs.len() + 1
            && segs[..root_segs.len()] == root_segs[..]
            && segs[root_segs.len()].eq_ignore_ascii_case("windows.old")
        {
            // Prefer the largest if a case-variant duplicate exists.
            if found.as_ref().map(|(_, b)| *bytes > *b).unwrap_or(true) {
                found = Some((path.clone(), *bytes));
            }
        }
    }
    found
}

/// The removal route shown for the previous Windows installation:
/// Windows' own storage tooling is the supported way to remove
/// `Windows.old`, so the recommendation is guidance, not an action.
const WINDOWS_OLD_GUIDANCE: &str = "Remove it through Windows Settings → System → Storage → Temporary files → Previous Windows installation, or Disk Cleanup (cleanmgr) → Clean up system files. DiskPilot does not delete this folder directly.";

fn is_session_orphan_dir(path: &str) -> bool {
    let norm = path.replace('\\', "/").to_lowercase();
    (norm.contains("/.claude/projects")
        || norm.contains("/.codex/sessions")
        || norm.contains("/.gemini/sessions")
        || norm.contains("/sessions"))
        && (norm.contains(".claude") || norm.contains(".codex") || norm.contains(".gemini"))
}

/// Build recommendations from a finished scan. Pure: explains only.
pub fn build_recommendations(summary: &ScanSummary, index: &DirIndex) -> Vec<Recommendation> {
    let mut candidates: Vec<Recommendation> = Vec::new();
    let mut dir_paths: Vec<&String> = index.dir_sizes.keys().collect();
    dir_paths.sort();

    // A previous Windows installation at the scan root gets a
    // dedicated guided-only recommendation below; the generic passes
    // must not also speak for the folder (or, in the Keep pass, for
    // its size as personal data).
    let windows_old = root_windows_old(summary, index);
    let windows_old_path: Option<&str> = windows_old.as_ref().map(|(p, _)| p.as_str());

    for dir_path in &dir_paths {
        if *dir_path == &summary.root {
            continue; // the scan root itself is never a cleanup candidate
        }
        if Some(dir_path.as_str()) == windows_old_path {
            continue;
        }
        let bytes = index.dir_sizes[*dir_path];
        if bytes < MIN_RECOMMEND_BYTES {
            continue;
        }
        let Some(class) = index.dir_classification.get(*dir_path) else {
            continue;
        };
        // Session transcripts for projects that no longer exist.
        if is_session_orphan_dir(dir_path) {
            // Orphan check: the sessions folder itself is the item; its parent
            // project-existence is judged by the devtools analyzers in depth,
            // here we surface it for review when it is sizeable.
            let policy = policies::session_policy();
            candidates.push(Recommendation {
                id: format!("rec-{}", slug(dir_path)),
                title: format!("{}: {}", policy.title_prefix, display_name(dir_path)),
                paths: vec![(*dir_path).clone()],
                bytes,
                tier: policy.tier,
                methods: policy.methods.clone(),
                why_detected: format!("Session data under {} totalling {} bytes.", dir_path, bytes),
                why_safe: policy.why_safe.to_string(),
                risks: policy.risks.to_string(),
                confidence: policy.confidence,
                alternative_command: policy.alternative_command.map(|s| s.to_string()),
                guided_only: false,
                guidance: None,
                source: "recommend.sessions".to_string(),
            });
            continue;
        }
        let Some(policy) = policies::policy_for_rule(&class.rule_id) else {
            continue;
        };
        let scope = classifier::scope_of(&class.rule_id);
        // Containers are expanded per child instead of recommended whole:
        // their cache children are leaves and produce their own items.
        if scope == RuleScope::Container {
            continue;
        }
        candidates.push(Recommendation {
            id: format!("rec-{}", slug(dir_path)),
            title: format!("{}: {}", policy.title_prefix, display_name(dir_path)),
            paths: vec![(*dir_path).clone()],
            bytes,
            tier: policy.tier,
            methods: policy.methods.clone(),
            why_detected: format!(
                "{} matched rule {} ({}) at {}.",
                class.label, class.rule_id, class.explanation, dir_path
            ),
            why_safe: policy.why_safe.to_string(),
            risks: policy.risks.to_string(),
            confidence: policy.confidence,
            alternative_command: policy.alternative_command.map(|s| s.to_string()),
            guided_only: false,
            guidance: None,
            source: format!("classifier:{}", class.rule_id),
        });
    }

    // Previous Windows installation: guided only. DiskPilot never
    // deletes `Windows.old` — the planner rejects guided-only items
    // and the safety gate treats the location as report-only — so the
    // recommendation carries the operating system's removal route.
    // Pushed before the nested-actionable dedup below, which then
    // also suppresses any generic candidate inside the folder.
    if let Some((path, bytes)) = &windows_old {
        candidates.push(Recommendation {
            id: "windows-old".to_string(),
            title: "Previous Windows installation (Windows.old)".to_string(),
            paths: vec![path.clone()],
            bytes: *bytes,
            tier: Tier::ReviewFirst,
            methods: vec![CleanupMethod::ManualReview],
            why_detected: format!(
                "A previous Windows installation folder was found at {} totalling {} bytes. Windows keeps it after an upgrade so the earlier installation can be restored.",
                path, bytes
            ),
            why_safe:
                "Once the current Windows installation is working well, the previous one is no longer needed day to day."
                    .to_string(),
            risks:
                "Removing it deletes the option to roll back to the previous Windows installation."
                    .to_string(),
            confidence: Confidence::High,
            alternative_command: None,
            guided_only: true,
            guidance: Some(WINDOWS_OLD_GUIDANCE.to_string()),
            source: "recommend.windows-old".to_string(),
        });
    }

    // Nested actionable dedup: if a path is inside another actionable
    // candidate, keep only the outer one. Keep-tier items never suppress.
    let actionable: Vec<String> = candidates
        .iter()
        .filter(|r| r.tier != Tier::Keep)
        .map(|r| r.paths[0].clone())
        .collect();
    candidates.retain(|r| {
        if r.tier == Tier::Keep {
            return true;
        }
        !actionable.iter().any(|other| {
            other != &r.paths[0]
                && r.paths[0].starts_with(&format!(
                    "{}{}",
                    other.trim_end_matches(['/', '\\']),
                    std::path::MAIN_SEPARATOR
                ))
        })
    });

    // Keep-tier awareness items for the largest personal-data folders.
    let mut user_dirs: Vec<(&String, &u64)> = index
        .dir_sizes
        .iter()
        .filter(|(p, b)| {
            **b >= MIN_RECOMMEND_BYTES
                && Some(p.as_str()) != windows_old_path
                && index
                    .dir_classification
                    .get(*p)
                    .map(|c| c.category == Category::UserData)
                    .unwrap_or(false)
                && p.matches(std::path::MAIN_SEPARATOR).count()
                    <= summary.root.matches(std::path::MAIN_SEPARATOR).count() + 1
        })
        .collect();
    user_dirs.sort_by(|a, b| b.1.cmp(a.1));
    for (p, b) in user_dirs.into_iter().take(5) {
        let policy = policies::keep_policy();
        candidates.push(Recommendation {
            id: format!("rec-keep-{}", slug(p)),
            title: format!("{}: {}", policy.title_prefix, display_name(p)),
            paths: vec![p.clone()],
            bytes: *b,
            tier: Tier::Keep,
            methods: policy.methods.clone(),
            why_detected: "Large folder of personal data found during the scan.".to_string(),
            why_safe: policy.why_safe.to_string(),
            risks: policy.risks.to_string(),
            confidence: policy.confidence,
            alternative_command: None,
            guided_only: false,
            guidance: None,
            source: "recommend.keep".to_string(),
        });
    }

    candidates.sort_by(|a, b| b.bytes.cmp(&a.bytes).then(a.id.cmp(&b.id)));
    // De-duplicate ids defensively (slug collisions on case-insensitive hosts).
    let mut seen: HashMap<String, usize> = HashMap::new();
    for r in &mut candidates {
        let n = seen.entry(r.id.clone()).or_insert(0);
        if *n > 0 {
            r.id = format!("{}-{}", r.id, n);
        }
        *n += 1;
    }
    candidates
}

fn display_name(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string())
}

/// Is every path in this recommendation regenerable (per its policy)?
pub fn is_regenerable(rec: &Recommendation) -> bool {
    if let Some(rule) = rec.source.strip_prefix("classifier:") {
        if let Some(p) = policies::policy_for_rule(rule) {
            return p.regenerable;
        }
    }
    false
}
