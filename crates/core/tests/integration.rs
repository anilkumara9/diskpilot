use diskpilot_core::cleanup::{execute_plan, ExecuteMode, FileOps, Planner};
use std::cell::RefCell;

#[derive(Default)]
struct MockFileOps {
    trashed: RefCell<Vec<String>>,
    deleted: RefCell<Vec<String>>,
}
impl FileOps for MockFileOps {
    fn move_to_trash(&self, path: &Path) -> Result<(), String> {
        self.trashed
            .borrow_mut()
            .push(path.to_string_lossy().to_string());
        Ok(())
    }
    fn permanent_delete(&self, path: &Path) -> Result<(), String> {
        self.deleted
            .borrow_mut()
            .push(path.to_string_lossy().to_string());
        Ok(())
    }
}
use diskpilot_core::classifier::{self, rules::RuleScope};
use diskpilot_core::docker::parse::{parse_system_df_v, SAMPLE_DF_V};
use diskpilot_core::model::*;
use diskpilot_core::recommend;
use diskpilot_core::safety::{self, SafetyContext};
use diskpilot_core::scanner::Scanner;
use diskpilot_core::secrets;
use std::io::Write;
use std::path::{Path, PathBuf};

fn write_file(path: &Path, bytes: usize) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    let mut f = std::fs::File::create(path).unwrap();
    f.write_all(&vec![b'x'; bytes]).unwrap();
}

fn ctx(root: &Path) -> SafetyContext {
    SafetyContext::new(root).with_home(root)
}

#[test]
fn scanner_totals_equal_category_totals() {
    let dir = tempfile::tempdir().unwrap();
    write_file(&dir.path().join("proj/node_modules/pkg/a.js"), 5000);
    write_file(&dir.path().join("proj/src/main.rs"), 3000);
    write_file(&dir.path().join("docs/readme.txt"), 2000);
    let (summary, _index) = Scanner::new(dir.path()).scan();
    let cat_sum: u64 = summary.category_totals.values().sum();
    assert_eq!(
        cat_sum, summary.total_bytes,
        "category totals must reconcile"
    );
    assert_eq!(summary.file_count, 3);
    assert!(summary.total_bytes > 0);
    assert!(!summary.top_files.is_empty());
}

/// The MFT fast path's virtual walk: flat entries in, the same
/// index and summary shape the walker produces out — totals,
/// tree sizes, counts, skipped list, and the engine tag.
#[test]
fn mft_entry_build_matches_walker_shape() {
    use diskpilot_core::mft::MftEntry;
    let dir = tempfile::tempdir().unwrap();
    let root_str = dir.path().to_string_lossy().to_string();
    let entry = |rel: &str, is_dir: bool, allocated: u64| {
        let name = rel.rsplit('\\').next().unwrap_or(rel).to_string();
        MftEntry {
            path: format!("{root_str}\\{rel}"),
            name,
            is_dir,
            logical_bytes: allocated,
            allocated_bytes: allocated,
            modified_ms: 0,
        }
    };
    let entries = vec![
        entry("docs", true, 0),
        entry("docs\\a.txt", false, 1024),
        entry("b.bin", false, 4096),
    ];
    let skipped = vec![format!("{root_str}\\linkdir")];
    let (summary, index) = Scanner::new(dir.path()).build_from_entries(entries, skipped);
    assert_eq!(summary.scan_engine.as_deref(), Some("mft"));
    assert_eq!(summary.total_bytes, 5120);
    assert_eq!(summary.file_count, 2);
    assert_eq!(summary.dir_count, 1);
    let cat_sum: u64 = summary.category_totals.values().sum();
    assert_eq!(cat_sum, summary.total_bytes);
    assert_eq!(summary.skipped.len(), 1);
    assert_eq!(index.files.len(), 2);
    assert_eq!(index.dir_sizes.get(&root_str), Some(&5120));
    assert_eq!(
        index.dir_sizes.get(&format!("{root_str}\\docs")),
        Some(&1024)
    );
    let root_children = index.children.get(&root_str).unwrap();
    assert_eq!(root_children.len(), 2);
    assert!(root_children.iter().any(|c| c.is_dir && c.bytes == 1024));
}

/// scan_auto on an ordinary folder (never a volume root) falls
/// back to the walker and says so.
#[test]
fn scan_auto_on_folder_uses_walker() {
    let dir = tempfile::tempdir().unwrap();
    write_file(&dir.path().join("a.txt"), 100);
    let (summary, _index) = Scanner::new(dir.path()).scan_auto();
    assert_eq!(summary.scan_engine.as_deref(), Some("walker"));
    assert_eq!(summary.file_count, 1);
}

