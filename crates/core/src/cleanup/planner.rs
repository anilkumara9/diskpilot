//! Cleanup planner. The UI sends recommendation IDs only; paths are resolved
//! from the engine's own recommendations, validated through the safety gate,
//! and frozen into a single-use plan that expires after ten minutes.

use crate::cards::CandidateGroup;
use crate::error::{CoreError, CoreResult};
use crate::model::*;
use crate::recommend;
use crate::safety::{self, SafetyContext};
use std::collections::HashMap;
use std::path::PathBuf;

pub const PLAN_TTL_MS: i64 = 10 * 60 * 1000;

#[derive(Debug, Default)]
pub struct Planner {
    pub plans: HashMap<String, CleanupPlan>,
    used: HashMap<String, bool>,
    counter: u64,
}

impl Planner {
    pub fn new() -> Self {
        Self::default()
    }

    /// Build a plan from recommendation IDs.
    ///
    /// Item bytes are never derived from the recommendation's
    /// aggregate: each path carries the bytes the scan index
    /// measured for that exact path (a file's allocated bytes, a
    /// directory's aggregated total). A path the index does not
    /// contain cannot be verified, so it is rejected — a plan only
    /// ever promises bytes that were actually measured, and its
    /// total is the sum of its items.
    pub fn build_plan(
        &mut self,
        recommendation_ids: &[String],
        recommendations: &[Recommendation],
        index: &crate::scanner::DirIndex,
        ctx: &SafetyContext,
        now: i64,
    ) -> CleanupPlan {
        let mut items: Vec<PlanItem> = Vec::new();
        let mut warnings: Vec<String> = Vec::new();
        let mut rejected: Vec<String> = Vec::new();
        let mut all_regenerable = true;
        let mut seen_paths: Vec<String> = Vec::new();

        for rid in recommendation_ids {
            let Some(rec) = recommendations.iter().find(|r| &r.id == rid) else {
                rejected.push(format!("unknown recommendation id: {}", rid));
                all_regenerable = false;
                continue;
            };
            if rec.tier == Tier::Keep {
                rejected.push(format!(
                    "{} is marked Keep and is never cleaned automatically",
                    rec.title
                ));
                all_regenerable = false;
                continue;
            }
            if rec.guided_only {
                // Guided-only recommendations (e.g. Windows.old) are
                // advice, never actions: no plan may contain them.
                // The safety gate would reject their paths anyway;
                // rejecting here names the reason and the route.
                rejected.push(format!(
                    "{} is guided only and is never cleaned by DiskPilot: {}",
                    rec.title,
                    rec.guidance.clone().unwrap_or_else(|| {
                        "follow the operating system's own removal route".to_string()
                    })
                ));
                all_regenerable = false;
                continue;
            }
            if rec
                .methods
                .iter()
                .any(|m| matches!(m, CleanupMethod::RunCommand | CleanupMethod::ManualReview))
                && !rec.methods.contains(&CleanupMethod::MoveToTrash)
            {
                rejected.push(format!(
                    "{} must be handled with its own command or manually: {}",
                    rec.title,
                    rec.alternative_command.clone().unwrap_or_default()
                ));
                all_regenerable = false;
                continue;
            }
            if !recommend::is_regenerable(rec) {
                all_regenerable = false;
            }
            for path in &rec.paths {
                if seen_paths.contains(path) {
                    continue;
                }
                seen_paths.push(path.clone());
                let Some(bytes) = measured_bytes(index, path) else {
                    rejected.push(format!(
                        "{}: not in the current scan, so its size cannot be verified",
                        path
                    ));
                    continue;
                };
                match safety::validate(&PathBuf::from(path), ctx) {
                    Ok(_) => items.push(PlanItem {
                        path: path.clone(),
                        bytes,
                    }),
                    Err(e) => rejected.push(format!("{}: {}", path, e)),
                }
            }
            if rec.tier == Tier::ReviewFirst {
                warnings.push(format!(
                    "{} is marked Review first: {}",
                    rec.title, rec.risks
                ));
            }
        }
        let allow_permanent =
            all_regenerable && !recommendation_ids.is_empty() && !items.is_empty();
        self.register_plan(now, items, warnings, rejected, allow_permanent)
    }

