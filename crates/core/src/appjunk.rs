//! Per-app junk: a curated rule table naming the cache locations of
//! well-known applications whose *contents are regenerable caches*
//! — the knowledge that makes dedicated cleaners useful, held to
//! this crate's conservative standard:
//!
//! - Only cache directories and cache files are named. Profiles,
//!   browsing history, cookies, saved state, offline/downloaded
//!   content, and user documents are NEVER a rule location.
//! - Every location is a template over the roots the shell supplies
//!   from the environment; a location that does not exist is
//!   skipped, so a rule for an app the user does not have costs
//!   nothing.
//! - Measurement never follows symlinks, is depth-bounded, and uses
//!   the crate's allocated-bytes rule, so figures agree with every
//!   other surface. Nothing here deletes anything: measured groups
//!   feed the ordinary recommendation/planner pipeline, where the
//!   safety gate validates each path again at action time.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::scanner::allocated_size_for_path;

/// The root directories rule templates resolve against. The shell
/// fills these from the process environment (LOCALAPPDATA,
/// APPDATA, ProgramData, SystemRoot, USERPROFILE).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JunkRoots {
    pub local_app_data: PathBuf,
    pub roaming_app_data: PathBuf,
    pub program_data: PathBuf,
    pub windows_dir: PathBuf,
    pub user_profile: PathBuf,
}

/// One application's junk rule.
///
/// `locations` are templates over the tokens `{LOCAL}`, `{ROAMING}`,
/// `{PROGRAMDATA}`, `{WINDOWS}`, `{PROFILE}` (the matching
/// [`JunkRoots`] fields), written with `/` separators. A location
/// may be:
///
/// - a directory — the directory itself is one junk path;
/// - a path with a `*` as a whole segment — the star matches exactly
///   one directory level (e.g. a Firefox profile name);
/// - a path whose final segment is a filename pattern containing
///   `*` (e.g. `thumbcache_*.db`) — each matching file is a junk
///   path;
/// - a single file — the file is one junk path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JunkRule {
    pub id: &'static str,
    pub app: &'static str,
    pub label: &'static str,
    pub note: &'static str,
    pub locations: &'static [&'static str],
}