#[test]
fn scanner_does_not_follow_symlink() {
    let dir = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    write_file(&outside.path().join("secret.txt"), 7000);
    write_file(&dir.path().join("real/file.txt"), 1000);
    #[cfg(unix)]
    std::os::unix::fs::symlink(outside.path(), dir.path().join("linkdir")).unwrap();
    #[cfg(unix)]
    {
        let (summary, _index) = Scanner::new(dir.path()).scan();
        assert_eq!(summary.file_count, 1, "symlinked dir must not be scanned");
        assert!(summary.skipped.iter().any(|s| s.contains("linkdir")));
    }
}

#[test]
fn exclusion_pattern_matching_rules() {
    use diskpilot_core::scanner::is_excluded;
    let pats = |ps: &[&str]| ps.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    // Exact segment match, any component, case-insensitive.
    assert!(is_excluded(
        Path::new("/x/proj/Node_Modules/pkg/a.js"),
        &pats(&["node_modules"])
    ));
    assert!(!is_excluded(
        Path::new("/x/proj/node_modulesx/a.js"),
        &pats(&["node_modules"])
    ));
    // Glob against a single segment.
    assert!(is_excluded(Path::new("/x/logs/app.log"), &pats(&["*.log"])));
    assert!(!is_excluded(
        Path::new("/x/logs/app.txt"),
        &pats(&["*.log"])
    ));
    assert!(is_excluded(Path::new("/x/gen-a/f"), &pats(&["gen-?"])));
    assert!(!is_excluded(Path::new("/x/gen-ab/f"), &pats(&["gen-?"])));
    // Multi-segment patterns match a trailing sequence of segments, with
    // either separator: they match the directory they name (the scanner
    // prunes the subtree there), not the same leaf under another parent.
    assert!(is_excluded(
        Path::new("/x/proj/node_modules"),
        &pats(&["proj/node_modules"])
    ));
    assert!(is_excluded(
        Path::new("/x/proj/node_modules"),
        &pats(&["proj\\node_modules"])
    ));
    assert!(!is_excluded(
        Path::new("/x/other/node_modules"),
        &pats(&["proj/node_modules"])
    ));
    // Empty patterns / empty list never match.
    assert!(!is_excluded(Path::new("/x/y"), &pats(&[""])));
    assert!(!is_excluded(Path::new("/x/y"), &[]));
}

#[test]
fn scanner_exclusions_hide_directory() {
    let dir = tempfile::tempdir().unwrap();
    write_file(&dir.path().join("proj/node_modules/pkg/a.js"), 5000);
    write_file(&dir.path().join("proj/src/main.rs"), 3000);
    write_file(&dir.path().join("docs/readme.txt"), 2000);
    let (baseline, _) = Scanner::new(dir.path()).scan();
    assert_eq!(baseline.file_count, 3);

    let (summary, index) = Scanner::new(dir.path())
        .with_exclusions(vec!["node_modules".to_string()])
        .scan();
    assert_eq!(
        summary.file_count, 2,
        "excluded dir's files are not counted"
    );
    let dep_bytes = diskpilot_core::scanner::allocated_size_for_path(
        &dir.path().join("proj/node_modules/pkg/a.js"),
    );
    assert_eq!(summary.total_bytes, baseline.total_bytes - dep_bytes);
    // Not in the index, not in totals, not in the skipped list.
    assert!(!index.dir_sizes.keys().any(|p| p.contains("node_modules")));
    assert!(!index.files.iter().any(|f| f.path.contains("node_modules")));
    assert!(!summary.skipped.iter().any(|s| s.contains("node_modules")));
}

#[test]
fn scanner_exclusions_multi_segment_prunes_subtree() {
    let dir = tempfile::tempdir().unwrap();
    write_file(&dir.path().join("proj/node_modules/pkg/a.js"), 5000);
    write_file(&dir.path().join("other/node_modules/pkg/b.js"), 4000);
    let (summary, index) = Scanner::new(dir.path())
        .with_exclusions(vec!["proj/node_modules".to_string()])
        .scan();
    assert_eq!(summary.file_count, 1, "only other/node_modules survives");
    assert!(index.files[0].path.contains("other"));
}

#[test]
fn scanner_exclusions_wildcards() {
    let dir = tempfile::tempdir().unwrap();
    write_file(&dir.path().join("app.log"), 4000);
    write_file(&dir.path().join("keep.txt"), 1000);
    write_file(&dir.path().join("gen-a/out.bin"), 2000);
    write_file(&dir.path().join("src/out.bin"), 3000);
    let (summary, _index) = Scanner::new(dir.path())
        .with_exclusions(vec!["*.log".to_string(), "gen-*".to_string()])
        .scan();
    assert_eq!(
        summary.file_count, 2,
        "only keep.txt and src/out.bin remain"
    );
}

