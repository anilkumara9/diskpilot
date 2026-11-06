//! Drive (volume) enumeration: every mounted volume with its total
//! and free space, for the shell's drive overview.
//!
//! On Windows every drive letter `A:\` through `Z:\` is probed and the
//! letters that answer with a real, non-empty volume are kept (empty
//! card readers and unassigned letters stay silent or report zero and
//! are dropped). On other platforms the single filesystem root `/` is
//! reported, which keeps the feature meaningful in development.

use serde::{Deserialize, Serialize};

/// One mounted volume.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DriveInfo {
    /// Volume root, e.g. `C:\` (or `/` off Windows).
    pub root_path: String,
    pub total_bytes: u64,
    pub free_bytes: u64,
}

/// Keep only probe results that produced a real, non-empty volume.
/// Pure, so the filtering rule is testable without mounted drives.
fn filter_drives(candidates: Vec<(String, Option<(u64, u64)>)>) -> Vec<DriveInfo> {
    candidates
        .into_iter()
        .filter_map(|(root_path, probe)| {
            let (total_bytes, free_bytes) = probe?;
            if total_bytes == 0 {
                return None;
            }
            Some(DriveInfo {
                root_path,
                total_bytes,
                free_bytes,
            })
        })
        .collect()
}

/// Probe one root: `(total, free)` when the volume answers.
fn probe(root: &str) -> Option<(u64, u64)> {
    let total = fs2::total_space(root).ok()?;
    let free = fs2::free_space(root).ok()?;
    Some((total, free))
}

/// Every mounted volume on this machine.
pub fn list_drives() -> Vec<DriveInfo> {
    #[cfg(windows)]
    let roots: Vec<String> = ('A'..='Z').map(|c| format!("{c}:\\")).collect();
    #[cfg(not(windows))]
    let roots: Vec<String> = vec!["/".to_string()];
    let candidates = roots
        .into_iter()
        .map(|root| {
            let probed = probe(&root);
            (root, probed)
        })
        .collect();
    filter_drives(candidates)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filter_drops_unanswered_and_empty_volumes() {
        let drives = filter_drives(vec![
            ("A:\\".to_string(), None),
            ("C:\\".to_string(), Some((500, 200))),
            ("D:\\".to_string(), Some((0, 0))),
            ("E:\\".to_string(), Some((100, 0))),
        ]);
        assert_eq!(
            drives,
            vec![
                DriveInfo {
                    root_path: "C:\\".to_string(),
                    total_bytes: 500,
                    free_bytes: 200,
                },
                DriveInfo {
                    root_path: "E:\\".to_string(),
                    total_bytes: 100,
                    free_bytes: 0,
                },
            ]
        );
    }

    #[test]
    fn filter_handles_no_candidates() {
        assert!(filter_drives(Vec::new()).is_empty());
    }

    #[test]
    fn list_drives_never_panics_and_reports_this_platform() {
        let drives = list_drives();
        #[cfg(not(windows))]
        {
            assert_eq!(drives.len(), 1);
            assert_eq!(drives[0].root_path, "/");
            assert!(drives[0].total_bytes > 0);
        }
        #[cfg(windows)]
        {
            assert!(drives
                .iter()
                .all(|d| d.root_path.len() == 3 && d.total_bytes > 0));
        }
    }
}
