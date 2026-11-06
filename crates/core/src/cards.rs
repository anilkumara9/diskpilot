//! File-level Clean Up cards: old installers, forgotten downloads, old
//! screenshots, big old videos and big forgotten files, detected from
//! the scan index with last-modified time as the evidence.
//!
//! The directory-level recommendation engine cannot express these: they
//! are about individual files and how long they have sat untouched. A
//! card is advisory only — detection never removes anything. Files picked
//! from a card go through the cleanup planner like everything else, and
//! the plan-time safety gate (plus its re-run at execution) is the
//! enforcement point; see [`crate::cleanup::Planner::build_plan_for_candidates`].
//!
//! Detection semantics (ages are `now_ms - modified_ms`, evaluated
//! against the `now_ms` passed in — the engine state passes the moment
//! the scan was stored):
//!
//! - **old-installers** — extension `msi`, `msix`, `msixbundle` or
//!   `appinstaller`, or an `exe` whose lowercase name contains `setup`
//!   or `install`; older than 30 days.
//! - **forgotten-downloads** — any directory component of the path is
//!   named `Downloads` (case-insensitive); older than 90 days.
//! - **old-screenshots** — the screenshot kind rule from
//!   [`crate::search`]: the name contains `screenshot`
//!   (case-insensitive) or any path component equals `Screenshots`;
//!   image extensions only (`png`, `jpg`, `jpeg`, `webp`, `bmp`,
//!   `gif`); older than 90 days.
//! - **big-old-videos** — video extensions (`mp4`, `mkv`, `mov`,
//!   `avi`, `wmv`, `webm`, `m4v`); at least 100 MB (decimal,
//!   100,000,000 bytes, the same convention as the search grammar);
//!   older than 180 days.
//! - **big-forgotten-files** — any extension or kind; at least
//!   500 MB (decimal, 500,000,000 bytes); older than 180 days. The
//!   catch-all for the giant disk image or archive nothing has
//!   touched in half a year, which is why it runs last: a big old
//!   video is a big old video, not just a big file.
//!
//! A file belongs to at most one card: the rules are evaluated in the
//! order above and the first match wins, so an old installer sitting in
//! Downloads appears under Old installers only. Zero-byte files are
//! skipped (the scanner also reports hard-link duplicates as zero
//! counted bytes, so this avoids double-counting them), as are files
//! with no recorded mtime (`modified_ms <= 0` — without age evidence a
//! file is never called old). Files the classifier marked
//! [`Category::SystemFiles`], and report-only locations
//! ([`crate::safety::is_report_only`], e.g. `Windows.old`), are skipped
//! conservatively; everything else is left for the plan-time safety
//! gate to judge.

use crate::model::{Category, FileEntry};
use crate::scanner::DirIndex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::Path;

const DAY_MS: i64 = 86_400_000;

const INSTALLER_AGE_MS: i64 = 30 * DAY_MS;
const DOWNLOADS_AGE_MS: i64 = 90 * DAY_MS;
const SCREENSHOT_AGE_MS: i64 = 90 * DAY_MS;
const VIDEO_AGE_MS: i64 = 180 * DAY_MS;
const VIDEO_MIN_BYTES: u64 = 100_000_000;
const BIG_FORGOTTEN_AGE_MS: i64 = 180 * DAY_MS;
const BIG_FORGOTTEN_MIN_BYTES: u64 = 500_000_000;

/// One file offered by a Clean Up card.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CandidateFile {
    /// Stable within a scan: first 16 hex chars of the SHA-256 of the
    /// path. The planner resolves plans against these ids only.
    pub id: String,
    pub path: String,
    pub name: String,
    pub bytes: u64,
    pub modified_ms: i64,
}

/// All files for one card, in card order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CandidateGroup {
    /// Card id: `old-installers`, `forgotten-downloads`,
    /// `old-screenshots`, `big-old-videos` or `big-forgotten-files`.
    pub card: String,
    pub title: String,
    pub files: Vec<CandidateFile>,
    pub total_bytes: u64,
}

/// The five cards, in display order: (id, title).
const CARDS: [(&str, &str); 5] = [
    ("old-installers", "Old installers"),
    ("forgotten-downloads", "Forgotten downloads"),
    ("old-screenshots", "Old screenshots"),
    ("big-old-videos", "Big old videos"),
    ("big-forgotten-files", "Big files, long forgotten"),
];

fn candidate_id(path: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(path.as_bytes());
    let digest = hasher.finalize();
    digest[..8].iter().map(|b| format!("{b:02x}")).collect()
}

/// Lowercase extension without the dot ("" when the name has none).
fn extension_of(lower_name: &str) -> &str {
    lower_name.rsplit_once('.').map(|(_, e)| e).unwrap_or("")
}

/// Path segments split on either separator, matching how the search
/// module reads paths (index paths use the host separator, but the
/// rules below are separator-agnostic).
fn segments(path: &str) -> Vec<&str> {
    path.split(['/', '\\']).filter(|s| !s.is_empty()).collect()
}

fn is_installer(lower_name: &str, ext: &str) -> bool {
    matches!(ext, "msi" | "msix" | "msixbundle" | "appinstaller")
        || (ext == "exe" && (lower_name.contains("setup") || lower_name.contains("install")))
}

fn in_downloads(segs: &[&str]) -> bool {
    // Directory components only: every segment except the file name.
    segs[..segs.len().saturating_sub(1)]
        .iter()
        .any(|s| s.eq_ignore_ascii_case("downloads"))
}