#[test]
fn scanner_exclusions_root_never_excluded_and_empty_noop() {
    let dir = tempfile::tempdir().unwrap();
    write_file(&dir.path().join("sub/file.txt"), 1500);
    let (baseline, _) = Scanner::new(dir.path()).scan();

    // Pattern equal to the root's own final component: the root itself is
    // never excluded, and no child carries that name, so nothing changes.
    let root_name = dir
        .path()
        .file_name()
        .unwrap()
        .to_string_lossy()
        .to_string();
    let (with_root_pattern, _) = Scanner::new(dir.path())
        .with_exclusions(vec![root_name])
        .scan();
    assert_eq!(with_root_pattern.file_count, baseline.file_count);
    assert_eq!(with_root_pattern.total_bytes, baseline.total_bytes);

    // Empty pattern list is a no-op: identical summary.
    let (empty, _) = Scanner::new(dir.path()).with_exclusions(Vec::new()).scan();
    assert_eq!(empty.file_count, baseline.file_count);
    assert_eq!(empty.dir_count, baseline.dir_count);
    assert_eq!(empty.total_bytes, baseline.total_bytes);
    assert_eq!(empty.category_totals, baseline.category_totals);
}

#[test]
fn hardlink_counted_once() {
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("a.bin");
    write_file(&a, 6000);
    let _b = dir.path().join("b.bin");
    #[cfg(unix)]
    {
        std::fs::hard_link(&a, &_b).unwrap();
        let (summary, _index) = Scanner::new(dir.path()).scan();
        assert_eq!(summary.file_count, 2);
        // Two directory entries, one inode: bytes counted once. Logical
        // counted size equals one file's allocated size, well under 2x.
        let one = diskpilot_core::scanner::allocated_size_for_path(&a);
        assert_eq!(summary.total_bytes, one);
        // Duplicates finder also treats them as the same content but the
        // scanner-level dedup is what matters here.
        let _ = _b;
    }
}

#[test]
fn classifier_leaf_and_container() {
    // Leaf: node_modules claims its whole subtree.
    let leaf_path = PathBuf::from("/home/u/proj/node_modules/pkg/deep/file.js");
    let c = classifier::classify_path(&leaf_path);
    assert_eq!(c.category, Category::BuildArtifacts);
    assert_eq!(classifier::scope_of(&c.rule_id), RuleScope::Leaf);

    // Child under a leaf inherits the leaf even with a different name.
    let parent = (c.clone(), RuleScope::Leaf);
    let (child, _) = classifier::classify_child(
        &PathBuf::from("/home/u/proj/node_modules/pkg/cache"),
        true,
        Some(&parent),
    );
    assert_eq!(child.category, Category::BuildArtifacts);

    // Container: AppData allows deeper rules to refine.
    let appdata = classifier::classify_path(&PathBuf::from("/home/u/AppData/Local"));
    assert_eq!(appdata.category, Category::ApplicationData);
    assert_eq!(classifier::scope_of(&appdata.rule_id), RuleScope::Container);
    let (refined, _) = classifier::classify_child(
        &PathBuf::from("/home/u/AppData/Local/npm-cache"),
        true,
        Some(&(appdata, RuleScope::Container)),
    );
    assert_eq!(refined.category, Category::Cache);
}

#[test]
fn safety_rejects_protected_outside_and_symlink() {
    let dir = tempfile::tempdir().unwrap();
    let c = ctx(dir.path());
    // Outside the scan root + home.
    assert!(safety::validate(Path::new("/etc/shadow-copy"), &c).is_err());
    // Traversal.
    assert!(safety::validate(&dir.path().join("x/../../etc"), &c).is_err());
    // The root itself.
    assert!(safety::validate(dir.path(), &c).is_err());
    // Symlink inside the tree pointing inside: the link itself is rejected.
    #[cfg(unix)]
    {
        let target = dir.path().join("realdir");
        std::fs::create_dir_all(&target).unwrap();
        let link = dir.path().join("linkdir");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        assert!(safety::validate(&link.join("file.txt"), &c).is_err());
    }
    // A plain file inside is accepted.
    let ok_file = dir.path().join("cache/item.bin");
    write_file(&ok_file, 100);
    assert!(safety::validate(&ok_file, &c).is_ok());
}

fn sample_recommendation(dir: &Path) -> (Vec<Recommendation>, PathBuf) {
    let cache_dir = dir.join("proj/node_modules");
    write_file(&cache_dir.join("pkg/a.js"), 5000);
    let (summary, index) = Scanner::new(dir).scan();
    let recs = recommend::build_recommendations(&summary, &index);
    (recs, cache_dir)
}

