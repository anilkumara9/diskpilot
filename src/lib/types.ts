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
  /** True when the live index is actively following this scan's
      drive (decorated by the shell when the summary is served;
      absent on summaries from older builds). */
  liveIndexActive?: boolean;
  /** True when the live index could not be kept exactly current
      (journal wrapped or a change could not be applied) and the
      next scan will reconcile it. */
  indexStale?: boolean;
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
  /** True when DiskPilot cannot act on this item itself and can only
      guide the user to the tool that can (e.g. Windows.old, which
      belongs to Disk Cleanup / Storage Sense). Guided items render
      without a selection checkbox and never enter a cleanup plan. */
  guidedOnly?: boolean;
  /** The step-by-step guidance shown for a `guidedOnly` item. */
  guidance?: string;
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
  /** For cleanup runs: how many of the run's Recycle Bin items the
      shell can still restore to their original locations. Absent or
      0 means nothing from this entry can be restored. */
  restorableCount?: number;
}

/** `restore_cleanup_run` result: what came back, what failed, and
    how many of the run's items were permanently deleted (and so
    could not be restored at all). */
export interface RestoreCleanupRunResult {
  restored: string[];
  failed: { path: string; error: string }[];
  skippedPermanent: number;
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
  /** Whether the command itself succeeded, as reported by the
      shell. Optional because older shell builds did not send it:
      only an explicit `false` renders as a failure. */
  success?: boolean;
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
export type AutoScanSchedule = "off" | "daily" | "weekly";

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
  /** Scheduled background scans while the app sits in the tray.
      Older settings files without the field load as "off". */
  autoScan: AutoScanSchedule;
  /** After a scheduled scan, notify when the scanned drive's free
      space drops below this many GB (decimal). */
  lowSpaceAlertGb: number;
  /** After a scheduled scan, notify when total usage grew by more
      than this many GB (decimal) since the previous scan. */
  growthAlertGb: number;
  /** Launch DiskPilot when the user signs in to Windows. */
  autoStart: boolean;
  /** True once the first-run welcome has been completed or skipped.
      Older settings files without the field load as false. */
  onboarded: boolean;
  /** Read text inside images and screenshots (Windows built-in OCR,
      on-device) so plain-language search can match words in them.
      Older settings files without the field load as true. */
  ocrEnabled: boolean;
  /** Follow the scanned drive's NTFS change journal while the app
      runs, applying changes to the index between scans. Needs
      administrator volume access; without it the index simply
      refreshes on each scan. Older settings files without the
      field load as true. */
  liveIndex: boolean;
  /** Automatic maintenance (see `AutoMaintenanceSettings`). Older
      settings files without the field load as disabled. */
  autoMaintenance: AutoMaintenanceSettings;
  /** When automatic maintenance last ran (ms since epoch), as
      recorded by the shell; null when it has never run. */
  lastMaintenanceMs: number | null;
  /** Sidebar in its icon-only collapsed mode. Older settings
      files without the field load as false. */
  sidebarCollapsed: boolean;
}

/** Automatic maintenance: when due, the shell permanently removes
    Recycle Bin items older than `minAgeDays` and clears old
    temporary files through the same safety checks as a manual
    cleanup. Every run is recorded in History. */
export interface AutoMaintenanceSettings {
  enabled: boolean;
  frequency: "daily" | "weekly";
  minAgeDays: number;
}

/* ---------- Watch: scan history, forecast, biggest changes ---------- */

/** One completed scan in the persisted time series (oldest first).
    `categories` keys are the categories' serde names, exactly as a
    snapshot's `categoryTotals` map uses them. */
export interface ScanHistoryEntry {
  scannedAtMs: number;
  rootPath: string;
  totalBytes: number;
  fileCount: number;
  driveFreeBytes: number | null;
  categories: Record<string, number>;
}

/** Least-squares space forecast over the scan history for the
    current root. `daysUntilFull`/`projectedFullAtMs` are null when
    usage is flat or shrinking, or too few points exist. */
export interface SpaceForecast {
  dailyGrowthBytes: number;
  daysUntilFull: number | null;
  projectedFullAtMs: number | null;
  pointsUsed: number;
}

/** One directory's change between the previous and current scan. */
export interface DirChange {
  path: string;
  name: string;
  previousBytes: number;
  currentBytes: number;
  deltaBytes: number;
}

/** Directory-level diff of the current snapshot vs. the previous one. */
export interface BiggestChanges {
  grown: DirChange[];
  shrunk: DirChange[];
  added: DirChange[];
  removed: DirChange[];
}

/* ---------- Drives & cloud storage ---------- */

/** One drive (volume root) on this PC, from `get_drives`. */
export interface DriveInfo {
  rootPath: string;
  totalBytes: number;
  freeBytes: number;
}

/** A cloud sync folder (e.g. OneDrive) found on this PC and how much
    of it is currently stored locally. Null when none is present. */
export interface CloudStatus {
  provider: string;
  rootPath: string;
  localBytes: number;
  fileCount: number;
}

export interface FreeUpFailure {
  path: string;
  error: string;
}

/** Result of marking files online-only: they stay in the cloud and
    re-download when opened — nothing is deleted. */
