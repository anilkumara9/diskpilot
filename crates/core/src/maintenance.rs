//! Automatic-maintenance policy: pure scheduling and purge-selection
//! rules. The shell owns the clock, the settings store, and every
//! filesystem action; this module only decides *whether* a run is due
//! and *which* Recycle Bin items a run may purge, so both decisions
//! are testable without touching a disk.
//!
//! The bias is conservative throughout: a run is due only when the
//! full interval has clearly elapsed, and a bin item is purged only
//! when it is strictly older than the retention cutoff — an item
//! deleted exactly at the cutoff is kept.

use serde::{Deserialize, Serialize};

const HOUR_MS: i64 = 3_600_000;
const DAY_MS: i64 = 86_400_000;

/// How often automatic maintenance runs. Mirrors the cadence names
/// the Watch settings already use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MaintenanceFrequency {
    Daily,
    Weekly,
}

impl MaintenanceFrequency {
    /// Minimum elapsed time since the last run before the next run is
    /// due. Deliberately a little under the nominal interval (20 h
    /// for daily, 6.5 d for weekly) so a run scheduled at the same
    /// time of day is not pushed a full extra day by small clock or
    /// scheduling drift.
    fn interval_ms(self) -> i64 {
        match self {
            MaintenanceFrequency::Daily => 20 * HOUR_MS,
            MaintenanceFrequency::Weekly => 13 * DAY_MS / 2,
        }
    }
}

/// Whether automatic maintenance is due at `now_ms`.
///
/// A maintenance that has never run is always due. Clock skew (the
/// recorded last run sitting in the future) is never due: the elapsed
/// computation saturates at zero rather than wrapping negative.
pub fn maintenance_due(
    last_run_ms: Option<i64>,
    now_ms: i64,
    frequency: MaintenanceFrequency,
) -> bool {
    let Some(last) = last_run_ms else {
        return true;
    };
    now_ms.saturating_sub(last) >= frequency.interval_ms()
}

/// Select the Recycle Bin item ids a maintenance run may purge:
/// exactly those whose deletion time is strictly older than
/// `min_age_days` before `now_ms`. An item deleted exactly at the
/// cutoff is NOT purged (strict inequality). `min_age_days` is
/// floored at 1 — a zero-day retention would purge items deleted a
/// moment ago, which no automatic run may ever do. Input order is
/// preserved.
pub fn select_bin_purge(items: &[(String, i64)], now_ms: i64, min_age_days: u32) -> Vec<String> {
    let age_days = i64::from(min_age_days.max(1));
    let cutoff = now_ms.saturating_sub(age_days.saturating_mul(DAY_MS));
    items
        .iter()
        .filter(|(_, deleted_at_ms)| *deleted_at_ms < cutoff)
        .map(|(id, _)| id.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_700_000_000_000;

    #[test]
    fn never_run_is_due() {
        assert!(maintenance_due(None, NOW, MaintenanceFrequency::Daily));
        assert!(maintenance_due(None, NOW, MaintenanceFrequency::Weekly));
    }

    #[test]
    fn daily_threshold_is_twenty_hours() {
        let just_under = NOW - (20 * HOUR_MS - 1);
        let exactly = NOW - 20 * HOUR_MS;
        assert!(!maintenance_due(
            Some(just_under),
            NOW,
            MaintenanceFrequency::Daily
        ));
        assert!(maintenance_due(
            Some(exactly),
            NOW,
            MaintenanceFrequency::Daily
        ));
    }

    #[test]
    fn weekly_threshold_is_six_and_a_half_days() {
        let threshold = 13 * DAY_MS / 2;
        assert!(!maintenance_due(
            Some(NOW - (threshold - 1)),
            NOW,
            MaintenanceFrequency::Weekly
        ));
        assert!(maintenance_due(
            Some(NOW - threshold),
            NOW,
            MaintenanceFrequency::Weekly
        ));
    }

    #[test]
    fn future_last_run_is_never_due() {
        assert!(!maintenance_due(
            Some(NOW + DAY_MS),
            NOW,
            MaintenanceFrequency::Daily
        ));
        assert!(!maintenance_due(
            Some(NOW + DAY_MS),
            NOW,
            MaintenanceFrequency::Weekly
        ));
    }

    fn item(id: &str, deleted_at_ms: i64) -> (String, i64) {
        (id.to_string(), deleted_at_ms)
    }

    #[test]
    fn purge_selects_strictly_older_than_cutoff() {
        let cutoff = NOW - 30 * DAY_MS;
        let items = vec![
            item("much-older", cutoff - DAY_MS),
            item("one-ms-older", cutoff - 1),
            item("exactly-at-cutoff", cutoff),
            item("one-ms-newer", cutoff + 1),
            item("brand-new", NOW),
        ];
        assert_eq!(
            select_bin_purge(&items, NOW, 30),
            vec!["much-older".to_string(), "one-ms-older".to_string()]
        );
    }

    #[test]
    fn purge_floors_min_age_at_one_day() {
        let items = vec![
            item("two-days-ago", NOW - 2 * DAY_MS),
            item("twelve-hours-ago", NOW - 12 * HOUR_MS),
        ];
        // min_age_days = 0 must behave as 1, never as "purge everything".
        assert_eq!(
            select_bin_purge(&items, NOW, 0),
            vec!["two-days-ago".to_string()]
        );
    }

    #[test]
    fn purge_preserves_input_order_and_handles_empty() {
        let items = vec![
            item("b-old", NOW - 90 * DAY_MS),
            item("a-old", NOW - 60 * DAY_MS),
        ];
        assert_eq!(
            select_bin_purge(&items, NOW, 30),
            vec!["b-old".to_string(), "a-old".to_string()]
        );
        assert!(select_bin_purge(&[], NOW, 30).is_empty());
    }
}