#[test]
fn recommendations_and_planner_single_use_and_expiry() {
    let dir = tempfile::tempdir().unwrap();
    let (recs, _cache_dir) = sample_recommendation(dir.path());
    assert!(
        recs.iter().any(|r| r.paths[0].contains("node_modules")),
        "expected a node_modules recommendation, got: {:?}",
        recs.iter().map(|r| &r.title).collect::<Vec<_>>()
    );
    let rec = recs
        .iter()
        .find(|r| r.paths[0].contains("node_modules"))
        .unwrap()
        .clone();
    let c = ctx(dir.path());
    let mut planner = Planner::new();
    let now = 1_000_000i64;
    let plan = planner.build_plan(std::slice::from_ref(&rec.id), &recs, &c, now);
    assert_eq!(plan.items.len(), 1);
    assert!(plan.allow_permanent, "node_modules is regenerable");
    // Single use.
    assert!(planner.take_plan(&plan.id, now + 1000).is_ok());
    assert!(planner.take_plan(&plan.id, now + 1000).is_err());
    // Expiry on a fresh plan.
    let plan2 = planner.build_plan(std::slice::from_ref(&rec.id), &recs, &c, now);
    assert!(planner
        .take_plan(
            &plan2.id,
            now + diskpilot_core::cleanup::planner::PLAN_TTL_MS + 1
        )
        .is_err());
}

#[test]
fn windows_old_at_root_is_a_guided_only_recommendation() {
    let dir = tempfile::tempdir().unwrap();
    let old = dir.path().join("Windows.old");
    write_file(&old.join("Windows/System32/kernel.dll"), 5000);
    write_file(&old.join("Users/meda/old.txt"), 3000);
    let (summary, index) = Scanner::new(dir.path()).scan();
    let recs = recommend::build_recommendations(&summary, &index);

    let rec = recs
        .iter()
        .find(|r| r.id == "windows-old")
        .expect("a root-level Windows.old must produce the guided recommendation");
    assert_eq!(rec.title, "Previous Windows installation (Windows.old)");
    assert_eq!(rec.tier, Tier::ReviewFirst);
    assert!(rec.guided_only);
    assert_eq!(
        rec.guidance.as_deref(),
        Some("Remove it through Windows Settings → System → Storage → Temporary files → Previous Windows installation, or Disk Cleanup (cleanmgr) → Clean up system files. DiskPilot does not delete this folder directly.")
    );
    assert_eq!(rec.paths, vec![old.to_string_lossy().to_string()]);
    assert_eq!(rec.bytes, index.size_of(&rec.paths[0]));
    assert!(rec.bytes > 0);
    // Exactly one recommendation speaks for the folder: no generic or
    // Keep item may also claim the path.
    assert_eq!(
        recs.iter()
            .filter(|r| r.paths.iter().any(|p| p == &rec.paths[0]))
            .count(),
        1
    );

    // The planner rejects it explicitly: no items, one rejection
    // naming the guidance, and permanent deletion stays off.
    let c = ctx(dir.path());
    let mut planner = Planner::new();
    let plan = planner.build_plan(&["windows-old".to_string()], &recs, &c, 7_000_000);
    assert!(plan.items.is_empty());
    assert!(!plan.allow_permanent);
    assert_eq!(plan.rejected.len(), 1);
    assert!(plan.rejected[0].contains("guided only"));
}

#[test]
fn nested_windows_old_gets_no_guided_recommendation() {
    let dir = tempfile::tempdir().unwrap();
    write_file(
        &dir.path().join("backup/Windows.old/Windows/file.dll"),
        5000,
    );
    let (summary, index) = Scanner::new(dir.path()).scan();
    let recs = recommend::build_recommendations(&summary, &index);
    assert!(
        recs.iter().all(|r| r.id != "windows-old" && !r.guided_only),
        "only a root-level Windows.old is the previous installation"
    );
}

#[test]
fn executor_uses_mock_and_records() {
    let dir = tempfile::tempdir().unwrap();
    let (recs, _cache_dir) = sample_recommendation(dir.path());
    let rec = recs
        .iter()
        .find(|r| r.paths[0].contains("node_modules"))
        .unwrap()
        .clone();
    let c = ctx(dir.path());
    let mut planner = Planner::new();
    let now = 2_000_000i64;
    let plan = planner.build_plan(std::slice::from_ref(&rec.id), &recs, &c, now);
    let taken = planner.take_plan(&plan.id, now + 10).unwrap();
    let ops = MockFileOps::default();
    let result = execute_plan(&taken, ExecuteMode::MoveToTrash, &c, &ops).unwrap();
    assert_eq!(result.succeeded.len(), 1);
    assert_eq!(ops.trashed.borrow().len(), 1);
    assert!(result.reclaimed_bytes > 0);

    // History round-trip.
    let store = diskpilot_core::history::HistoryStore::in_memory().unwrap();
    store
        .record_cleanup(
            "move-to-trash",
            &result,
            None,
            diskpilot_core::history::RemovedItemMode::Trash,
        )
        .unwrap();
    let entries = store.list_history(10).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].paths.len(), 1);
}

