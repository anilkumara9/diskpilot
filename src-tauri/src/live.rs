//! Live index: keep the in-memory scan index fresh between scans
//! by following the NTFS USN change journal.
//!
//! Design (see `docs/ARCHITECTURE.md` for the full write-up):
//!
//! - After every completed scan, the journal position of the
//!   scanned root's volume is checkpointed to
//!   `usn_checkpoints.json` (keyed by volume root, e.g. `C:\`).
//! - A poller thread wakes about 15 s after startup and then every
//!   60 s: it reads the changes since the checkpoint, resolves
//!   their paths through the MFT, keeps the ones under the current
//!   scan root, and applies them to the in-memory index through
//!   the core's sound apply path (`diskpilot_core::live_apply`).
//! - Anything that cannot be applied exactly — a journal reset or
//!   wrap, a batch too large to be a normal minute of activity, a
//!   directory change, an unresolvable path — marks the index
//!   **stale** instead of guessing. A stale index heals only
//!   through a fresh scan; when the Watch scheduler is enabled the
//!   poller triggers that scan itself through the normal
//!   scheduled-scan path.
//!
//! Honest limits, by design:
//!
//! - The live index updates **only while DiskPilot is running**
//!   (the tray app counts). Changes made while it is closed are
//!   picked up by the next scan, exactly as before this feature.
//! - It needs the same volume access as the MFT scan path
//!   (administrator). Without it, the journal query fails, no
//!   checkpoint is kept, and polling quietly stays off — the app
//!   behaves exactly as if the setting were off.
//! - Freshness is "within about a minute" (the poll interval),
//!   never real-time.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use diskpilot_core::model::ScanSummary;
use diskpilot_core::scanner::Scanner;
use diskpilot_core::usn_volume::{self, UsnReadError};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use crate::ShellState;

/// One volume's journal position: the journal's identity and the
/// USN the next read resumes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Checkpoint {
    pub journal_id: u64,
    pub next_usn: i64,
}

/// Runtime live-index status, decorated onto the served scan
/// summary (never persisted with it).
#[derive(Debug, Default)]
pub struct LiveStatus {
    /// The index no longer provably matches the volume; only a
    /// fresh scan reconciles it.
    pub stale: bool,
    /// The most recent poll completed its journal read (a reset
    /// detection counts: the read machinery worked).
    pub last_poll_ok: bool,
}

/// A minute of ordinary activity never produces this many changes
/// under one scan root; beyond it, incremental application costs
/// more than the rescan it tries to avoid, and the batch is
/// treated as drift instead.
const MAX_APPLICABLE_PER_POLL: usize = 20_000;

const STARTUP_DELAY: Duration = Duration::from_secs(15);
const POLL_INTERVAL: Duration = Duration::from_secs(60);

// ---------------------------------------------------------------------------
// Checkpoint store
// ---------------------------------------------------------------------------

/// Load the checkpoint map from `path`. A missing, unreadable or
/// corrupt file is an empty map, never an error — losing a
/// checkpoint only costs the live index until the next scan
/// re-checkpoints.
pub fn load_checkpoints_from(path: &Path) -> HashMap<String, Checkpoint> {
    match std::fs::read_to_string(path) {
        Ok(text) => serde_json::from_str(&text).unwrap_or_else(|e| {
            eprintln!("diskpilot: ignoring corrupt USN checkpoints file: {e}");
            HashMap::new()
        }),
        Err(_) => HashMap::new(),
    }
}

/// Persist the checkpoint map to `path`. Failures are logged, not
/// propagated: the in-memory flow continues either way.
pub fn save_checkpoints_to(path: &Path, map: &HashMap<String, Checkpoint>) {
    match serde_json::to_string_pretty(map) {
        Ok(text) => {
            if let Err(e) = std::fs::write(path, text) {
                eprintln!("diskpilot: could not write USN checkpoints: {e}");
            }
        }
        Err(e) => eprintln!("diskpilot: could not serialize USN checkpoints: {e}"),
    }
}

fn load_checkpoints(state: &ShellState) -> HashMap<String, Checkpoint> {
    load_checkpoints_from(&state.usn_checkpoints_file())
}

fn save_checkpoints(state: &ShellState, map: &HashMap<String, Checkpoint>) {
    save_checkpoints_to(&state.usn_checkpoints_file(), map);
}

