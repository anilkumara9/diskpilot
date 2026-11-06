/* Shared contract between the React frontend and the Rust engine.
   All shapes mirror the engine's serde camelCase types exactly. */

export type Category =
  | "cache"
  | "buildArtifacts"
  | "developerTools"
  | "dockerStorage"
  | "applicationData"
  | "logs"
  | "downloads"
  | "userData"
  | "binaryData"
  | "systemFiles"
  | "unknown";

export const CATEGORIES: Category[] = [
  "cache",
  "buildArtifacts",
  "developerTools",
  "dockerStorage",
  "applicationData",
  "logs",
  "downloads",
  "userData",
  "binaryData",
  "systemFiles",
  "unknown",
];

export type Tier = "likelySafe" | "reviewFirst" | "keep";
export type Confidence = "high" | "medium" | "low";

export interface Classification {
  category: Category;
  ruleId: string;
  label: string;
  explanation: string;
  confidence: Confidence;
  inheritedFrom?: string | null;
}

export interface FileEntry {
  path: string;
  name: string;
  bytes: number;
  category: Category;
}

export interface DirNode {
  path: string;
  name: string;
  bytes: number;
  category: Category;
  fileCount: number;
  dirCount: number;
}

/** One entry of a directory listing, as returned by the shell's
    `list_directory` command (mirrors the core explorer entry plus the
    folder classification when the entry is a directory). */
export interface DirectoryEntry {
  name: string;
  path: string;
  bytes: number;
  isDir: boolean;
  category: Category;
  classification?: Classification | null;
}

export interface LocationEntry {
  path: string;
  name: string;
  bytes: number;
  category: Category;
}

export interface ScanError {
  path: string;
  message: string;
}

export interface ScanSummary {
  root: string;
  startedAtMs: number;
  finishedAtMs: number;
  durationMs: number;
  totalBytes: number;
  fileCount: number;
  dirCount: number;
  categoryTotals: Record<Category, number>;
  recoverableByCategory: Record<Category, number>;
  topFiles: FileEntry[];
  locations: LocationEntry[];
  heavyFolders: LocationEntry[];
  errors: ScanError[];
  skipped: string[];
}

export interface ScanProgress {
  currentPath: string;
  filesSeen: number;
  bytesSeen: number;
}

export interface Recommendation {
  id: string;
  title: string;
  paths: string[];
  bytes: number;
  tier: Tier;
  methods: string[];
  whyDetected: string;
  whySafe: string;
  risks: string;
  confidence: Confidence;
  alternativeCommand?: string | null;
  source: string;
}

export interface CleanupPlanItem {
  path: string;
  bytes: number;
}

export interface CleanupPlan {
  id: string;
  createdAtMs: number;
  expiresAtMs: number;
  items: CleanupPlanItem[];
  warnings: string[];
  rejected: string[];
  totalBytes: number;
  allowPermanent: boolean;
}

export type CleanupMode = "recycleBin" | "permanent";

/* ---------- Clean Up cards (core `cards` module) ---------- */

/** One file offered by a Clean Up card. `id` is core-assigned and
    stable for the scan (a hash of the path); cleanup plans are built
    from these ids only, never from paths sent by the client. */
export interface CandidateFile {
  id: string;
  path: string;
  name: string;
  bytes: number;
  modifiedMs: number;
}

/** All files for one Clean Up card. The core returns only non-empty
    groups, in fixed card order. */
export interface CandidateGroup {
  card: string;
  title: string;
  files: CandidateFile[];
  totalBytes: number;
}

export interface CleanupFailure {
  path: string;
  message: string;
}

export interface CleanupResult {
  planId: string;
  succeeded: string[];
  failed: CleanupFailure[];
  reclaimedBytes: number;
  finishedAtMs: number;
}

export interface HistoryEntry {
  id: number;
  timestampMs: number;
  operation: string;
  paths: string[];
  reclaimedBytes: number;
  errors: string[];
  detail?: string | null;
}