#[test]
fn duplicates_found_by_hash() {
    let dir = tempfile::tempdir().unwrap();
    write_file(&dir.path().join("a/copy.txt"), 4000);
    // Same content, different path.
    let content = vec![b'y'; 4000];
    std::fs::create_dir_all(dir.path().join("b")).unwrap();
    std::fs::write(dir.path().join("b/copy.txt"), &content).unwrap();
    std::fs::write(dir.path().join("a/copy.txt"), &content).unwrap();
    write_file(&dir.path().join("unique.txt"), 1234);
    let groups = diskpilot_core::duplicates::find_duplicates(dir.path());
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].files.len(), 2);
    assert_eq!(groups[0].size, 4000);
}

#[test]
fn duplicates_parallel_matches_full_hash_reference() {
    // The parallel staged finder (size -> partial hash -> full hash) must
    // produce exactly the groups an independent serial grouping by full
    // SHA-256 alone produces.
    let dir = tempfile::tempdir().unwrap();
    let pattern = |seed: u8, n: usize| -> Vec<u8> {
        (0..n).map(|i| seed.wrapping_add((i % 251) as u8)).collect()
    };
    // Pair A: identical large files (span the partial-hash head+tail window).
    let big = pattern(7, 200_000);
    std::fs::create_dir_all(dir.path().join("a")).unwrap();
    std::fs::create_dir_all(dir.path().join("b")).unwrap();
    std::fs::write(dir.path().join("a/big.bin"), &big).unwrap();
    std::fs::write(dir.path().join("b/big.bin"), &big).unwrap();
    // Pair B: same head and tail, different middle. Same size and same
    // partial hash, different content — staging must still separate them.
    let mut b1 = pattern(3, 200_000);
    let mut b2 = b1.clone();
    b1[100_000] = 0xAA;
    b2[100_000] = 0xBB;
    assert_ne!(b1, b2);
    std::fs::write(dir.path().join("a/tricky.bin"), &b1).unwrap();
    std::fs::write(dir.path().join("b/tricky.bin"), &b2).unwrap();
    // Pair C: identical small files.
    let small = pattern(9, 4_000);
    std::fs::write(dir.path().join("a/small.bin"), &small).unwrap();
    std::fs::write(dir.path().join("b/small.bin"), &small).unwrap();
    // A unique file and an empty file (ignored: zero-length).
    write_file(&dir.path().join("unique.bin"), 1234);
    std::fs::write(dir.path().join("empty.bin"), b"").unwrap();

    // Independent reference: serial walk, group by (size, full hash) only.
    fn collect(root: &Path, out: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(root).unwrap().flatten() {
            let p = entry.path();
            let md = std::fs::symlink_metadata(&p).unwrap();
            if md.file_type().is_symlink() {
                continue;
            }
            if md.is_dir() {
                collect(&p, out);
            } else if md.is_file() && md.len() > 0 {
                out.push(p);
            }
        }
    }
    let mut files = Vec::new();
    collect(dir.path(), &mut files);
    let mut reference: Vec<(u64, String, Vec<String>)> = Vec::new();
    for f in &files {
        let size = std::fs::metadata(f).unwrap().len();
        let hash = diskpilot_core::duplicates::hash_full(f).unwrap();
        let path = f.to_string_lossy().to_string();
        if let Some(g) = reference.iter_mut().find(|g| g.0 == size && g.1 == hash) {
            g.2.push(path);
        } else {
            reference.push((size, hash, vec![path]));
        }
    }
    reference.retain(|g| g.2.len() >= 2);
    for g in &mut reference {
        g.2.sort();
    }
    reference.sort();

    // Canonical form of the real (parallel, staged) result.
    let mut actual: Vec<(u64, String, Vec<String>)> =
        diskpilot_core::duplicates::find_duplicates(dir.path())
            .into_iter()
            .map(|g| {
                let mut paths: Vec<String> = g.files.iter().map(|f| f.path.clone()).collect();
                paths.sort();
                (g.size, g.hash, paths)
            })
            .collect();
    actual.sort();

    assert_eq!(actual.len(), 2, "expected the big and small groups only");
    assert_eq!(actual, reference);
}

