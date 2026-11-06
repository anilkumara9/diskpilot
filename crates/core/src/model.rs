//! Shared data model. Serde names are camelCase to mirror the TypeScript types.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord,
)]
#[serde(rename_all = "camelCase")]
pub enum Category {
    Cache,
    BuildArtifacts,
    DockerStorage,
    ApplicationData,
    Logs,
    Downloads,
    UserData,
    BinaryData,
    SystemFiles,
    DeveloperTools,
    #[default]
    Unknown,
}

impl Category {
    pub fn all() -> [Category; 11] {
        [
            Category::Cache,
            Category::BuildArtifacts,
            Category::DockerStorage,
            Category::ApplicationData,
            Category::Logs,
            Category::Downloads,
            Category::UserData,
            Category::BinaryData,
            Category::SystemFiles,
            Category::DeveloperTools,
            Category::Unknown,
        ]
    }
    pub fn label(&self) -> &'static str {
        match self {
            Category::Cache => "Cache",
            Category::BuildArtifacts => "Build artifacts",
            Category::DockerStorage => "Docker storage",
            Category::ApplicationData => "Application data",
            Category::Logs => "Logs",
            Category::Downloads => "Downloads",
            Category::UserData => "User data",
            Category::BinaryData => "Binary data",
            Category::SystemFiles => "System files",
            Category::DeveloperTools => "Developer tools",
            Category::Unknown => "Unknown",
        }
    }
    pub fn is_recoverable(&self) -> bool {
        matches!(
            self,
            Category::Cache | Category::BuildArtifacts | Category::Logs | Category::DockerStorage
        )
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Tier {
    LikelySafe,
    ReviewFirst,
    Keep,
}
impl Tier {
    pub fn label(&self) -> &'static str {
        match self {
            Tier::LikelySafe => "Likely safe",
            Tier::ReviewFirst => "Review first",
            Tier::Keep => "Keep",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Confidence {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Classification {
    pub category: Category,
    pub rule_id: String,
    pub label: String,
    pub explanation: String,
    pub confidence: Confidence,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inherited_from: Option<String>,
}
impl Classification {
    pub fn new(
        category: Category,
        rule_id: impl Into<String>,
        label: impl Into<String>,
        explanation: impl Into<String>,
        confidence: Confidence,
    ) -> Self {
        Self {
            category,
            rule_id: rule_id.into(),
            label: label.into(),
            explanation: explanation.into(),
            confidence,
            inherited_from: None,
        }
    }
    pub fn inherited(mut self, from: impl Into<String>) -> Self {
        self.inherited_from = Some(from.into());
        self
    }
    pub fn unknown() -> Self {
        Self::new(
            Category::Unknown,
            "fallback.unknown",
            "Unknown",
            "No classification rule matched this item.",
            Confidence::Low,
        )
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileEntry {
    pub path: String,
    pub name: String,
    pub bytes: u64,
    pub category: Category,
    /// Last-modified time, milliseconds since the Unix epoch (0 when the
    /// platform metadata carried no usable mtime). `serde(default)` keeps
    /// snapshots written before this field existed loadable.
    #[serde(default)]
    pub modified_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DirNode {
    pub path: String,
    pub name: String,
    pub bytes: u64,
    pub category: Category,
    pub file_count: u64,
    pub dir_count: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocationEntry {
    pub path: String,
    pub name: String,
    pub bytes: u64,
    pub category: Category,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanError {
    pub path: String,
    pub message: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanSummary {
    pub root: String,
    pub started_at_ms: i64,
    pub finished_at_ms: i64,
    pub duration_ms: i64,
    /// Free space on the scanned root's drive at scan end, when the
    /// platform could report it. `serde(default)` keeps snapshots
    /// written before this field existed loadable (as `None`).
    #[serde(default)]
    pub drive_free_bytes: Option<u64>,
    pub total_bytes: u64,
    pub file_count: u64,
    pub dir_count: u64,
    pub category_totals: HashMap<Category, u64>,
    pub recoverable_by_category: HashMap<Category, u64>,
    pub top_files: Vec<FileEntry>,
    pub locations: Vec<LocationEntry>,
    pub heavy_folders: Vec<LocationEntry>,
    pub errors: Vec<ScanError>,
    pub skipped: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CleanupMethod {
    MoveToTrash,
    PermanentDelete,
    RunCommand,
    ManualReview,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Recommendation {
    pub id: String,
    pub title: String,
    pub paths: Vec<String>,
    pub bytes: u64,
    pub tier: Tier,
    pub methods: Vec<CleanupMethod>,
    pub why_detected: String,
    pub why_safe: String,
    pub risks: String,
    pub confidence: Confidence,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alternative_command: Option<String>,
    /// Guided-only recommendations are never cleaned by DiskPilot
    /// itself: the planner rejects them outright, and the UI shows
    /// `guidance` (the operating system's own removal route) instead of
    /// a cleanup action. `serde(default)` keeps recommendations
    /// serialised before this field existed loadable.
    #[serde(default)]
    pub guided_only: bool,
    /// The operating-system route for removing a guided-only item,
    /// shown verbatim to the user.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub guidance: Option<String>,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanItem {
    pub path: String,
    pub bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupPlan {
    pub id: String,
    pub created_at_ms: i64,
    pub expires_at_ms: i64,
    pub items: Vec<PlanItem>,
    pub warnings: Vec<String>,
    pub rejected: Vec<String>,
    pub total_bytes: u64,
    pub allow_permanent: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupFailure {
    pub path: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupResult {
    pub plan_id: String,
    pub succeeded: Vec<String>,
    pub failed: Vec<CleanupFailure>,
    pub reclaimed_bytes: u64,
    pub finished_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    pub id: i64,
    pub timestamp_ms: i64,
    pub operation: String,
    pub paths: Vec<String>,
    pub reclaimed_bytes: u64,
    pub errors: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DuplicateFile {
    pub path: String,
    /// Allocated bytes on disk — the user-facing figure (see the
    /// byte-accounting rule in scanner/mod.rs docs). Copies of one file
    /// can legitimately differ here (compression, sparse blocks).
    pub bytes: u64,
    pub is_clone: bool,
    pub is_hardlink: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DuplicateGroup {
    /// Logical size shared by every copy. This is the grouping key (an
    /// internal correctness input), NOT a display figure — displayed
    /// per-file sizes are the allocated `DuplicateFile::bytes`.
    pub size: u64,
    pub hash: String,
    pub files: Vec<DuplicateFile>,
}
impl DuplicateGroup {
    /// Space freed by removing every copy except the keeper. The keeper
    /// is the first file (the same default the UI pre-selects). The
    /// figure is allocated-based: each removed copy contributes its
    /// allocated bytes, except hard links to the keeper, which free
    /// nothing because the inode survives with the keeper.
    pub fn wasted_bytes(&self) -> u64 {
        self.files
            .iter()
            .skip(1)
            .map(|f| if f.is_hardlink { 0 } else { f.bytes })
            .sum()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DockerImage {
    pub repository: String,
    pub tag: String,
    pub id: String,
    pub size_bytes: u64,
    pub shared_bytes: u64,
    pub unique_bytes: u64,
    pub containers: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DockerContainer {
    pub id: String,
    pub name: String,
    pub image: String,
    pub state: String,
    pub size_bytes: u64,
    pub compose_project: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DockerVolume {
    pub name: String,
    pub links: i64,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DockerOverview {
    pub installed: bool,
    pub running: bool,
    pub detail: String,
    pub images: Vec<DockerImage>,
    pub containers: Vec<DockerContainer>,
    pub volumes: Vec<DockerVolume>,
    pub images_total_bytes: u64,
    pub images_reclaimable_bytes: u64,
    pub containers_total_bytes: u64,
    pub containers_reclaimable_bytes: u64,
    pub volumes_total_bytes: u64,
    pub volumes_reclaimable_bytes: u64,
    pub build_cache_bytes: u64,
    pub build_cache_reclaimable_bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ToolItemState {
    Current,
    Stale,
    Orphaned,
    Superseded,
    Informational,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolItem {
    pub name: String,
    pub path: String,
    pub bytes: u64,
    pub state: ToolItemState,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolSection {
    pub title: String,
    pub items: Vec<ToolItem>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolReport {
    pub tool: String,
    pub sections: Vec<ToolSection>,
}
impl ToolReport {
    pub fn total_bytes(&self) -> u64 {
        self.sections
            .iter()
            .flat_map(|s| s.items.iter().map(|i| i.bytes))
            .sum()
    }
    pub fn is_empty(&self) -> bool {
        self.sections.iter().all(|s| s.items.is_empty())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppEntry {
    pub name: String,
    pub publisher: String,
    pub install_bytes: u64,
    pub data_bytes: u64,
    pub data_paths: Vec<String>,
    pub leftover: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiskInfo {
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub available_bytes: u64,
    pub volume_name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SecretFinding {
    pub path: String,
    pub kind: String,
    pub masked: String,
    pub line: usize,
}

pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

pub fn path_to_string(p: &std::path::Path) -> String {
    p.to_string_lossy().to_string()
}

pub fn home_dir_string() -> String {
    dirs::home_dir()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default()
}

pub fn path_buf(s: &str) -> PathBuf {
    PathBuf::from(s)
}
