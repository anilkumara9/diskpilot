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

    for dir_path in &dir_paths {
        if *dir_path == &summary.root {
            continue; // the scan root itself is never a cleanup candidate
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
            source: format!("classifier:{}", class.rule_id),
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
