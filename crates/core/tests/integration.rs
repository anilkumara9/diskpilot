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
fn hardlink_counted_once() {
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("a.bin");
    write_file(&a, 6000);
    let b = dir.path().join("b.bin");
    #[cfg(unix)]
    {
        std::fs::hard_link(&a, &b).unwrap();
        let (summary, _index) = Scanner::new(dir.path()).scan();
        assert_eq!(summary.file_count, 2);
        // Two directory entries, one inode: bytes counted once. Logical
        // counted size equals one file's allocated size, well under 2x.
        let one = diskpilot_core::scanner::allocated_size_for_path(&a);
        assert_eq!(summary.total_bytes, one);
        // Duplicates finder also treats them as the same content but the
        // scanner-level dedup is what matters here.
        let _ = b;
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
        .record_cleanup("move-to-trash", &result, None)
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