export interface FreeUpResult {
  succeeded: string[];
  failed: FreeUpFailure[];
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
  /** True when the hit matched only through text read inside the
      file (OCR), not through its name. */
  matchedInText?: boolean;
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

/* ---------- Media summary & games (core `media` / `games` modules) ---------- */

/** Byte total (allocated) and file count for one media kind. */
export interface MediaBucket {
  bytes: number;
  count: number;
}

/** One file in a biggest-media list. */
export interface MediaItem {
  path: string;
  name: string;
  bytes: number;
  modifiedMs: number;
}

/** Media totals for the current scan, from `get_media_summary`.
    Null until a scan has run. `screenshots` is a subset of
    `photos` — screenshots are photos that also match the
    screenshot rule. Biggest lists carry at most 15 entries,
    largest first. */
export interface MediaSummary {
  photos: MediaBucket;
  videos: MediaBucket;
  screenshots: MediaBucket;
  biggestVideos: MediaItem[];
  biggestPhotos: MediaItem[];
}

/** One installed game, from the launcher's own manifest. */
export interface GameEntry {
  name: string;
  bytes: number;
  path: string;
  appId: string;
}

/** One library folder (Steam) or install-folder group (Epic). */
export interface GameLibrary {
  source: "Steam" | "Epic";
  path: string;
  games: GameEntry[];
  bytes: number;
}

/** Every installed Steam/Epic game found on this machine, from
    `get_games`. Libraries is empty when neither launcher is found. */
export interface GamesReport {
  libraries: GameLibrary[];
  totalBytes: number;
  totalGames: number;
}

/** Which Duplicates view is showing: verified identical files, or
    look-alike photos. Lives in the store so other pages (Media) can
    deep-link to a view. */
export type DuplicatesMode = "exact" | "similar";

/* ---------- Startup items & compression (core shell commands) ---------- */

/** Where a sign-in entry was registered. */
export type StartupSource =
  | "registryUser"
  | "registryMachine"
  | "folderUser"
  | "folderMachine";

/** One program that opens when the user signs in to Windows.
    `targetPath`/`targetBytes` are null when the entry's command
    does not resolve to a single file on disk; `targetExists`
    reports whether that file is there. Disabling is reversible:
    the shell keeps the entry and restores it exactly on
    re-enable. */
export interface StartupItem {
  id: string;
  name: string;
  command: string;
  source: StartupSource;
  targetPath: string | null;
  targetExists: boolean;
  targetBytes: number | null;
  enabled: boolean;
}

/** A large, long-untouched folder Windows could compress in
    place. Folders already compressed are filtered out by the
    shell, so every candidate returned can be acted on. */
export interface CompressionCandidate {
  path: string;
  bytes: number;
  fileCount: number;
  newestModifiedMs: number;
  compressed: boolean;
}

/** Result of setting a folder's compression state: `ok` false
    carries the failure in `error`, never a throw, so the row can
    report it in place. */
export interface SetCompressionResult {
  path: string;
  ok: boolean;
  error: string | null;
}

/** Allocated bytes and file count for one file extension in the
    current scan. `extension` is the bare extension ("zip"); the
    empty string means files with no extension. */
export interface ExtensionStat {
  extension: string;
  bytes: number;
  count: number;
}

/* ---------- App junk, drive health, software updates, moves ---------- */

/** One application's measured cache locations, from
    `list_app_junk`. `paths` are the cache folders themselves;
    cleaning moves their CONTENTS to the Recycle Bin. */
export interface AppJunkGroup {
  id: string;
  app: string;
  label: string;
  note: string;
  paths: string[];
  bytes: number;
  fileCount: number;
  /** True when only part of this cache could be measured, so
      the size shown is a lower bound, not the whole cache. */
  truncated: boolean;
}

export interface AppJunkFailure {
  path: string;
  error: string;
}

/** `clean_app_junk` result: which paths were cleaned, which
    failed (and why), and the bytes moved to the Recycle Bin. */
export interface CleanAppJunkResult {
  succeeded: string[];
  failed: AppJunkFailure[];
  reclaimedBytes: number;
}

export type DriveHealthStatus = "healthy" | "caution" | "critical" | "unknown";

/** One physical drive's SMART self-report, from `get_drive_health`.
    Every measured fact is nullable: drives routinely decline to
    report individual values, and the UI says "Not reported"
    rather than guessing. `lifeLeftPercent` is already life LEFT
    (the shell converts wear figures). */
export interface DriveHealth {
  model: string;
  interface: string;
  capacityBytes: number;
  temperatureC: number | null;
  lifeLeftPercent: number | null;
  powerOnHours: number | null;
  powerCycles: number | null;
  reallocatedSectors: number | null;
  predictedFailure: boolean;
  status: DriveHealthStatus;
}

/** One outdated application, from winget via
    `list_available_updates`. Versions are display strings. */
export interface AvailableUpdate {
  name: string;
  id: string;
  currentVersion: string;
  availableVersion: string;
}

/** `list_available_updates` result. `available` is false when
    winget itself is not available on this PC — a real state,
    distinct from "no updates" and from a failed check (`error`). */
export interface AvailableUpdates {
  available: boolean;
  updates: AvailableUpdate[];
  error: string | null;
}

/** One application's update outcome from `update_software`. */
export interface SoftwareUpdateOutcome {
  id: string;
  success: boolean;
  message: string;
}

/** `relocate_folder` / `undo_relocation` result: the folder's
    original location, where it lives now, and its measured size.
    `warning` carries a caveat the shell reports alongside a
    completed move (null when there is nothing to add) — the
    move succeeded, but the user should still see it. */
export interface RelocateResult {
  source: string;
  target: string;
  bytes: number;
  warning: string | null;
}

export type PageId =
  | "overview"
  | "health"
  | "storage"
  | "media"
  | "duplicates"
  | "applications"
  | "games"
  | "startup"
  | "developer"
  | "docker"
  | "downloads"
  | "assistant"
  | "cleanup"
  | "history"
  | "settings";