export interface DuplicateFile {
  path: string;
  bytes: number;
  isClone: boolean;
  isHardlink: boolean;
  modifiedAtMs?: number | null;
}

export interface DuplicateGroup {
  size: number;
  hash: string;
  files: DuplicateFile[];
}

export type DockerState = "notInstalled" | "notRunning" | "ready";

export interface DockerContainer {
  id: string;
  name: string;
  image: string;
  state: string;
  sizeBytes: number;
  composeProject?: string | null;
}

export interface DockerImage {
  repository: string;
  tag: string;
  id: string;
  sizeBytes: number;
  sharedBytes: number;
  uniqueBytes: number;
  containers: number;
}

export interface DockerVolume {
  name: string;
  links: number;
  sizeBytes: number;
}

/** Matches the shell's Docker overview DTO exactly: every core
    DockerOverview field, plus the WSL virtual-disk facts composed by
    the shell. `vhdxPath`/`vhdxBytes`/`vhdxCompactionSteps` are null when
    no Docker virtual disk exists on this machine. The page derives the
    display state from `installed`/`running`. */
export interface DockerOverview {
  installed: boolean;
  running: boolean;
  detail: string;
  images: DockerImage[];
  containers: DockerContainer[];
  volumes: DockerVolume[];
  imagesTotalBytes: number;
  imagesReclaimableBytes: number;
  containersTotalBytes: number;
  containersReclaimableBytes: number;
  volumesTotalBytes: number;
  volumesReclaimableBytes: number;
  buildCacheBytes: number;
  buildCacheReclaimableBytes: number;
  vhdxPath: string | null;
  vhdxBytes: number | null;
  vhdxCompactionSteps: string[] | null;
}

export type DockerActionKind =
  | "removeStoppedContainer"
  | "removeUnusedImage"
  | "pruneBuildCache";

export interface DockerActionRequest {
  kind: DockerActionKind;
  id?: string | null;
  name?: string | null;
  reference?: string | null;
}

export interface DockerActionPreview {
  command: string;
  description: string;
}

export interface DockerActionResult {
  command: string;
  output: string;
}

export type ToolItemState =
  | "current"
  | "stale"
  | "orphaned"
  | "superseded"
  | "informational";

export interface ToolItem {
  name: string;
  path: string;
  bytes: number;
  state: ToolItemState;
  detail: string;
}

export interface ToolSection {
  title: string;
  items: ToolItem[];
}

export interface ToolReport {
  tool: string;
  sections: ToolSection[];
}

export interface AppEntry {
  name: string;
  publisher: string;
  installBytes: number;
  dataBytes: number;
  dataPaths: string[];
  leftover: boolean;
}

export interface DiskInfo {
  totalBytes: number;
  usedBytes: number;
  availableBytes: number;
  volumeName: string;
}

export interface EnvironmentInfo {
  os: string;
  arch: string;
  homeDir: string;
  appVersion: string;
}

export type ThemePreference = "system" | "light" | "dark";
export type AssistantProvider = "anthropic" | "openai" | "openrouter";

export interface AppSettings {
  scanRoot: string;
  theme: ThemePreference;
  assistantEnabled: boolean;
  assistantProvider: AssistantProvider;
  /** Folders and patterns excluded from scans (a folder or file name,
      or a wildcard like `*.iso`). Older settings files without the
      field load with an empty list. */
  exclusions: string[];
  /** Natural-language searches bookmarked from the Storage page.
      Older settings files without the field load with an empty list. */
  savedSearches: string[];
}

/* ---------- Downloads & provenance (core `provenance` module) ---------- */

/** The source Windows recorded for a downloaded file, from its
    Zone.Identifier zone data. URLs are null when the stream did not
    record them; the label is always present. */
export interface DownloadSource {
  zoneId: number | null;
  hostUrl: string | null;
  referrerUrl: string | null;
  label: string;
}

/** One recent file in the Downloads folder. */
export interface RecentFile {
  path: string;
  name: string;
  bytes: number;
  modifiedMs: number;
  source: DownloadSource | null;
}

