//! Space forecast: how fast is this drive filling up, and when does
//! it run out?
//!
//! Pure arithmetic over the scan-history time series the shell
//! persists (see `scan_history.json` in the shell): a least-squares
//! linear fit of scanned total bytes against time. The caller filters
//! history entries to a single scan root; this module applies the
//! recency window and the "enough data" rules.

use serde::{Deserialize, Serialize};

/// One point of the scan-history time series.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryPoint {
    /// When the scan finished, milliseconds since the Unix epoch.
    pub scanned_at_ms: i64,
    /// Total allocated bytes the scan measured for the root.
    pub total_bytes: u64,
    /// Free bytes on the root's drive at scan end, when known.
    pub drive_free_bytes: Option<u64>,
}

/// The result of fitting recent history: the current growth rate and,
/// when the drive is growing and its free space is known, when it
/// fills up.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Forecast {
    /// Fitted growth in bytes per day. Zero when there is not enough
    /// data to fit; negative when the drive is shrinking.
    pub daily_growth_bytes: f64,
    /// `free / growth` in days, when growth is positive and the
    /// latest point knows the free space.
    pub days_until_full: Option<f64>,
    /// `now + days_until_full`, milliseconds since the Unix epoch.
    pub projected_full_at_ms: Option<i64>,
    /// How many history points the fit used (after the 30-day window
    /// and the caller's root filter).
    pub points_used: usize,
}

impl Default for Forecast {
    fn default() -> Self {
        Self {
            daily_growth_bytes: 0.0,
            days_until_full: None,
            projected_full_at_ms: None,
            points_used: 0,
        }
    }
}

const DAY_MS: f64 = 86_400_000.0;
/// Only the last 30 days of history describe the current growth rate.
const WINDOW_MS: i64 = 30 * 86_400_000;
/// A fit needs points at least 12 hours apart; two scans minutes
/// apart describe scan noise, not a trend.
const MIN_SPAN_MS: i64 = 12 * 3_600_000;
/// Beyond this horizon a "when does it fill" answer is numerology,
/// not a forecast: a nearly-flat growth rate over 100 years of free
/// space reports growth with no deadline instead of a date
/// saturated at the edge of representable time.
const MAX_HORIZON_DAYS: f64 = 36_500.0;

/// Fit a forecast from `entries` (already filtered to one scan root)
/// as of `now_ms`.
pub fn forecast(entries: &[HistoryPoint], now_ms: i64) -> Forecast {
    let window_start = now_ms - WINDOW_MS;
    let mut points: Vec<&HistoryPoint> = entries
        .iter()
        .filter(|p| p.scanned_at_ms >= window_start && p.scanned_at_ms <= now_ms)
        .collect();
    points.sort_by_key(|p| p.scanned_at_ms);

    let mut result = Forecast {
        points_used: points.len(),
        ..Forecast::default()
    };
    if points.len() < 2 {
        return result;
    }
    let span_ms = points[points.len() - 1].scanned_at_ms - points[0].scanned_at_ms;
    if span_ms < MIN_SPAN_MS {
        return result;
    }

    // Least-squares slope of total_bytes against time in days.
    let n = points.len() as f64;
    let xs: Vec<f64> = points
        .iter()
        .map(|p| (p.scanned_at_ms - points[0].scanned_at_ms) as f64 / DAY_MS)
        .collect();
    let ys: Vec<f64> = points.iter().map(|p| p.total_bytes as f64).collect();
    let mean_x = xs.iter().sum::<f64>() / n;
    let mean_y = ys.iter().sum::<f64>() / n;
    let mut numerator = 0.0f64;
    let mut denominator = 0.0f64;
    for (x, y) in xs.iter().zip(ys.iter()) {
        numerator += (x - mean_x) * (y - mean_y);
        denominator += (x - mean_x) * (x - mean_x);
    }
    if denominator <= 0.0 {
        return result;
    }
    let daily_growth = numerator / denominator;
    result.daily_growth_bytes = daily_growth;

    // Days until full: the most recent point's free space divided by
    // the growth rate. Only meaningful while the drive is growing.
    if daily_growth > 0.0 {
        if let Some(free) = points.iter().rev().find_map(|p| p.drive_free_bytes) {
            let days = free as f64 / daily_growth;
            if days.is_finite() && days <= MAX_HORIZON_DAYS {
                result.days_until_full = Some(days);
                result.projected_full_at_ms = Some(now_ms + (days * DAY_MS) as i64);
            }
        }
    }
    result
}

