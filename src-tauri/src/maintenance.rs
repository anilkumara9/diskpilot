//! Automatic maintenance: the scheduled, conservative clean-up
//! the user opts into in Settings (`Settings::auto_maintenance`).
//!
//! A run does exactly two things, both bounded by policy that
//! lives in the core:
//!
//! 1. Recycle Bin purge — items whose deletion time is strictly
//!    older than the configured retention age
//!    (`maintenance::select_bin_purge`) are permanently deleted.
//!    Each item's `$I`/`$R` pair is re-validated and its size read
//!    from the server-side metadata before anything is removed,
//!    exactly like a manual empty.
//! 2. Temporary files — when a scan index is present, the scan's
//!    LikelySafe `cache.temp` recommendations are planned and
//!    executed permanently through the ordinary planner gate
//!    (`build_plan` + `take_plan_for_mode` + `execute_plan`). If
//!    the gate refuses permanent deletion for the plan, the temp
//!    step is skipped silently — maintenance never finds another
//!    way to delete.
//!
//! The run is recorded to History (operation `auto-maintenance`,
//! no item paths: bin items were already the user's own deletions
//! and temp items flow through the plan audit) and the
//! `last_maintenance_ms` stamp is set even on partial failure, so
//! a broken item cannot cause a retry storm. This runs on the
//! watch scheduler thread and must never surface an error: every
//! failure is folded into the recorded entry or dropped.

use tauri::{AppHandle, Manager};

use diskpilot_core::cleanup::{execute_plan, ExecuteMode, FileOps, RealFileOps};
use diskpilot_core::maintenance as policy;
use diskpilot_core::model::{now_ms, Tier};
use diskpilot_core::safety::SafetyContext;

use crate::commands::with_history;
use crate::ShellState;

/// Run maintenance if it is enabled and due. Called from the
/// watch scheduler: once shortly after startup and on every
/// scheduler tick thereafter (no second timer is created).
pub fn run_if_due(app: &AppHandle) {
    let state = app.state::<ShellState>();
    run(&state);
}

fn run(state: &ShellState) {
    let settings = match state.settings.lock() {
        Ok(settings) => settings.clone(),
        Err(_) => return,
    };
    let config = &settings.auto_maintenance;
    if !config.enabled {
        return;
    }
    if !policy::maintenance_due(settings.last_maintenance_ms, now_ms(), config.frequency) {
        return;
    }

    let mut reclaimed: u64 = 0;
    let mut errors: Vec<String> = Vec::new();

    let purged = purge_recycle_bin(config.min_age_days, &mut reclaimed, &mut errors);
    let temp_items = clean_temp_files(state, &mut reclaimed, &mut errors);

    let detail = format!(
        "Automatic maintenance (retention {} days): purged {} Recycle Bin item(s), cleared {} temporary item(s)",
        config.min_age_days, purged, temp_items
    );
    errors.truncate(50);
    let _ = with_history(state, |history| {
        history
            .record_action(
                now_ms(),
                "auto-maintenance",
                &[],
                reclaimed,
                &errors,
                Some(&detail),
                diskpilot_core::history::RemovedItemMode::Permanent,
            )
            .map(|_| ())
    });

    // Stamp the run even on partial failure (see the module
    // docs). A failed persist is ignored: the in-memory stamp
    // still prevents a tight retry loop this session.
    if let Ok(mut slot) = state.settings.lock() {
        slot.last_maintenance_ms = Some(now_ms());
        let snapshot = slot.clone();
        drop(slot);
        let _ = crate::persist_settings(&state.app_data_dir, &snapshot);
    }
}

/// One day in milliseconds — the core policy's own unit
/// (`maintenance::select_bin_purge` computes its cutoff with it),
/// recomputed here so the delete-time re-verification below uses
/// exactly the cutoff the selection used.
const DAY_MS: i64 = 86_400_000;

/// Permanently delete the Recycle Bin items the core's policy
/// selects (deleted strictly longer ago than the retention age).
/// Returns how many items were purged.
fn purge_recycle_bin(min_age_days: u32, reclaimed: &mut u64, errors: &mut Vec<String>) -> usize {
    let items = crate::recycle_bin::list_items();
    let aged: Vec<(String, i64)> = items
        .iter()
        .map(|item| (item.id.clone(), item.deleted_at_ms))
        .collect();
    let now = now_ms();
    let selected = policy::select_bin_purge(&aged, now, min_age_days);
    let cutoff = now.saturating_sub(i64::from(min_age_days.max(1)).saturating_mul(DAY_MS));
    let mut purged = 0usize;
    for id in &selected {
        // Re-validate and re-read the server-side metadata, like
        // the manual empty: what is recorded and summed comes from
        // the `$I…` file, never from the earlier listing.
        match crate::recycle_bin::validate_item_id(id) {
            Some((payload, info)) => {
                // Re-verify at delete time against the FRESH `$I`
                // record: the listing this run started from may be
                // stale by the time the delete happens. An item
                // whose fresh deletion time no longer falls before
                // the cutoff is skipped (and not counted), and an
                // item whose record can no longer be parsed is
                // never deleted at all — no purge ever acts on a
                // stale listing alone.
                let Some(item) = crate::recycle_bin::read_item(&payload, &info) else {
                    continue;
                };
                if item.deleted_at_ms >= cutoff {
                    continue;
                }
                let bytes = item.bytes;
                let outcome = RealFileOps
                    .permanent_delete(&payload)
                    .and_then(|_| std::fs::remove_file(&info).map_err(|e| e.to_string()));
                match outcome {
                    Ok(()) => {
                        purged += 1;
                        *reclaimed += bytes;
                    }
                    Err(message) => errors.push(format!("{id}: {message}")),
                }
            }
            None => errors.push(format!("{id}: no longer in the Recycle Bin")),
        }
    }
    purged
}

/// Permanently clear the current scan's temporary-files
/// recommendations through the planner gate. Returns how many
/// items were removed; every "not applicable" outcome (no scan,
/// no temp recommendations, the gate refusing permanent deletion)
/// is a silent zero.
fn clean_temp_files(state: &ShellState, reclaimed: &mut u64, errors: &mut Vec<String>) -> usize {
    let ctx = SafetyContext::new(state.scan_root());
    let plan = {
        let Ok(mut engine) = state.engine.lock() else {
            return 0;
        };
        let Some(index) = engine.index.clone() else {
            return 0;
        };
        let temp_ids: Vec<String> = engine
            .recommendations
            .iter()
            .filter(|rec| rec.source == "classifier:cache.temp" && rec.tier == Tier::LikelySafe)
            .map(|rec| rec.id.clone())
            .collect();
        if temp_ids.is_empty() {
            return 0;
        }
        let recs = engine.recommendations.clone();
        let plan = engine
            .planner
            .build_plan(&temp_ids, &recs, &index, &ctx, now_ms());
        if plan.items.is_empty() {
            return 0;
        }
        // The mode gate is the point: permanent deletion happens
        // only when the core says every item in the plan allows
        // it. A refusal skips the whole step.
        match engine
            .planner
            .take_plan_for_mode(&plan.id, ExecuteMode::PermanentDelete, now_ms())
        {
            Ok(plan) => plan,
            Err(_) => return 0,
        }
    };
    match execute_plan(&plan, ExecuteMode::PermanentDelete, &ctx, &RealFileOps) {
        Ok(result) => {
            for failure in &result.failed {
                errors.push(format!("{}: {}", failure.path, failure.message));
            }
            *reclaimed += result.reclaimed_bytes;
            result.succeeded.len()
        }
        Err(_) => 0,
    }
}