// ---------------------------------------------------------------------------
// Path helpers
// ---------------------------------------------------------------------------

/// The canonical volume root (`C:\`) a scan root lives on, or
/// `None` when it is not on a plain drive-letter volume (UNC
/// shares have no local USN journal to follow).
fn volume_of(root: &str) -> Option<String> {
    diskpilot_core::mft_volume::volume_prefix(Path::new(root))
}

/// Normalize a path for prefix comparison: one separator style,
/// no trailing separator (except a bare volume root), lowercased —
/// Windows paths are case-insensitive and the journal's spelling
/// (anchored at the volume root, MFT case) routinely differs from
/// the scan root's spelling as the user typed it.
fn normalize_for_compare(path: &str) -> String {
    let mut p = path.replace('/', "\\");
    while p.len() > 3 && p.ends_with('\\') {
        p.pop();
    }
    p.to_lowercase()
}

/// Is `path` the scan root itself or inside it? The boundary is a
/// separator, so `C:\data2` is not under `C:\data`.
pub fn path_is_under(path: &str, root: &str) -> bool {
    let path = normalize_for_compare(path);
    let root = normalize_for_compare(root);
    if path == root {
        return true;
    }
    // A bare volume root (`c:\`) already ends at a separator, so
    // everything on the volume is below it; any other root needs
    // the separator boundary at its end.
    path.len() > root.len()
        && path.starts_with(&root)
        && (root.ends_with('\\') || path.as_bytes()[root.len()] == b'\\')
}

// ---------------------------------------------------------------------------
// Status + summary decoration
// ---------------------------------------------------------------------------

fn set_stale(state: &ShellState, stale: bool) {
    if let Ok(mut live) = state.live.lock() {
        live.stale = stale;
    }
}

fn set_poll_ok(state: &ShellState, ok: bool) {
    if let Ok(mut live) = state.live.lock() {
        live.last_poll_ok = ok;
    }
}

fn is_stale(state: &ShellState) -> bool {
    state.live.lock().map(|live| live.stale).unwrap_or(false)
}

fn live_index_enabled(state: &ShellState) -> bool {
    state
        .settings
        .lock()
        .map(|settings| settings.live_index)
        .unwrap_or(false)
}

/// Decorate the summary the shell serves with the live-index
/// status fields. The stored summary always carries `false` for
/// both; only this served clone reflects the poller's state.
pub fn decorate(state: &ShellState, summary: &mut ScanSummary) {
    summary.index_stale = is_stale(state);
    summary.live_index_active = false;
    if !live_index_enabled(state) {
        return;
    }
    let Some(volume) = volume_of(&summary.root) else {
        return;
    };
    let has_checkpoint = load_checkpoints(state).contains_key(&volume);
    let last_poll_ok = state
        .live
        .lock()
        .map(|live| live.last_poll_ok)
        .unwrap_or(false);
    summary.live_index_active = has_checkpoint && last_poll_ok;
}

// ---------------------------------------------------------------------------
// Lifecycle: checkpoint refresh at scan end
// ---------------------------------------------------------------------------

/// Called after every completed scan. A completed scan reconciles
/// everything, so the stale flag clears; the checkpoint then moves
/// to the journal's current position (queried at scan end, so it
/// matches the index's freshness as closely as the design allows).
///
/// When the journal cannot be queried — no administrator volume
/// access, no journal on the volume — any checkpoint for the
/// volume is dropped and the live index quietly stays off.
pub async fn on_scan_completed(state: &ShellState, summary: &ScanSummary) {
    set_stale(state, false);
    if !live_index_enabled(state) {
        return;
    }
    let Some(volume) = volume_of(&summary.root) else {
        return;
    };
    let volume_path = PathBuf::from(&volume);
    let queried =
        tauri::async_runtime::spawn_blocking(move || usn_volume::query_journal(&volume_path)).await;
    let mut checkpoints = load_checkpoints(state);
    match queried {
        Ok(Ok(info)) => {
            checkpoints.insert(
                volume,
                Checkpoint {
                    journal_id: info.journal_id,
                    next_usn: info.next_usn,
                },
            );
        }
        _ => {
            checkpoints.remove(&volume);
        }
    }
    save_checkpoints(state, &checkpoints);
}

// ---------------------------------------------------------------------------
// Poller
// ---------------------------------------------------------------------------