    /// Build a plan from Clean Up card candidate IDs.
    ///
    /// The safety model is identical to [`Planner::build_plan`]: the
    /// caller sends IDs only, paths are resolved against `groups` —
    /// card data the core itself computed from its own scan index —
    /// and every resolved path passes the same safety gate before it
    /// enters the plan. Unknown IDs land in the plan's `rejected` list
    /// rather than failing the build, mirroring the recommendation
    /// flow, so this returns `Ok` for any ID list.
    ///
    /// One deliberate difference: `allow_permanent` is **always false**.
    /// Card candidates are ordinary user files (installers, downloads,
    /// photos, videos), not regenerable caches, so card plans can only
    /// ever be executed as Move to Recycle Bin — the executor refuses
    /// permanent deletion for plans that do not allow it.
    pub fn build_plan_for_candidates(
        &mut self,
        candidate_ids: &[String],
        groups: &[CandidateGroup],
        ctx: &SafetyContext,
        now_ms: i64,
    ) -> CoreResult<CleanupPlan> {
        let mut items: Vec<PlanItem> = Vec::new();
        let mut rejected: Vec<String> = Vec::new();
        let mut seen_paths: Vec<String> = Vec::new();
        for cid in candidate_ids {
            let Some(candidate) = groups
                .iter()
                .flat_map(|g| g.files.iter())
                .find(|f| &f.id == cid)
            else {
                rejected.push(format!("unknown candidate id: {}", cid));
                continue;
            };
            if seen_paths.contains(&candidate.path) {
                continue;
            }
            seen_paths.push(candidate.path.clone());
            match safety::validate(&PathBuf::from(&candidate.path), ctx) {
                Ok(_) => items.push(PlanItem {
                    path: candidate.path.clone(),
                    bytes: candidate.bytes,
                }),
                Err(e) => rejected.push(format!("{}: {}", candidate.path, e)),
            }
        }
        Ok(self.register_plan(now_ms, items, Vec::new(), rejected, false))
    }

    /// Mint, store and return a plan. The single place plans are
    /// created, so every entry point shares the ID scheme, the
    /// ten-minute expiry and the single-use bookkeeping enforced by
    /// [`Planner::take_plan`].
    fn register_plan(
        &mut self,
        now: i64,
        items: Vec<PlanItem>,
        warnings: Vec<String>,
        rejected: Vec<String>,
        allow_permanent: bool,
    ) -> CleanupPlan {
        self.counter += 1;
        let id = format!("plan-{}-{}", now, self.counter);
        let total_bytes: u64 = items.iter().map(|i| i.bytes).sum();
        let plan = CleanupPlan {
            id: id.clone(),
            created_at_ms: now,
            expires_at_ms: now + PLAN_TTL_MS,
            items,
            warnings,
            rejected,
            total_bytes,
            allow_permanent,
        };
        self.plans.insert(id, plan.clone());
        plan
    }

    /// Take a plan for execution in a specific mode, validating
    /// the mode against the plan BEFORE the plan is consumed: a
    /// permanent-delete request against a plan that does not allow
    /// it errors with the plan intact, so the caller can still
    /// execute the same plan as a move-to-trash afterwards. (Taking
    /// first and letting the executor reject the mode burned the
    /// single-use plan on a no-op.)
    pub fn take_plan_for_mode(
        &mut self,
        plan_id: &str,
        mode: super::executor::ExecuteMode,
        now: i64,
    ) -> CoreResult<CleanupPlan> {
        if mode == super::executor::ExecuteMode::PermanentDelete {
            let plan = self
                .plans
                .get(plan_id)
                .ok_or_else(|| CoreError::PlanNotFound(plan_id.to_string()))?;
            if !plan.allow_permanent {
                return Err(CoreError::SafetyRejected(
                    "permanent delete is only allowed when every item is regenerable".to_string(),
                ));
            }
        }
        self.take_plan(plan_id, now)
    }

    /// Take a plan for execution: enforces existence, expiry and single use.
    pub fn take_plan(&mut self, plan_id: &str, now: i64) -> CoreResult<CleanupPlan> {
        let plan = self
            .plans
            .get(plan_id)
            .cloned()
            .ok_or_else(|| CoreError::PlanNotFound(plan_id.to_string()))?;
        if self.used.get(plan_id).copied().unwrap_or(false) {
            return Err(CoreError::PlanAlreadyUsed(plan_id.to_string()));
        }
        if now > plan.expires_at_ms {
            return Err(CoreError::PlanExpired(plan_id.to_string()));
        }
        self.used.insert(plan_id.to_string(), true);
        Ok(plan)
    }

    pub fn get_plan(&self, plan_id: &str) -> Option<&CleanupPlan> {
        self.plans.get(plan_id)
    }
}

