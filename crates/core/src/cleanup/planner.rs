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

    pub fn build_plan(
        &mut self,
        recommendation_ids: &[String],
        recommendations: &[Recommendation],
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
                match safety::validate(&PathBuf::from(path), ctx) {
                    Ok(_) => items.push(PlanItem {
                        path: path.clone(),
                        bytes: rec.bytes / rec.paths.len().max(1) as u64,
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
        // Recompute item bytes honestly: a recommendation with several paths
        // stores per-path bytes in its plan item from the recommendation
        // total split above; single-path items keep the full amount.
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