export interface ProvenanceFile {
  path: string;
  bytes: number;
  modifiedMs: number;
}

/** All downloads in the folder that share one recorded source. */
export interface ProvenanceGroup {
  label: string;
  host: string | null;
  files: ProvenanceFile[];
  totalBytes: number;
}

/* ---------- Natural-language search (core `search` module) ---------- */

/** Which dimension of the query a chip describes. Exactly one chip
    is produced per dimension, so removing a chip clears exactly one
    dimension of the filters. */
export type ChipKind = "kind" | "size" | "date" | "source" | "term";

export interface QueryChip {
  kind: ChipKind;
  label: string;
}

/** The structured form of a parsed query, mirroring the core's
    SearchFilters exactly (serde camelCase). `search_files` returns
    this object; the client may drop one dimension (chip removal) and
    send it back to `run_search_filters` unchanged otherwise. */
export interface SearchFilters {
  kinds: string[];
  minBytes: number | null;
  maxBytes: number | null;
  modifiedAfterMs: number | null;
  modifiedBeforeMs: number | null;
  source: string | null;
  terms: string[];
}

export interface SearchHit {
  path: string;
  name: string;
  bytes: number;
  category: Category;
  modifiedMs: number;
}

/** `search_files` response: chips + echoed filters + first hits. */
export interface SearchResponse {
  chips: QueryChip[];
  filters: SearchFilters;
  hits: SearchHit[];
  totalMatches: number;
}

/** `run_search_filters` response. */
export interface SearchHitsResponse {
  hits: SearchHit[];
  totalMatches: number;
}

/* ---------- Similar images & media info ---------- */

export interface SimilarImage {
  path: string;
  bytes: number;
  width: number;
  height: number;
}

/** A set of look-alike images. Files are largest first — the first
    entry is the keeper the core designates. */
export interface SimilarGroup {
  files: SimilarImage[];
  wastedBytes: number;
}

/** EXIF metadata for one image file; every field is nullable because
    files routinely carry partial metadata. */
export interface MediaInfo {
  width: number | null;
  height: number | null;
  make: string | null;
  model: string | null;
  takenAtMs: number | null;
}

/** One category's change between the previous scan and the current
    one. `deltaBytes` is signed: negative means the category shrank. */
export interface CategoryDelta {
  category: Category;
  currentBytes: number;
  previousBytes: number;
  deltaBytes: number;
}

/** Current scan vs. the previous scan of the same root, as returned by
    the shell's `get_scan_comparison`. Deltas are signed: a negative
    `totalBytesDelta` means space was freed overall. Null when there is
    no previous scan or the root changed. */
export interface ScanComparison {
  previousScannedAtMs: number;
  totalBytesDelta: number;
  fileCountDelta: number;
  categories: CategoryDelta[];
}

export interface RecycleBinItem {
  id: string;
  name: string;
  originalPath: string;
  bytes: number;
  deletedAtMs: number;
}

export interface EmptyRecycleBinProgress {
  completed: number;
  total: number;
  currentName: string;
  freedBytes: number;
}

export interface SecretFinding {
  path: string;
  kind: string;
  masked: string;
  line: number;
}

export interface ChatMessage {
  role: "user" | "assistant";
  content: string;
}

export interface AssistantReply {
  content: string;
  highlightedRecommendationIds: string[];
}

/** Local UI message: a chat message plus any recommendations the
    assistant highlighted in that reply (kept for the review button).
    `sentAtMs` is local-only display metadata (the timestamp under the
    message); it is never sent to the core — the history payload maps
    back down to plain ChatMessage. */
export interface AssistantMessage extends ChatMessage {
  highlightedRecommendationIds?: string[];
  sentAtMs?: number;
}

export type PageId =
  | "overview"
  | "storage"
  | "duplicates"
  | "applications"
  | "developer"
  | "docker"
  | "downloads"
  | "assistant"
  | "cleanup"
  | "history"
  | "settings";