/// The bytes the scan index measured for one exact path: a
/// directory's aggregated total, or a file's allocated bytes.
/// `None` when the index never saw the path.
fn measured_bytes(index: &crate::scanner::DirIndex, path: &str) -> Option<u64> {
    if let Some(bytes) = index.dir_sizes.get(path) {
        return Some(*bytes);
    }
    index.files.iter().find(|f| f.path == path).map(|f| f.bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Category, Confidence, FileEntry};
    use crate::scanner::DirIndex;

    fn rec(id: &str, paths: Vec<String>, bytes: u64) -> Recommendation {
        Recommendation {
            id: id.to_string(),
            title: format!("Rec {id}"),
            paths,
            bytes,
            tier: Tier::LikelySafe,
            methods: vec![CleanupMethod::MoveToTrash],
            why_detected: String::new(),
            why_safe: String::new(),
            risks: String::new(),
            confidence: Confidence::High,
            alternative_command: None,
            guided_only: false,
            guidance: None,
            source: "test".to_string(),
        }
    }

    fn file_entry(path: &str, bytes: u64) -> FileEntry {
        FileEntry {
            path: path.to_string(),
            name: path.rsplit('/').next().unwrap_or(path).to_string(),
            bytes,
            category: Category::Cache,
            modified_ms: 0,
            ocr_text: None,
        }
    }

    fn fixture() -> (tempfile::TempDir, SafetyContext, DirIndex, Vec<String>) {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let a = root.join("a.bin");
        let b = root.join("b.bin");
        std::fs::write(&a, b"x").unwrap();
        std::fs::write(&b, b"x").unwrap();
        let a = a.to_string_lossy().to_string();
        let b = b.to_string_lossy().to_string();
        let mut index = DirIndex::default();
        index.files.push(file_entry(&a, 1_052_672));
        index.files.push(file_entry(&b, 2_097_152));
        let ctx = SafetyContext::new(root).with_home(root);
        (dir, ctx, index, vec![a, b])
    }

    /// The audit's repro: two paths with different measured sizes
    /// must keep them (the old code split the recommendation total
    /// evenly, 1,574,912 each), and the total is their sum.
    #[test]
    fn plan_items_carry_measured_per_path_bytes() {
        let (_dir, ctx, index, paths) = fixture();
        let recs = vec![rec("r1", paths.clone(), 1_052_672 + 2_097_152)];
        let mut planner = Planner::new();
        let plan = planner.build_plan(&["r1".to_string()], &recs, &index, &ctx, 1_000);
        assert_eq!(plan.items.len(), 2);
        assert_eq!(plan.items[0].path, paths[0]);
        assert_eq!(plan.items[0].bytes, 1_052_672);
        assert_eq!(plan.items[1].path, paths[1]);
        assert_eq!(plan.items[1].bytes, 2_097_152);
        assert_eq!(plan.total_bytes, 1_052_672 + 2_097_152);
        assert!(plan.rejected.is_empty());
    }

    /// A path missing from the index is dropped with a rejection,
    /// and the total reflects only what remains.
    #[test]
    fn unmeasured_path_is_dropped_and_total_follows() {
        let (_dir, ctx, index, paths) = fixture();
        let mut all = paths.clone();
        all.push(
            ctx.scan_root
                .join("ghost.bin")
                .to_string_lossy()
                .to_string(),
        );
        let recs = vec![rec("r1", all, 9_999_999)];
        let mut planner = Planner::new();
        let plan = planner.build_plan(&["r1".to_string()], &recs, &index, &ctx, 1_000);
        assert_eq!(plan.items.len(), 2);
        assert_eq!(plan.total_bytes, 1_052_672 + 2_097_152);
        assert_eq!(plan.rejected.len(), 1);
        assert!(plan.rejected[0].contains("ghost.bin"));
        assert!(plan.rejected[0].contains("cannot be verified"));
    }

    /// A directory path is measured from the index's dir totals.
    #[test]
    fn directory_paths_measure_from_dir_sizes() {
        let (_dir, ctx, mut index, _paths) = fixture();
        let sub = ctx.scan_root.join("cache-dir");
        std::fs::create_dir_all(&sub).unwrap();
        let sub = sub.to_string_lossy().to_string();
        index.dir_sizes.insert(sub.clone(), 42_000);
        let recs = vec![rec("r1", vec![sub], 1)];
        let mut planner = Planner::new();
        let plan = planner.build_plan(&["r1".to_string()], &recs, &index, &ctx, 1_000);
        assert_eq!(plan.items.len(), 1);
        assert_eq!(plan.items[0].bytes, 42_000);
        assert_eq!(plan.total_bytes, 42_000);
    }

    /// A mode mismatch must not consume the plan: after a refused
    /// permanent-delete take, the same plan still takes for trash.
    #[test]
    fn mode_mismatch_does_not_consume_the_plan() {
        let (_dir, ctx, index, paths) = fixture();
        // source "test" is not a classifier rule, so the plan is
        // not regenerable and never allows permanent deletion.
        let recs = vec![rec("r1", paths, 10)];
        let mut planner = Planner::new();
        let plan = planner.build_plan(&["r1".to_string()], &recs, &index, &ctx, 1_000);
        assert!(!plan.allow_permanent);
        let refused = planner.take_plan_for_mode(
            &plan.id,
            super::super::executor::ExecuteMode::PermanentDelete,
            1_100,
        );
        assert!(refused.is_err());
        // The plan is intact: a trash take still succeeds, and the
        // single-use rule still applies after it.
        let taken = planner.take_plan_for_mode(
            &plan.id,
            super::super::executor::ExecuteMode::MoveToTrash,
            1_200,
        );
        assert!(taken.is_ok());
        assert!(planner.take_plan(&plan.id, 1_300).is_err());
    }
}