fn is_screenshot(lower_name: &str, segs: &[&str]) -> bool {
    lower_name.contains("screenshot") || segs.iter().any(|s| s.eq_ignore_ascii_case("screenshots"))
}

/// Which card (index into [`CARDS`]) a file belongs to, if any. Rules
/// run in card order; the first match wins.
fn classify(file: &FileEntry, now_ms: i64) -> Option<usize> {
    if file.bytes == 0 || file.modified_ms <= 0 {
        return None;
    }
    if file.category == Category::SystemFiles
        || crate::safety::is_report_only(Path::new(&file.path))
    {
        return None;
    }
    let age_ms = now_ms - file.modified_ms;
    let lower_name = file.name.to_lowercase();
    let ext = extension_of(&lower_name);
    let segs = segments(&file.path);

    if age_ms > INSTALLER_AGE_MS && is_installer(&lower_name, ext) {
        return Some(0);
    }
    if age_ms > DOWNLOADS_AGE_MS && in_downloads(&segs) {
        return Some(1);
    }
    if age_ms > SCREENSHOT_AGE_MS
        && matches!(ext, "png" | "jpg" | "jpeg" | "webp" | "bmp" | "gif")
        && is_screenshot(&lower_name, &segs)
    {
        return Some(2);
    }
    if age_ms > VIDEO_AGE_MS
        && file.bytes >= VIDEO_MIN_BYTES
        && matches!(ext, "mp4" | "mkv" | "mov" | "avi" | "wmv" | "webm" | "m4v")
    {
        return Some(3);
    }
    if age_ms > BIG_FORGOTTEN_AGE_MS && file.bytes >= BIG_FORGOTTEN_MIN_BYTES {
        return Some(4);
    }
    None
}

/// Detect the Clean Up card candidates in a scan index. Only non-empty
/// groups are returned, in the fixed card order; each group's files are
/// sorted largest first (path order breaks ties, deterministically).
pub fn cleanup_cards(index: &DirIndex, now_ms: i64) -> Vec<CandidateGroup> {
    let mut buckets: [Vec<CandidateFile>; 5] = Default::default();
    for file in &index.files {
        if let Some(card) = classify(file, now_ms) {
            buckets[card].push(CandidateFile {
                id: candidate_id(&file.path),
                path: file.path.clone(),
                name: file.name.clone(),
                bytes: file.bytes,
                modified_ms: file.modified_ms,
            });
        }
    }
    CARDS
        .iter()
        .enumerate()
        .filter_map(|(i, (card, title))| {
            let mut files = std::mem::take(&mut buckets[i]);
            if files.is_empty() {
                return None;
            }
            files.sort_by(|a, b| b.bytes.cmp(&a.bytes).then(a.path.cmp(&b.path)));
            let total_bytes = files.iter().map(|f| f.bytes).sum();
            Some(CandidateGroup {
                card: card.to_string(),
                title: title.to_string(),
                files,
                total_bytes,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_700_000_000_000;

    fn entry(name: &str, bytes: u64, age_days: i64, category: Category) -> FileEntry {
        FileEntry {
            path: format!("/root/{name}"),
            name: name.to_string(),
            bytes,
            category,
            modified_ms: NOW - age_days * DAY_MS,
            ocr_text: None,
        }
    }

    fn index(files: Vec<FileEntry>) -> DirIndex {
        DirIndex {
            files,
            ..Default::default()
        }
    }

    fn group<'a>(groups: &'a [CandidateGroup], card: &str) -> Option<&'a CandidateGroup> {
        groups.iter().find(|g| g.card == card)
    }

    #[test]
    fn big_forgotten_files_catches_any_kind() {
        let groups = cleanup_cards(
            &index(vec![entry(
                "backup-2023.iso",
                600_000_000,
                200,
                Category::UserData,
            )]),
            NOW,
        );
        let big = group(&groups, "big-forgotten-files").expect("iso lands in card five");
        assert_eq!(big.title, "Big files, long forgotten");
        assert_eq!(big.files.len(), 1);
        assert_eq!(big.files[0].name, "backup-2023.iso");
        assert_eq!(big.total_bytes, 600_000_000);
    }

    #[test]
    fn big_old_video_stays_a_video_not_a_big_file() {
        let groups = cleanup_cards(
            &index(vec![entry(
                "holiday.mp4",
                600_000_000,
                200,
                Category::UserData,
            )]),
            NOW,
        );
        assert!(group(&groups, "big-old-videos").is_some());
        assert!(group(&groups, "big-forgotten-files").is_none());
    }

    #[test]
    fn big_but_recent_or_small_or_system_files_land_nowhere() {
        let groups = cleanup_cards(
            &index(vec![
                // Big but touched ten days ago.
                entry("fresh.iso", 600_000_000, 10, Category::UserData),
                // Old but under the 500 MB floor.
                entry("small-old.iso", 499_999_999, 200, Category::UserData),
                // Big and old but a system file.
                entry("system.iso", 600_000_000, 200, Category::SystemFiles),
                // Big and old but zero recorded mtime: no age evidence.
                FileEntry {
                    modified_ms: 0,
                    ..entry("ageless.iso", 600_000_000, 200, Category::UserData)
                },
            ]),
            NOW,
        );
        assert!(groups.is_empty(), "no card fires, got: {groups:?}");
    }
}