/// The rule table. Every location below is a cache its owner
/// regenerates on demand; the per-rule comment records why.
pub fn junk_rules() -> &'static [JunkRule] {
    &[
        // Discord's Electron caches live under the roaming userData
        // dir (and are mirrored in some installs under Local). Cache,
        // Code Cache and GPUCache only — never the profile itself.
        JunkRule {
            id: "discord",
            app: "Discord",
            label: "Discord caches",
            note: "Discord rebuilds these caches on launch; chats and settings are not touched.",
            locations: &[
                "{ROAMING}/Discord/Cache",
                "{ROAMING}/Discord/Code Cache",
                "{ROAMING}/Discord/GPUCache",
                "{LOCAL}/Discord/Cache",
                "{LOCAL}/Discord/Code Cache",
                "{LOCAL}/Discord/GPUCache",
            ],
        },
        // Slack's Electron caches; workspace data lives elsewhere in
        // the profile and is not named here.
        JunkRule {
            id: "slack",
            app: "Slack",
            label: "Slack caches",
            note: "Slack rebuilds these caches on launch; sign-ins and workspaces are not touched.",
            locations: &[
                "{ROAMING}/Slack/Cache",
                "{ROAMING}/Slack/Code Cache",
                "{ROAMING}/Slack/GPUCache",
            ],
        },
        // Classic Teams Electron caches. (The new MSIX Teams keeps
        // its data in a packaged location and is deliberately not
        // named.)
        JunkRule {
            id: "teams",
            app: "Microsoft Teams",
            label: "Microsoft Teams caches",
            note: "Teams rebuilds these caches on launch; chats and files are not touched.",
            locations: &[
                "{ROAMING}/Microsoft/Teams/Cache",
                "{ROAMING}/Microsoft/Teams/Code Cache",
                "{ROAMING}/Microsoft/Teams/GPUCache",
            ],
        },
        // Chrome's per-profile HTTP, code and GPU caches for the
        // Default profile. History, cookies and bookmarks live in
        // separate files that are not named.
        JunkRule {
            id: "chrome",
            app: "Google Chrome",
            label: "Chrome caches",
            note: "Chrome rebuilds these caches as you browse; history, cookies and bookmarks are not touched.",
            locations: &[
                "{LOCAL}/Google/Chrome/User Data/Default/Cache",
                "{LOCAL}/Google/Chrome/User Data/Default/Code Cache",
                "{LOCAL}/Google/Chrome/User Data/Default/GPUCache",
            ],
        },
        // Edge is Chromium-based with the same regenerable layout.
        JunkRule {
            id: "edge",
            app: "Microsoft Edge",
            label: "Edge caches",
            note: "Edge rebuilds these caches as you browse; history, cookies and favorites are not touched.",
            locations: &[
                "{LOCAL}/Microsoft/Edge/User Data/Default/Cache",
                "{LOCAL}/Microsoft/Edge/User Data/Default/Code Cache",
                "{LOCAL}/Microsoft/Edge/User Data/Default/GPUCache",
            ],
        },
        // Firefox's disk cache (cache2) inside each profile in the
        // *local* profile tree — the roaming profile (bookmarks,
        // history, logins) is never named.
        JunkRule {
            id: "firefox",
            app: "Firefox",
            label: "Firefox caches",
            note: "Firefox rebuilds its disk cache as you browse; bookmarks, history and logins live in a different profile tree and are not touched.",
            locations: &["{LOCAL}/Mozilla/Firefox/Profiles/*/cache2"],
        },
        // VS Code's Electron caches plus its diagnostic logs (both
        // regenerated); settings, extensions and workspace state are
        // not named.
        JunkRule {
            id: "vscode",
            app: "Visual Studio Code",
            label: "VS Code caches and logs",
            note: "VS Code rebuilds these caches and logs on launch; settings, extensions and your files are not touched.",
            locations: &[
                "{ROAMING}/Code/Cache",
                "{ROAMING}/Code/CachedData",
                "{ROAMING}/Code/Code Cache",
                "{ROAMING}/Code/GPUCache",
                "{ROAMING}/Code/logs",
            ],
        },
        // NVIDIA shader caches (recompiled on demand) and the
        // downloaded driver-installer archive (the installed driver
        // does not live there).
        JunkRule {
            id: "nvidia",
            app: "NVIDIA",
            label: "NVIDIA shader and installer caches",
            note: "Shaders recompile on demand and the driver installer archive is only needed while installing; the installed driver is not touched.",
            locations: &[
                "{LOCAL}/NVIDIA/DXCache",
                "{LOCAL}/NVIDIA/GLCache",
                "{PROGRAMDATA}/NVIDIA Corporation/Downloader",
            ],
        },
        // The shared Direct3D shader cache; every app recompiles
        // what it needs.
        JunkRule {
            id: "d3d-shader-cache",
            app: "DirectX",
            label: "DirectX shader cache",
            note: "Games and apps recompile their shaders on next launch; nothing else lives here.",
            locations: &["{LOCAL}/D3DSCache"],
        },
        // Windows Update's download staging. Installed updates are
        // stored elsewhere; Windows re-downloads anything it still
        // needs.
        JunkRule {
            id: "windows-update",
            app: "Windows Update",
            label: "Windows Update downloads",
            note: "Downloaded update packages waiting to install; Windows re-downloads anything it still needs. Installed updates are not touched.",
            locations: &["{WINDOWS}/SoftwareDistribution/Download"],
        },
        // Explorer's thumbnail and icon cache databases; both are
        // rebuilt automatically as folders are browsed.
        JunkRule {
            id: "explorer-thumbs",
            app: "Windows Explorer",
            label: "Thumbnail and icon caches",
            note: "Explorer rebuilds thumbnail and icon caches automatically as you browse folders.",
            locations: &[
                "{LOCAL}/Microsoft/Windows/Explorer/thumbcache_*.db",
                "{LOCAL}/Microsoft/Windows/Explorer/iconcache_*.db",
            ],
        },
    ]
}