#[test]
fn devtools_all_reports_parallel_preserves_order() {
    use diskpilot_core::devtools::{self, DevPaths};
    let dir = tempfile::tempdir().unwrap();
    // Fixtures that trigger two different analyzers (Rust toolchains and
    // Node), so cross-analyzer ordering is observable.
    write_file(
        &dir.path()
            .join(".rustup/toolchains/stable-x86_64-unknown-linux-gnu/bin/rustc"),
        3000,
    );
    write_file(&dir.path().join("proj/node_modules/pkg/index.js"), 2000);
    let paths = DevPaths::under(dir.path());

    // The serial expectation: each analyzer called directly, in the
    // documented order, empty reports dropped.
    let mut expected: Vec<ToolReport> = Vec::new();
    expected.extend(devtools::editors::analyze_all(&paths));
    expected.extend(devtools::agents::analyze_all(&paths));
    if let Some(r) = devtools::node::analyze(&paths) {
        expected.push(r);
    }
    if let Some(r) = devtools::toolchains::analyze_rust(&paths) {
        expected.push(r);
    }
    if let Some(r) = devtools::toolchains::analyze_python(&paths) {
        expected.push(r);
    }
    if let Some(r) = devtools::dotnet::analyze(&paths) {
        expected.push(r);
    }
    if let Some(r) = devtools::models::analyze(&paths) {
        expected.push(r);
    }
    expected.retain(|r| !r.is_empty());
    assert!(
        expected.len() >= 2,
        "fixture should trigger at least the Node and Rust reports, got {}",
        expected.len()
    );

    let actual = devtools::all_reports(&paths);
    assert_eq!(actual, expected, "parallel reports must match serial order");
}

#[test]
fn docker_parse_fixture() {
    let o = parse_system_df_v(SAMPLE_DF_V);
    assert_eq!(o.images.len(), 2);
    assert_eq!(o.containers.len(), 2);
    assert_eq!(o.volumes.len(), 2);
    assert!(o.build_cache_bytes > 0);
}

#[test]
fn secrets_masked() {
    let text = "token: ghp_abcdefghijklmnopqrstuvwxyz0123456789 and sk-ant-api03-abcdefghijklmnopqrstuvwxyz";
    let found = secrets::scan_text("chat.jsonl", text);
    assert!(found.len() >= 2);
    for f in &found {
        assert!(!f.masked.contains("abcdefghijklmnopqrstuvwxyz"));
    }
}

#[test]
fn snapshot_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    write_file(&dir.path().join("f.txt"), 100);
    let (summary, _i) = Scanner::new(dir.path()).scan();
    let snap = dir.path().join("snap.json");
    diskpilot_core::snapshot::save_snapshot(&summary, &snap).unwrap();
    let loaded = diskpilot_core::snapshot::load_snapshot(&snap)
        .unwrap()
        .unwrap();
    assert_eq!(loaded.total_bytes, summary.total_bytes);
    assert_eq!(loaded.file_count, summary.file_count);
}

#[test]
fn disk_info_nonzero() {
    let info = diskpilot_core::disk::disk_info_for(Path::new("/"));
    assert!(info.total_bytes > 0);
    assert!(info.available_bytes > 0);
}

#[test]
fn assistant_tool_loop_with_scripted_client() {
    use diskpilot_core::assistant::*;
    use serde_json::json;
    struct Scripted {
        step: usize,
    }
    impl AssistantClient for Scripted {
        fn send(
            &mut self,
            _messages: &[ChatMessage],
            _defs: &[ToolDefinition],
        ) -> Result<Vec<ClientEvent>, String> {
            self.step += 1;
            if self.step == 1 {
                Ok(vec![ClientEvent::ToolCall {
                    id: "1".into(),
                    name: "get_category_totals".into(),
                    input: json!({}),
                }])
            } else {
                Ok(vec![
                    ClientEvent::Text("Your biggest category is covered above.".into()),
                    ClientEvent::Done,
                ])
            }
        }
    }
    let dir = tempfile::tempdir().unwrap();
    write_file(&dir.path().join("f.txt"), 100);
    let (summary, index) = Scanner::new(dir.path()).scan();
    let recs: Vec<Recommendation> = Vec::new();
    let ctx = ToolContext {
        summary: Some(&summary),
        index: Some(&index),
        recommendations: &recs,
        docker: None,
        devtools: &[],
        home: dir.path().to_string_lossy().to_string(),
    };
    let mut session = AssistantSession::new(Provider::Anthropic, ctx);
    let mut client = Scripted { step: 0 };
    let answer = session.ask(&mut client, "what uses space?").unwrap();
    assert!(answer.contains("biggest category"));
}