/// Spawn the poller thread: one catch-up tick shortly after
/// startup, then a tick every minute for the life of the process.
pub fn spawn_poller(app: AppHandle) {
    std::thread::spawn(move || {
        std::thread::sleep(STARTUP_DELAY);
        loop {
            tauri::async_runtime::block_on(poll_once(&app));
            std::thread::sleep(POLL_INTERVAL);
        }
    });
}

async fn poll_once(app: &AppHandle) {
    let state = app.state::<ShellState>();
    if !live_index_enabled(&state) {
        return;
    }
    // Snapshot what this tick works on. No engine lock is ever held
    // across an await or a blocking journal read.
    let (root, has_index) = {
        let Ok(engine) = state.engine.lock() else {
            return;
        };
        if engine.scanning {
            return;
        }
        match engine.summary.as_ref() {
            Some(summary) => (summary.root.clone(), engine.index.is_some()),
            None => return,
        }
    };
    if !has_index {
        // A summary restored from the on-disk snapshot has no
        // in-memory index to keep fresh; live updates resume with
        // the first real scan of this session.
        set_poll_ok(&state, false);
        return;
    }
    let Some(volume) = volume_of(&root) else {
        set_poll_ok(&state, false);
        return;
    };
    let checkpoints = load_checkpoints(&state);
    let Some(checkpoint) = checkpoints.get(&volume).copied() else {
        set_poll_ok(&state, false);
        return;
    };

    let volume_path = PathBuf::from(&volume);
    let read = tauri::async_runtime::spawn_blocking(move || {
        usn_volume::read_changes(&volume_path, checkpoint.journal_id, checkpoint.next_usn)
    })
    .await;

    match read {
        Ok(Err(UsnReadError::Unavailable(_))) | Err(_) => {
            // No volume access (or the blocking task itself failed):
            // polling quietly stays off; the checkpoint is kept so
            // a later privilege change resumes where it left off.
            set_poll_ok(&state, false);
        }
        Ok(Err(UsnReadError::JournalReset)) => {
            // The journal was recreated or wrapped past the
            // checkpoint: those changes are gone for good. Mark
            // stale, re-checkpoint at the journal's head, and let
            // the heal step below reconcile by rescan if allowed.
            set_poll_ok(&state, true);
            set_stale(&state, true);
            recheckpoint(&state, &volume).await;
            heal_if_allowed(app, &state, &root).await;
        }
        Ok(Ok(batch)) => {
            set_poll_ok(&state, true);
            // The checkpoint advances even for a batch that ends up
            // stale: everything in it has been *considered*, and
            // the stale flag — not a re-read — is what tracks the
            // debt until the next scan.
            let mut checkpoints = load_checkpoints(&state);
            checkpoints.insert(
                volume.clone(),
                Checkpoint {
                    journal_id: checkpoint.journal_id,
                    next_usn: batch.next_usn,
                },
            );
            save_checkpoints(&state, &checkpoints);

            if batch.unresolved > 0 {
                // Some records' paths could not be resolved through
                // the MFT; they may belong under the scan root.
                set_stale(&state, true);
            }
            let applicable: Vec<_> = batch
                .changes
                .into_iter()
                .filter(|change| {
                    change
                        .path
                        .as_deref()
                        .is_some_and(|path| path_is_under(path, &root))
                })
                .collect();
            if applicable.len() > MAX_APPLICABLE_PER_POLL {
                set_stale(&state, true);
            } else if !applicable.is_empty() {
                apply_batch(&state, &root, &applicable);
            }
            if is_stale(&state) {
                heal_if_allowed(app, &state, &root).await;
            }
        }
    }
}

/// Re-point a volume's checkpoint at the journal's current head
/// after a reset. If the journal can no longer be queried at all,
/// the checkpoint is dropped (polling stops until the next scan).
async fn recheckpoint(state: &ShellState, volume: &str) {
    let volume_path = PathBuf::from(volume);
    let queried =
        tauri::async_runtime::spawn_blocking(move || usn_volume::query_journal(&volume_path)).await;
    let mut checkpoints = load_checkpoints(state);
    match queried {
        Ok(Ok(info)) => {
            checkpoints.insert(
                volume.to_string(),
                Checkpoint {
                    journal_id: info.journal_id,
                    next_usn: info.next_usn,
                },
            );
        }
        _ => {
            checkpoints.remove(volume);
        }
    }
    save_checkpoints(state, &checkpoints);
}