/// One rule's measured junk on this machine.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppJunkGroup {
    pub id: String,
    pub app: String,
    pub label: String,
    pub note: String,
    /// The concrete junk paths that exist (cache directories and
    /// individual cache files), sorted.
    pub paths: Vec<PathBuf>,
    /// Total allocated bytes across `paths`.
    pub bytes: u64,
    /// Total files across `paths` (a file path counts once).
    pub file_count: u64,
    /// True when any of the group's paths hit the walk depth or
    /// entry caps, so `bytes`/`file_count` are a lower bound rather
    /// than the full size of the location.
    pub truncated: bool,
}

/// Deepest directory level a junk measurement descends.
const MAX_WALK_DEPTH: usize = 32;
/// Hard cap on entries visited per measured path, so a
/// pathological tree can never turn a measurement into a scan.
/// Hitting either cap marks the group `truncated`.
const MAX_ENTRIES_PER_PATH: u64 = 500_000;

/// Resolve a rule location template to the concrete paths that
/// exist. Missing branches die silently at every step.
fn resolve_location(template: &str, roots: &JunkRoots) -> Vec<PathBuf> {
    let substituted = template
        .replace("{LOCAL}", &roots.local_app_data.to_string_lossy())
        .replace("{ROAMING}", &roots.roaming_app_data.to_string_lossy())
        .replace("{PROGRAMDATA}", &roots.program_data.to_string_lossy())
        .replace("{WINDOWS}", &roots.windows_dir.to_string_lossy())
        .replace("{PROFILE}", &roots.user_profile.to_string_lossy())
        // Roots arrive in OS spelling (backslashes on Windows);
        // templates are written with '/', so normalize before
        // splitting into segments.
        .replace('\\', "/");
    // Splitting the substituted string is safe: wildcards only ever
    // appear after a token, and root paths supplied by the shell
    // never contain a bare "*" segment.
    let segments: Vec<&str> = substituted.split('/').filter(|s| !s.is_empty()).collect();
    let mut current: Vec<PathBuf> = vec![PathBuf::from("/")];
    // Preserve a Windows drive anchor ("C:") when present.
    if let Some(first) = segments.first() {
        if first.ends_with(':') {
            current = vec![PathBuf::from(format!("{first}/"))];
        }
    }
    let start = if segments.first().is_some_and(|s| s.ends_with(':')) {
        1
    } else {
        0
    };
    let last = segments.len().saturating_sub(1);
    for (i, seg) in segments.iter().enumerate().skip(start) {
        let mut next: Vec<PathBuf> = Vec::new();
        if *seg == "*" {
            // Single directory level: every real subdirectory.
            for base in &current {
                if let Ok(rd) = std::fs::read_dir(base) {
                    for entry in rd.flatten() {
                        let ft = match entry.file_type() {
                            Ok(ft) => ft,
                            Err(_) => continue,
                        };
                        if ft.is_dir() && !ft.is_symlink() {
                            next.push(entry.path());
                        }
                    }
                }
            }
        } else if seg.contains('*') {
            if i != last {
                // A filename pattern is only supported as the final
                // segment; anything else is a template we do not
                // resolve (conservative: skip the location).
                return Vec::new();
            }
            for base in &current {
                if let Ok(rd) = std::fs::read_dir(base) {
                    for entry in rd.flatten() {
                        let ft = match entry.file_type() {
                            Ok(ft) => ft,
                            Err(_) => continue,
                        };
                        if ft.is_file() && !ft.is_symlink() {
                            let name = entry.file_name().to_string_lossy().to_lowercase();
                            if wildcard_match(&seg.to_lowercase(), &name) {
                                next.push(entry.path());
                            }
                        }
                    }
                }
            }
        } else {
            for base in &current {
                next.push(base.join(seg));
            }
        }
        current = next;
        if current.is_empty() {
            break;
        }
    }
    // Keep only paths that exist and are not links: a symlinked
    // cache location is never listed (measurement would refuse it
    // anyway, and no downstream surface should be handed a link).
    current
        .into_iter()
        .filter(|p| {
            std::fs::symlink_metadata(p)
                .map(|md| !md.file_type().is_symlink())
                .unwrap_or(false)
        })
        .collect()
}

