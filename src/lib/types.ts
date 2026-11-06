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
    assistant highlighted in that reply (kept for the review button). */
export interface AssistantMessage extends ChatMessage {
  highlightedRecommendationIds?: string[];
}

export type PageId =
  | "overview"
  | "storage"
  | "duplicates"
  | "applications"
  | "developer"
  | "docker"
  | "assistant"
  | "cleanup"
  | "history"
  | "settings";