// ---------------------------------------------------------------------------
// Clean Up cards (crates/core/src/cards.rs)
//
// Fixture files are always created "now", so card age thresholds are
// tested by passing a FUTURE now_ms to cleanup_cards: at +200 days every
// fixture file is 200 days old, at +45 days only the 30-day installer
// threshold is crossed, and so on.
// ---------------------------------------------------------------------------

use diskpilot_core::cards::{cleanup_cards, CandidateFile, CandidateGroup};

const DAY: i64 = 86_400_000;

fn cards_fixture(dir: &Path) {
    // Installers living in Downloads: exclusivity must put them in
    // old-installers ONLY, never in forgotten-downloads.
    write_file(&dir.join("Downloads/SetupTool.exe"), 5000);
    write_file(&dir.join("Downloads/driver.msi"), 4000);
    write_file(&dir.join("Downloads/forgotten.zip"), 3000);
    // Zero-byte installer name: skipped everywhere.
    write_file(&dir.join("Downloads/setup-empty.exe"), 0);
    // Screenshots by name and by folder; a plain photo is neither.
    write_file(&dir.join("Pictures/Screenshot 2024.png"), 2000);
    write_file(&dir.join("Pictures/Screenshots/shot.jpg"), 1500);
    write_file(&dir.join("Pictures/photo.jpg"), 2500);
    // One video over the 100 MB floor, one under it.
    write_file(&dir.join("Videos/big.mp4"), 100_000_001);
    write_file(&dir.join("Videos/small.mp4"), 5000);
    write_file(&dir.join("misc/readme.txt"), 1000);
}

fn card_group<'a>(groups: &'a [CandidateGroup], card: &str) -> Option<&'a CandidateGroup> {
    groups.iter().find(|g| g.card == card)
}

fn card_names(group: &CandidateGroup) -> Vec<&str> {
    group.files.iter().map(|f| f.name.as_str()).collect()
}

#[test]
fn cleanup_cards_classify_with_exclusivity() {
    let dir = tempfile::tempdir().unwrap();
    cards_fixture(dir.path());
    let (_summary, index) = Scanner::new(dir.path()).scan();
    let t0 = index.files.iter().map(|f| f.modified_ms).max().unwrap();
    let groups = cleanup_cards(&index, t0 + 200 * DAY);

    // Fixed card order, all four present.
    let order: Vec<&str> = groups.iter().map(|g| g.card.as_str()).collect();
    assert_eq!(
        order,
        vec![
            "old-installers",
            "forgotten-downloads",
            "old-screenshots",
            "big-old-videos"
        ]
    );
    let titles: Vec<&str> = groups.iter().map(|g| g.title.as_str()).collect();
    assert_eq!(
        titles,
        vec![
            "Old installers",
            "Forgotten downloads",
            "Old screenshots",
            "Big old videos"
        ]
    );

    let installers = card_group(&groups, "old-installers").unwrap();
    let mut names = card_names(installers);
    names.sort();
    assert_eq!(names, vec!["SetupTool.exe", "driver.msi"]);
    // Files sorted largest first; the group total is the file sum.
    assert!(installers
        .files
        .windows(2)
        .all(|w| w[0].bytes >= w[1].bytes));
    assert_eq!(
        installers.total_bytes,
        installers.files.iter().map(|f| f.bytes).sum::<u64>()
    );
    // Ids are the stable 16-hex-char path hashes.
    assert!(installers
        .files
        .iter()
        .all(|f| f.id.len() == 16 && f.id.chars().all(|c| c.is_ascii_hexdigit())));

    // Exclusivity: the Downloads installers are NOT here.
    let downloads = card_group(&groups, "forgotten-downloads").unwrap();
    assert_eq!(card_names(downloads), vec!["forgotten.zip"]);

    let shots = card_group(&groups, "old-screenshots").unwrap();
    let mut names = card_names(shots);
    names.sort();
    assert_eq!(names, vec!["Screenshot 2024.png", "shot.jpg"]);

    let videos = card_group(&groups, "big-old-videos").unwrap();
    assert_eq!(card_names(videos), vec!["big.mp4"]);

    // The zero-byte installer, the plain photo, the small video and the
    // readme appear in no card at all: 6 candidates in total.
    let total: usize = groups.iter().map(|g| g.files.len()).sum();
    assert_eq!(total, 6);
    let all_names: Vec<&str> = groups.iter().flat_map(|g| card_names(g)).collect();
    for absent in ["setup-empty.exe", "photo.jpg", "small.mp4", "readme.txt"] {
        assert!(
            !all_names.contains(&absent),
            "{absent} must not be a candidate"
        );
    }
}