/// Minimal `*` glob matcher over already-lowercased strings: `*`
/// matches any run of characters (including none); all other
/// characters match literally.
fn wildcard_match(pattern: &str, name: &str) -> bool {
    let pat: Vec<char> = pattern.chars().collect();
    let txt: Vec<char> = name.chars().collect();
    // Classic two-pointer glob with star backtracking.
    let (mut p, mut t) = (0usize, 0usize);
    let (mut star_p, mut star_t) = (usize::MAX, 0usize);
    while t < txt.len() {
        if p < pat.len() && pat[p] == txt[t] {
            p += 1;
            t += 1;
        } else if p < pat.len() && pat[p] == '*' {
            star_p = p;
            star_t = t;
            p += 1;
        } else if star_p != usize::MAX {
            p = star_p + 1;
            star_t += 1;
            t = star_t;
        } else {
            return false;
        }
    }
    while p < pat.len() && pat[p] == '*' {
        p += 1;
    }
    p == pat.len()
}

/// Measure one junk path: (allocated bytes, file count, whether
/// the walk hit a cap and the figures are therefore a lower
/// bound). Files measure themselves; directories are walked
/// bounded, never following symlinks.
fn measure_path(path: &Path) -> (u64, u64, bool) {
    let Ok(md) = std::fs::symlink_metadata(path) else {
        return (0, 0, false);
    };
    if md.file_type().is_symlink() {
        return (0, 0, false);
    }
    if md.is_file() {
        return (allocated_size_for_path(path), 1, false);
    }
    if !md.is_dir() {
        return (0, 0, false);
    }
    let mut bytes = 0u64;
    let mut files = 0u64;
    let mut visited = 0u64;
    let mut truncated = false;
    let mut stack: Vec<(PathBuf, usize)> = vec![(path.to_path_buf(), 0)];
    while let Some((dir, depth)) = stack.pop() {
        if depth > MAX_WALK_DEPTH {
            truncated = true;
            continue;
        }
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in rd.flatten() {
            visited += 1;
            if visited > MAX_ENTRIES_PER_PATH {
                return (bytes, files, true);
            }
            let ft = match entry.file_type() {
                Ok(ft) => ft,
                Err(_) => continue,
            };
            if ft.is_symlink() {
                continue;
            }
            let p = entry.path();
            if ft.is_dir() {
                stack.push((p, depth + 1));
            } else if ft.is_file() {
                bytes = bytes.saturating_add(allocated_size_for_path(&p));
                files += 1;
            }
        }
    }
    (bytes, files, truncated)
}

/// Component-normalized, lowercased key for ancestor comparisons:
/// components joined by `/`, case folded, so claims spelled in
/// different cases still recognize each other on a
/// case-insensitive filesystem.
fn claim_key(path: &Path) -> String {
    let mut key = String::new();
    for comp in path.components() {
        if !key.is_empty() {
            key.push('/');
        }
        key.push_str(&comp.as_os_str().to_string_lossy().to_lowercase());
    }
    key
}