#[cfg(test)]
mod tests {
    #[test]
    fn absurd_horizon_reports_growth_without_a_deadline() {
        // ~10 TB free growing 1 byte/day: the naive projection is
        // tens of billions of days and used to saturate the
        // projected date at the edge of i64. Beyond 100 years the
        // answer is "growing, no meaningful deadline".
        let pts = vec![
            HistoryPoint {
                scanned_at_ms: 0,
                total_bytes: 0,
                drive_free_bytes: Some(10_000_000_000_000),
            },
            HistoryPoint {
                scanned_at_ms: 2 * 86_400_000,
                total_bytes: 2,
                drive_free_bytes: Some(10_000_000_000_000),
            },
        ];
        let f = forecast(&pts, 2 * 86_400_000);
        assert_eq!(f.daily_growth_bytes, 1.0);
        assert_eq!(f.days_until_full, None);
        assert_eq!(f.projected_full_at_ms, None);
    }

    use super::*;

    const GB: u64 = 1_000_000_000;
    const NOW: i64 = 1_800_000_000_000;
    const DAY: i64 = 86_400_000;

    fn point(days_ago: i64, total_gb: u64, free_gb: Option<u64>) -> HistoryPoint {
        HistoryPoint {
            scanned_at_ms: NOW - days_ago * DAY,
            total_bytes: total_gb * GB,
            drive_free_bytes: free_gb.map(|g| g * GB),
        }
    }

    #[test]
    fn flat_series_never_fills() {
        let entries = [point(4, 100, Some(20)), point(0, 100, Some(20))];
        let f = forecast(&entries, NOW);
        assert_eq!(f.points_used, 2);
        assert!(
            f.daily_growth_bytes.abs() < 1.0,
            "growth {}",
            f.daily_growth_bytes
        );
        assert_eq!(f.days_until_full, None);
        assert_eq!(f.projected_full_at_ms, None);
    }

    #[test]
    fn steady_growth_projects_full_date() {
        // +1 GB/day for 5 days, 20 GB free at the latest scan.
        let entries = [
            point(5, 95, Some(25)),
            point(4, 96, Some(24)),
            point(3, 97, Some(23)),
            point(2, 98, Some(22)),
            point(1, 99, Some(21)),
            point(0, 100, Some(20)),
        ];
        let f = forecast(&entries, NOW);
        assert_eq!(f.points_used, 6);
        assert!(
            (f.daily_growth_bytes - GB as f64).abs() < GB as f64 * 0.01,
            "growth {}",
            f.daily_growth_bytes
        );
        let days = f.days_until_full.expect("days until full");
        assert!((days - 20.0).abs() < 0.2, "days {days}");
        let projected = f.projected_full_at_ms.expect("projected date");
        assert!((projected - (NOW + 20 * DAY)).abs() < DAY / 10);
    }

    #[test]
    fn single_point_cannot_forecast() {
        let entries = [point(0, 100, Some(20))];
        let f = forecast(&entries, NOW);
        assert_eq!(f.points_used, 1);
        assert_eq!(f.daily_growth_bytes, 0.0);
        assert_eq!(f.days_until_full, None);
    }

    #[test]
    fn empty_history_is_zeroed() {
        let f = forecast(&[], NOW);
        assert_eq!(f, Forecast::default());
    }

    #[test]
    fn points_older_than_thirty_days_are_excluded() {
        // The 40-day-old point would flatten the fit if it counted.
        let entries = [
            point(40, 100, Some(20)),
            point(2, 98, Some(22)),
            point(0, 100, Some(20)),
        ];
        let f = forecast(&entries, NOW);
        assert_eq!(f.points_used, 2);
        assert!(
            (f.daily_growth_bytes - GB as f64).abs() < GB as f64 * 0.01,
            "growth {}",
            f.daily_growth_bytes
        );
    }

    #[test]
    fn points_too_close_together_do_not_fit() {
        let mut recent = point(0, 101, Some(19));
        recent.scanned_at_ms = NOW - 3_600_000; // one hour before NOW
        let entries = [recent, point(0, 100, Some(20))];
        let f = forecast(&entries, NOW);
        assert_eq!(f.points_used, 2);
        assert_eq!(f.daily_growth_bytes, 0.0);
        assert_eq!(f.days_until_full, None);
    }

    #[test]
    fn shrinking_drive_never_fills() {
        let entries = [point(3, 103, Some(17)), point(0, 100, Some(20))];
        let f = forecast(&entries, NOW);
        assert!(f.daily_growth_bytes < 0.0);
        assert_eq!(f.days_until_full, None);
    }

    #[test]
    fn unknown_free_space_gives_growth_without_deadline() {
        let entries = [point(2, 98, None), point(0, 100, None)];
        let f = forecast(&entries, NOW);
        assert!(f.daily_growth_bytes > 0.0);
        assert_eq!(f.days_until_full, None);
        assert_eq!(f.projected_full_at_ms, None);
    }
}