/// Apply one poll's worth of resolved, in-scope changes to the
/// in-memory index, then rebuild the summary (and therefore the
/// recommendations and cleanup cards) from the mutated index
/// through the same store a scan uses. Any unsound change flips
/// the stale flag instead of being approximated.
fn apply_batch(state: &ShellState, root: &str, changes: &[usn_volume::ResolvedChange]) {
    let exclusions = state
        .settings
        .lock()
        .map(|settings| settings.exclusions.clone())
        .unwrap_or_default();
    let Ok(mut engine) = state.engine.lock() else {
        return;
    };
    // The world may have moved since the tick started (a scan may
    // have begun, or the root changed). Bail without touching
    // anything; the next tick works from the new state.
    if engine.scanning {
        return;
    }
    // Clone the current summary first (it is the `previous` the
    // rebuilt summary carries provenance from), so no borrow of the
    // engine is alive while the index is taken out.
    let Some(previous) = engine.summary.clone() else {
        return;
    };
    if previous.root != root {
        return;
    }
    let Some(mut index) = engine.index.take() else {
        return;
    };
    let report = diskpilot_core::live_apply::apply_changes(
        &mut index,
        Path::new(root),
        &exclusions,
        changes,
    );
    if report.became_stale {
        set_stale(state, true);
    }
    if report.removed + report.upserted > 0 {
        let summary = Scanner::new(root).summarize_existing(&index, &previous);
        engine.store_scan(summary, index);
    } else {
        engine.index = Some(index);
    }
}

/// The healing path for a stale index: a background rescan through
/// the ordinary scheduled-scan machinery — but only when the user
/// has the Watch scheduler enabled (any schedule at all). With
/// Watch off, the stale flag is the whole surface until the user
/// scans again themselves.
async fn heal_if_allowed(app: &AppHandle, state: &ShellState, root: &str) {
    let watch_enabled = state
        .settings
        .lock()
        .map(|settings| settings.auto_scan != "off")
        .unwrap_or(false);
    if !watch_enabled {
        return;
    }
    let scanning = state
        .engine
        .lock()
        .map(|engine| engine.scanning)
        .unwrap_or(true);
    if scanning {
        return;
    }
    if let Err(e) = crate::commands::run_scan(app.clone(), state, root.to_string(), true).await {
        eprintln!("diskpilot: live-index heal rescan failed: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checkpoint_store_roundtrips() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("usn_checkpoints.json");
        let mut map = HashMap::new();
        map.insert(
            "C:\\".to_string(),
            Checkpoint {
                journal_id: 0x0123_4567_89ab_cdef,
                next_usn: 987_654_321,
            },
        );
        map.insert(
            "D:\\".to_string(),
            Checkpoint {
                journal_id: 7,
                next_usn: -42,
            },
        );
        save_checkpoints_to(&path, &map);
        let loaded = load_checkpoints_from(&path);
        assert_eq!(loaded, map);
    }

    #[test]
    fn corrupt_checkpoint_file_loads_as_empty() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("usn_checkpoints.json");
        std::fs::write(&path, "{ this is not the checkpoint map").unwrap();
        assert!(load_checkpoints_from(&path).is_empty());
        // A missing file is equally an empty map.
        let missing = dir.path().join("never-written.json");
        assert!(load_checkpoints_from(&missing).is_empty());
    }

    #[test]
    fn prefix_filtering_is_boundary_and_case_aware() {
        // The root itself and anything below it count.
        assert!(path_is_under(r"C:\data", r"C:\data"));
        assert!(path_is_under(r"C:\data\sub\file.txt", r"C:\data"));
        assert!(path_is_under(r"c:\DATA\Sub\FILE.txt", r"C:\Data"));
        // A sibling sharing a name prefix does not.
        assert!(!path_is_under(r"C:\data2\file.txt", r"C:\data"));
        assert!(!path_is_under(r"C:\database", r"C:\data"));
        // Other volumes and shallower paths do not.
        assert!(!path_is_under(r"D:\data\file.txt", r"C:\data"));
        assert!(!path_is_under(r"C:\", r"C:\data"));
        // A volume root scan covers the whole volume; spellings
        // with forward slashes or trailing separators still match.
        assert!(path_is_under(r"C:\anywhere\file.txt", "C:\\"));
        assert!(path_is_under("C:/data/file.txt", "C:\\data\\"));
    }
}