/// Drop claimed paths that another claim already covers. A
/// directory claim's measurement includes everything under it, so
/// a second claim on a descendant (or an exact duplicate claimed
/// by another group) would count those bytes twice; the
/// descendant claim is dropped and the bytes stay counted exactly
/// once, in the ancestor's group. Claims are processed in table
/// order; for exact duplicates the first group keeps the path.
fn collapse_claims(claims: &mut [BTreeSet<PathBuf>]) {
    // Flatten in group order with each claim's key.
    let mut flat: Vec<(usize, PathBuf, String)> = Vec::new();
    for (gi, set) in claims.iter().enumerate() {
        for p in set {
            flat.push((gi, p.clone(), claim_key(p)));
        }
    }
    let mut kept = vec![true; flat.len()];
    for i in 0..flat.len() {
        for j in 0..flat.len() {
            if i == j || !kept[j] {
                continue;
            }
            let (ki, kj) = (&flat[i].2, &flat[j].2);
            // Strict descendant of another claim, or an exact
            // duplicate of an earlier one.
            if ki.starts_with(&format!("{kj}/")) || (ki == kj && j < i) {
                kept[i] = false;
                break;
            }
        }
    }
    for set in claims.iter_mut() {
        set.clear();
    }
    for (entry, keep) in flat.into_iter().zip(kept) {
        if keep {
            claims[entry.0].insert(entry.1);
        }
    }
}