#[test]
fn cleanup_cards_age_thresholds() {
    let dir = tempfile::tempdir().unwrap();
    cards_fixture(dir.path());
    let (_summary, index) = Scanner::new(dir.path()).scan();
    let t0 = index.files.iter().map(|f| f.modified_ms).max().unwrap();

    // Brand-new files: no card fires.
    assert!(cleanup_cards(&index, t0).is_empty());
    // At +45 days only the 30-day installer threshold is crossed.
    let groups = cleanup_cards(&index, t0 + 45 * DAY);
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].card, "old-installers");
    // At +100 days the 90-day cards join; videos (180 days) still wait.
    let groups = cleanup_cards(&index, t0 + 100 * DAY);
    let order: Vec<&str> = groups.iter().map(|g| g.card.as_str()).collect();
    assert_eq!(
        order,
        vec!["old-installers", "forgotten-downloads", "old-screenshots"]
    );
}

#[test]
fn card_plan_is_trash_only_single_use_with_expiry() {
    let dir = tempfile::tempdir().unwrap();
    cards_fixture(dir.path());
    let (_summary, index) = Scanner::new(dir.path()).scan();
    let t0 = index.files.iter().map(|f| f.modified_ms).max().unwrap();
    let groups = cleanup_cards(&index, t0 + 200 * DAY);
    let installers = card_group(&groups, "old-installers").unwrap();
    let ids: Vec<String> = installers.files.iter().map(|f| f.id.clone()).collect();

    let c = ctx(dir.path());
    let now = 5_000_000i64;
    let mut planner = Planner::new();
    let plan = planner
        .build_plan_for_candidates(&ids, &groups, &c, now)
        .unwrap();
    // Trash-only, hard rule — even though these are regenerable
    // installer downloads, permanent deletion is never offered.
    assert!(!plan.allow_permanent);
    assert_eq!(plan.items.len(), 2);
    assert_eq!(plan.total_bytes, installers.total_bytes);
    assert!(plan.rejected.is_empty());
    // Same single-use + expiry semantics as recommendation plans.
    assert!(planner.take_plan(&plan.id, now + 1000).is_ok());
    assert!(planner.take_plan(&plan.id, now + 1000).is_err());
    let plan2 = planner
        .build_plan_for_candidates(&ids, &groups, &c, now)
        .unwrap();
    assert!(planner
        .take_plan(
            &plan2.id,
            now + diskpilot_core::cleanup::planner::PLAN_TTL_MS + 1
        )
        .is_err());
}

#[test]
fn card_plan_rejects_unknown_ids_and_protected_paths() {
    let dir = tempfile::tempdir().unwrap();
    cards_fixture(dir.path());
    let (_summary, index) = Scanner::new(dir.path()).scan();
    let t0 = index.files.iter().map(|f| f.modified_ms).max().unwrap();
    let groups = cleanup_cards(&index, t0 + 200 * DAY);
    let valid = card_group(&groups, "forgotten-downloads").unwrap().files[0]
        .id
        .clone();

    let c = ctx(dir.path());
    let mut planner = Planner::new();
    let plan = planner
        .build_plan_for_candidates(
            &[valid, "deadbeefdeadbeef".to_string()],
            &groups,
            &c,
            6_000_000,
        )
        .unwrap();
    assert_eq!(plan.items.len(), 1);
    assert_eq!(plan.rejected.len(), 1);
    assert!(plan.rejected[0].contains("unknown candidate id"));

    // A fabricated group pointing at a protected location: the
    // planner does not trust that groups came from detection — the
    // safety gate judges every resolved path.
    let hostile = CandidateGroup {
        card: "old-installers".to_string(),
        title: "Old installers".to_string(),
        files: vec![CandidateFile {
            id: "0123456789abcdef".to_string(),
            path: "/etc/hostname".to_string(),
            name: "hostname".to_string(),
            bytes: 10,
            modified_ms: 1,
        }],
        total_bytes: 10,
    };
    let plan = planner
        .build_plan_for_candidates(&["0123456789abcdef".to_string()], &[hostile], &c, 6_000_000)
        .unwrap();
    assert!(plan.items.is_empty());
    assert_eq!(plan.rejected.len(), 1);
    assert!(plan.rejected[0].contains("/etc/hostname"));
    assert!(!plan.allow_permanent);
}

#[test]
fn state_stores_cleanup_cards_with_scan() {
    let dir = tempfile::tempdir().unwrap();
    cards_fixture(dir.path());
    let (summary, index) = Scanner::new(dir.path()).scan();
    let mut state = diskpilot_core::AppState::new();
    state.store_scan(summary, index);
    // Fixture files were created just now, so no age threshold is
    // crossed and the stored groups match a fresh detection pass.
    assert!(state.cleanup_cards.is_empty());
    state.clear_scan();
    assert!(state.cleanup_cards.is_empty());
    assert!(state.recommendations.is_empty());
}