/// Measure every rule against `roots`. Claims are resolved for
/// all rules first and ancestor-collapsed (see [`collapse_claims`])
/// so nested claims are never double-counted. Groups left with
/// no paths or zero measured bytes are dropped. The result is
/// sorted by bytes descending (ties by rule id) and each group's
/// paths are sorted and deduplicated, so two runs over the same
/// tree return identical results.
pub fn measure_app_junk(roots: &JunkRoots) -> Vec<AppJunkGroup> {
    let mut claims: Vec<BTreeSet<PathBuf>> = junk_rules()
        .iter()
        .map(|rule| {
            let mut paths: BTreeSet<PathBuf> = BTreeSet::new();
            for loc in rule.locations {
                for p in resolve_location(loc, roots) {
                    paths.insert(p);
                }
            }
            paths
        })
        .collect();
    collapse_claims(&mut claims);
    let mut groups = Vec::new();
    for (rule, paths) in junk_rules().iter().zip(claims) {
        let mut bytes = 0u64;
        let mut file_count = 0u64;
        let mut truncated = false;
        for p in &paths {
            let (b, f, t) = measure_path(p);
            bytes = bytes.saturating_add(b);
            file_count = file_count.saturating_add(f);
            truncated |= t;
        }
        if bytes == 0 || paths.is_empty() {
            continue;
        }
        groups.push(AppJunkGroup {
            id: rule.id.to_string(),
            app: rule.app.to_string(),
            label: rule.label.to_string(),
            note: rule.note.to_string(),
            paths: paths.into_iter().collect(),
            bytes,
            file_count,
            truncated,
        });
    }
    groups.sort_by(|a, b| b.bytes.cmp(&a.bytes).then_with(|| a.id.cmp(&b.id)));
    groups
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn write_file(path: &Path, bytes: &[u8]) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
    }

    fn fixture() -> (tempfile::TempDir, JunkRoots) {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();
        let roots = JunkRoots {
            local_app_data: base.join("Local"),
            roaming_app_data: base.join("Roaming"),
            program_data: base.join("ProgramData"),
            windows_dir: base.join("Windows"),
            user_profile: base.join("User"),
        };
        // Discord (roaming): two cache locations with content.
        write_file(
            &roots.roaming_app_data.join("Discord/Cache/data.bin"),
            &[1u8; 4096],
        );
        write_file(
            &roots.roaming_app_data.join("Discord/GPUCache/g.bin"),
            &[2u8; 2048],
        );
        // A Discord non-cache file must never be picked up: it is
        // not under any rule location.
        write_file(
            &roots.roaming_app_data.join("Discord/settings.json"),
            &[3u8; 512],
        );
        // Firefox: two profiles, only one has cache2.
        write_file(
            &roots
                .local_app_data
                .join("Mozilla/Firefox/Profiles/aaa.default/cache2/entries/e1"),
            &[4u8; 1024],
        );
        fs::create_dir_all(
            roots
                .local_app_data
                .join("Mozilla/Firefox/Profiles/bbb.default/other"),
        )
        .unwrap();
        // Explorer caches: matching and non-matching filenames.
        let explorer = roots.local_app_data.join("Microsoft/Windows/Explorer");
        write_file(&explorer.join("thumbcache_32.db"), &[5u8; 8192]);
        write_file(&explorer.join("iconcache_16.db"), &[6u8; 1024]);
        write_file(&explorer.join("thumbcache.db"), &[7u8; 1024]); // no size suffix: NOT matched
        write_file(&explorer.join("unrelated.txt"), &[8u8; 1024]); // NOT matched
                                                                   // An app with an existing but empty cache dir: zero-byte
                                                                   // group must be dropped.
        fs::create_dir_all(roots.local_app_data.join("D3DSCache")).unwrap();
        (dir, roots)
    }

    fn group<'a>(groups: &'a [AppJunkGroup], id: &str) -> Option<&'a AppJunkGroup> {
        groups.iter().find(|g| g.id == id)
    }

    #[test]
    fn groups_rules_and_measures_allocated_bytes() {
        let (_dir, roots) = fixture();
        let groups = measure_app_junk(&roots);
        let discord = group(&groups, "discord").expect("discord group");
        // Paths are exactly the two existing cache locations.
        assert_eq!(discord.paths.len(), 2);
        assert!(discord
            .paths
            .iter()
            .all(|p| p.ends_with("Cache") || p.ends_with("GPUCache")));
        // Bytes equal the crate's own allocated measurement of the
        // fixture files (the allocated-bytes rule), and the profile's
        // settings.json is nowhere in the group.
        let expect =
            allocated_size_for_path(&roots.roaming_app_data.join("Discord/Cache/data.bin"))
                + allocated_size_for_path(&roots.roaming_app_data.join("Discord/GPUCache/g.bin"));
        assert_eq!(discord.bytes, expect);
        assert_eq!(discord.file_count, 2);
        assert!(!discord.paths.iter().any(|p| p.ends_with("settings.json")));
    }

    #[test]
    fn segment_wildcard_resolves_profile_dirs() {
        let (_dir, roots) = fixture();
        let groups = measure_app_junk(&roots);
        let firefox = group(&groups, "firefox").expect("firefox group");
        assert_eq!(firefox.paths.len(), 1);
        assert!(firefox.paths[0].ends_with("aaa.default/cache2"));
        assert_eq!(firefox.file_count, 1);
    }

    #[test]
    fn filename_patterns_match_exactly() {
        let (_dir, roots) = fixture();
        let groups = measure_app_junk(&roots);
        let explorer = group(&groups, "explorer-thumbs").expect("explorer group");
        let names: Vec<String> = explorer
            .paths
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().to_string())
            .collect();
        assert_eq!(names, vec!["iconcache_16.db", "thumbcache_32.db"]);
        assert_eq!(explorer.file_count, 2);
    }

    #[test]
    fn zero_byte_groups_are_dropped_and_output_is_deterministic() {
        let (_dir, roots) = fixture();
        let groups = measure_app_junk(&roots);
        assert!(group(&groups, "d3d-shader-cache").is_none());
        assert!(group(&groups, "slack").is_none()); // no locations exist
        assert_eq!(groups, measure_app_junk(&roots));
        // Sorted by bytes descending.
        assert!(groups.windows(2).all(|w| w[0].bytes >= w[1].bytes));
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_are_never_followed() {
        let (dir, roots) = fixture();
        // A symlink inside a cache dir pointing at a big outside file
        // must contribute nothing.
        let outside = dir.path().join("outside.bin");
        fs::write(&outside, vec![9u8; 65_536]).unwrap();
        let link = roots.roaming_app_data.join("Discord/Cache/link.bin");
        std::os::unix::fs::symlink(&outside, &link).unwrap();
        let groups = measure_app_junk(&roots);
        let discord = group(&groups, "discord").unwrap();
        assert_eq!(discord.file_count, 2);
        // And a symlinked cache *directory* is not descended either.
        let dir_link = roots.roaming_app_data.join("Discord/Code Cache");
        std::os::unix::fs::symlink(dir.path(), &dir_link).unwrap();
        let groups2 = measure_app_junk(&roots);
        assert_eq!(group(&groups2, "discord").unwrap().paths.len(), 2);
    }

    #[test]
    fn ancestor_claims_collapse_so_nested_junk_counts_once() {
        // A descendant claim (even spelled in a different case)
        // drops out of its group: its bytes live in the ancestor's
        // measurement. Unrelated claims survive untouched.
        let mut claims = vec![
            BTreeSet::from([PathBuf::from("/x/Cache")]),
            BTreeSet::from([PathBuf::from("/X/cache/Sub"), PathBuf::from("/y/Other")]),
        ];
        collapse_claims(&mut claims);
        assert_eq!(claims[0], BTreeSet::from([PathBuf::from("/x/Cache")]));
        assert_eq!(claims[1], BTreeSet::from([PathBuf::from("/y/Other")]));
        // Within one group the same collapse applies.
        let mut claims = vec![BTreeSet::from([
            PathBuf::from("/a/base"),
            PathBuf::from("/a/base/inner"),
            PathBuf::from("/a/base2"),
        ])];
        collapse_claims(&mut claims);
        assert_eq!(
            claims[0],
            BTreeSet::from([PathBuf::from("/a/base"), PathBuf::from("/a/base2")])
        );
        // A sibling that merely shares a name prefix is NOT a
        // descendant and must survive.
        let mut claims = vec![
            BTreeSet::from([PathBuf::from("/x/Cache")]),
            BTreeSet::from([PathBuf::from("/x/Cache2")]),
        ];
        collapse_claims(&mut claims);
        assert_eq!(claims[1].len(), 1);
        // An exact duplicate claimed by two groups stays with the
        // first; the second group is left empty (and so drops out
        // of measure_app_junk's output entirely).
        let mut claims = vec![
            BTreeSet::from([PathBuf::from("/same/Dir")]),
            BTreeSet::from([PathBuf::from("/SAME/dir")]),
        ];
        collapse_claims(&mut claims);
        assert_eq!(claims[0].len(), 1);
        assert!(claims[1].is_empty());
    }

    #[test]
    fn deep_trees_set_truncated_and_stop_at_the_cap() {
        let (_dir, roots) = fixture();
        // A 40-level chain inside Discord's Cache, with a file at
        // the very bottom: deeper than the 32-level walk cap.
        let mut deep = roots.roaming_app_data.join("Discord/Cache");
        for i in 0..40 {
            deep = deep.join(format!("d{i}"));
        }
        write_file(&deep.join("bottom.bin"), &[1u8; 128]);
        let groups = measure_app_junk(&roots);
        let discord = group(&groups, "discord").expect("discord group");
        assert!(discord.truncated);
        // The bottom file is beyond the cap and not counted; the
        // count is exactly the fixture's two shallow cache files.
        assert_eq!(discord.file_count, 2);
        // A shallow group measured in the same run is not flagged.
        let explorer = group(&groups, "explorer-thumbs").expect("explorer group");
        assert!(!explorer.truncated);
    }

    #[test]
    fn wildcard_matcher() {
        assert!(wildcard_match("thumbcache_*.db", "thumbcache_32.db"));
        assert!(!wildcard_match("thumbcache_*.db", "thumbcache.db"));
        assert!(wildcard_match("iconcache_*.db", "iconcache_.db"));
        assert!(wildcard_match("a*c", "abbbc"));
        assert!(!wildcard_match("a*c", "abbbd"));
        assert!(wildcard_match("*", "anything"));
        assert!(wildcard_match("", ""));
    }
}
