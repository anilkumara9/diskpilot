import type {
  AppEntry,
  AppSettings,
  AssistantReply,
  BiggestChanges,
  CandidateGroup,
  ChatMessage,
  CleanupMode,
  CleanupPlan,
  CleanupResult,
  CloudStatus,
  DirectoryEntry,
  DiskInfo,
  DockerActionRequest,
  DockerActionPreview,
  DockerActionResult,
  DockerOverview,
  DriveInfo,
  DuplicateGroup,
  EmptyRecycleBinProgress,
  EnvironmentInfo,
  FreeUpResult,
  GamesReport,
  HistoryEntry,
  MediaInfo,
  MediaSummary,
  PageId,
  ProvenanceGroup,
  RecentFile,
  Recommendation,
  RecycleBinItem,
  RestoreCleanupRunResult,
  ScanComparison,
  ScanHistoryEntry,
  ScanProgress,
  ScanSummary,
  SearchFilters,
  SearchHitsResponse,
  SearchResponse,
  SecretFinding,
  SimilarGroup,
  SpaceForecast,
  ToolReport,
} from "./types";

/** True only when running inside the Tauri desktop shell. */
export function inDesktopApp(): boolean {
  if (typeof window === "undefined") return false;
  const w = window as unknown as Record<string, unknown>;
  return "__TAURI_INTERNALS__" in w || "__TAURI__" in w;
}

async function invoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  const { invoke: tauriInvoke } = await import("@tauri-apps/api/core");
  return tauriInvoke<T>(command, args);
}

function requireDesktop(): void {
  if (!inDesktopApp()) {
    throw new Error("This action is only available in the DiskPilot desktop app.");
  }
}

/* ---------- Environment & disk ---------- */

export async function getEnvironment(): Promise<EnvironmentInfo | null> {
  if (!inDesktopApp()) return null;
  try {
    return await invoke<EnvironmentInfo>("get_environment");
  } catch {
    return null;
  }
}

export async function getDiskInfo(root?: string): Promise<DiskInfo | null> {
  if (!inDesktopApp()) return null;
  try {
    return await invoke<DiskInfo>("get_disk_info", root ? { path: root } : undefined);
  } catch {
    return null;
  }
}

/* ---------- Scanning ---------- */

export async function startScan(root: string): Promise<ScanSummary> {
  requireDesktop();
  return invoke<ScanSummary>("start_scan", { root });
}

export async function cancelScan(): Promise<void> {
  if (!inDesktopApp()) return;
  try {
    await invoke<void>("cancel_scan");
  } catch {
    /* scan already finished */
  }
}

export async function getScanSummary(): Promise<ScanSummary | null> {
  if (!inDesktopApp()) return null;
  try {
    return await invoke<ScanSummary | null>("get_scan_summary");
  } catch {
    return null;
  }
}

/** Current scan vs. the previous scan of the same root; null when
    there is no previous scan or the root changed. */
export async function getScanComparison(): Promise<ScanComparison | null> {
  if (!inDesktopApp()) return null;
  try {
    return await invoke<ScanComparison | null>("get_scan_comparison");
  } catch {
    return null;
  }
}

/** Every drive on this PC with its capacity and free space. */
export async function getDrives(): Promise<DriveInfo[]> {
  if (!inDesktopApp()) return [];
  try {
    return await invoke<DriveInfo[]>("get_drives");
  } catch {
    return [];
  }
}

/* ---------- Watch: history, forecast, biggest changes ---------- */

/** The persisted scan time series, oldest first. */
export async function getScanHistory(): Promise<ScanHistoryEntry[]> {
  if (!inDesktopApp()) return [];
  try {
    return await invoke<ScanHistoryEntry[]>("get_scan_history");
  } catch {
    return [];
  }
}

/** Space forecast for the configured scan root; null off-desktop. */
export async function getSpaceForecast(): Promise<SpaceForecast | null> {
  if (!inDesktopApp()) return null;
  try {
    return await invoke<SpaceForecast>("get_space_forecast");
  } catch {
    return null;
  }
}

/** Biggest directory changes between the current and previous scan. */
export async function getBiggestChanges(limit: number): Promise<BiggestChanges | null> {
  if (!inDesktopApp()) return null;
  try {
    return await invoke<BiggestChanges>("get_biggest_changes", { limit });
  } catch {
    return null;
  }
}

export async function listDirectory(path: string): Promise<DirectoryEntry[]> {
  if (!inDesktopApp()) return [];
  try {
    return await invoke<DirectoryEntry[]>("list_directory", { path });
  } catch {
    return [];
  }
}

/* ---------- Recommendations & cleanup ---------- */

export async function getRecommendations(): Promise<Recommendation[]> {
  if (!inDesktopApp()) return [];
  try {
    return await invoke<Recommendation[]>("get_recommendations");
  } catch {
    return [];
  }
}

export async function buildCleanupPlan(ids: string[]): Promise<CleanupPlan> {
  requireDesktop();
  return invoke<CleanupPlan>("build_cleanup_plan", { recommendationIds: ids });
}

/** Clean Up card candidates for the current scan (old installers,
    forgotten downloads, old screenshots, big old videos). Empty until
    a scan has run. */
export async function getCleanupCards(): Promise<CandidateGroup[]> {
  if (!inDesktopApp()) return [];
  try {
    return await invoke<CandidateGroup[]>("get_cleanup_cards");
  } catch {
    return [];
  }
}

/** Build a Trash-only cleanup plan from card candidate ids. The
    returned plan executes through `executeCleanupPlan` like any
    other plan. */
export async function buildCardCleanupPlan(ids: string[]): Promise<CleanupPlan> {
  requireDesktop();
  return invoke<CleanupPlan>("build_card_cleanup_plan", { candidateIds: ids });
}

export async function executeCleanupPlan(
  planId: string,
  mode: CleanupMode,
): Promise<CleanupResult> {
  requireDesktop();
  return invoke<CleanupResult>("execute_cleanup_plan", { planId, mode });
}

/* ---------- History, duplicates, docker, dev tools, apps ---------- */

export async function getHistory(): Promise<HistoryEntry[]> {
  if (!inDesktopApp()) return [];
  try {
    return await invoke<HistoryEntry[]>("get_history");
  } catch {
    return [];
  }
}

/**
 * Undo one cleanup run: restore its Recycle Bin items to their
 * original locations. The shell reports what came back, what
 * failed (and why), and how many of the run's items were
 * permanently deleted and so cannot be restored at all.
 */
export async function restoreCleanupRun(entryId: string): Promise<RestoreCleanupRunResult> {
  requireDesktop();
  return invoke<RestoreCleanupRunResult>("restore_cleanup_run", { entryId });
}

export async function findDuplicates(): Promise<DuplicateGroup[]> {
  if (!inDesktopApp()) return [];
  try {
    return await invoke<DuplicateGroup[]>("find_duplicates");
  } catch {
    return [];
  }
}

/**
 * Move the non-keeper copies of one duplicate group to the Recycle Bin.
 * The shell re-verifies each copy byte-for-byte against the keeper
 * immediately before moving it; the keeper itself is always refused.
 */
export async function removeDuplicateCopies(
  groupHash: string,
  keeperPath: string,
  removePaths: string[],
): Promise<CleanupResult> {
  requireDesktop();
  return invoke<CleanupResult>("remove_duplicate_copies", {
    groupHash,
    keeperPath,
    removePaths,
  });
}

export async function getDockerOverview(): Promise<DockerOverview | null> {
  if (!inDesktopApp()) return null;
  try {
    return await invoke<DockerOverview>("get_docker_overview");
  } catch {
    return null;
  }
}

export async function dockerPreviewAction(
  action: DockerActionRequest,
): Promise<DockerActionPreview> {
  requireDesktop();
  return invoke<DockerActionPreview>("docker_preview_action", { action });
}

export async function dockerExecuteAction(
  action: DockerActionRequest,
): Promise<DockerActionResult> {
  requireDesktop();
  return invoke<DockerActionResult>("docker_execute_action", { action });
}

export async function getDevtoolsReports(): Promise<ToolReport[]> {
  if (!inDesktopApp()) return [];
  try {
    return await invoke<ToolReport[]>("get_devtools_reports");
  } catch {
    return [];
  }
}

export async function getApplications(): Promise<AppEntry[]> {
  if (!inDesktopApp()) return [];
  try {
    return await invoke<AppEntry[]>("get_applications");
  } catch {
    return [];
  }
}

/* ---------- Downloads & provenance ---------- */

/** The newest files in the Downloads folder (at most 25), each with
    its recorded download source when Windows stored one. */
export async function getRecentDownloads(): Promise<RecentFile[]> {
  if (!inDesktopApp()) return [];
  try {
    return await invoke<RecentFile[]>("get_recent_downloads");
  } catch {
    return [];
  }
}

/** Downloads grouped by their recorded source. Empty on platforms
    without zone data — provenance is never guessed. */
export async function getDownloadProvenance(): Promise<ProvenanceGroup[]> {
  if (!inDesktopApp()) return [];
  try {
    return await invoke<ProvenanceGroup[]>("get_download_provenance");
  } catch {
    return [];
  }
}

/** Media totals for the current scan (photos, videos, screenshots,
    and the biggest files of each kind); null until a scan has run. */
export async function getMediaSummary(): Promise<MediaSummary | null> {
  if (!inDesktopApp()) return null;
  try {
    return await invoke<MediaSummary | null>("get_media_summary");
  } catch {
    return null;
  }
}

/** Installed Steam and Epic games with their disk usage, read from
    the launchers' manifest files at call time — no scan needed. A
    machine with neither launcher gets an empty report, not an error. */
export async function getGames(): Promise<GamesReport> {
  if (!inDesktopApp()) return { libraries: [], totalBytes: 0, totalGames: 0 };
  try {
    return await invoke<GamesReport>("get_games");
  } catch {
    return { libraries: [], totalBytes: 0, totalGames: 0 };
  }
}

/** EXIF metadata for one image file; null when none is readable. */
export async function getMediaInfo(path: string): Promise<MediaInfo | null> {
  if (!inDesktopApp()) return null;
  try {
    return await invoke<MediaInfo | null>("get_media_info", { path });
  } catch {
    return null;
  }
}

/* ---------- Natural-language search ---------- */

/**
 * Parse a natural-language query and run it over the current scan
 * index. Throws the shell's error string (e.g. no scan yet) so the
 * page can show it — unlike the list loaders, a failed search must
 * not look like an empty result.
 */
export async function searchFiles(query: string): Promise<SearchResponse> {
  requireDesktop();
  return invoke<SearchResponse>("search_files", { query });
}

/** Re-run a search from filters the client adjusted (a chip removed). */
export async function runSearchFilters(filters: SearchFilters): Promise<SearchHitsResponse> {
  requireDesktop();
  return invoke<SearchHitsResponse>("run_search_filters", { filters });
}

/** Look-alike photo groups under the scan root (perceptual hashing
    in the core). Read-only data: similar images are review-only. */
export async function getSimilarImages(): Promise<SimilarGroup[]> {
  if (!inDesktopApp()) return [];
  try {
    return await invoke<SimilarGroup[]>("get_similar_images");
  } catch {
    return [];
  }
}

/* ---------- Recycle Bin ---------- */

export async function getRecycleBinItems(): Promise<RecycleBinItem[]> {
  if (!inDesktopApp()) return [];
  try {
    return await invoke<RecycleBinItem[]>("get_recycle_bin_items");
  } catch {
    return [];
  }
}

export async function emptyRecycleBin(items: RecycleBinItem[]): Promise<CleanupResult> {
  requireDesktop();
  return invoke<CleanupResult>("empty_recycle_bin", { items });
}

/** Put one Recycle Bin item back at its original location. */
export async function restoreRecycleItem(id: string): Promise<void> {
  requireDesktop();
  await invoke<void>("restore_recycle_item", { id });
}

export async function cancelEmptyRecycleBin(): Promise<void> {
  if (!inDesktopApp()) return;
  try {
    await invoke<void>("cancel_empty_recycle_bin");
  } catch {
    /* no empty operation running */
  }
}

export async function onRecycleBinProgress(
  handler: (progress: EmptyRecycleBinProgress) => void,
): Promise<() => void> {
  if (!inDesktopApp()) return () => undefined;
  const { listen } = await import("@tauri-apps/api/event");
  const unlisten = await listen<EmptyRecycleBinProgress>("recycle-bin://progress", (event) =>
    handler(event.payload),
  );
  return unlisten;
}

/* ---------- Secrets scan ---------- */

export async function scanTranscriptSecrets(): Promise<SecretFinding[]> {
  if (!inDesktopApp()) return [];
  try {
    return await invoke<SecretFinding[]>("scan_transcript_secrets");
  } catch {
    return [];
  }
}

/* ---------- Assistant ---------- */

export async function assistantAsk(
  message: string,
  history: ChatMessage[],
): Promise<AssistantReply> {
  requireDesktop();
  return invoke<AssistantReply>("assistant_ask", { message, history });
}

export async function saveApiKey(provider: string, key: string): Promise<void> {
  requireDesktop();
  await invoke<void>("save_api_key", { provider, key });
}

export async function hasApiKey(provider: string): Promise<boolean> {
  if (!inDesktopApp()) return false;
  try {
    return await invoke<boolean>("has_api_key", { provider });
  } catch {
    return false;
  }
}

export async function deleteApiKey(provider: string): Promise<void> {
  requireDesktop();
  await invoke<void>("delete_api_key", { provider });
}

/* ---------- Settings ---------- */

export async function getSettings(): Promise<AppSettings | null> {
  if (!inDesktopApp()) return null;
  try {
    return await invoke<AppSettings | null>("get_settings");
  } catch {
    return null;
  }
}

export async function saveSettings(settings: AppSettings): Promise<void> {
  if (!inDesktopApp()) return;
  await invoke<void>("save_settings", { settings });
}

/** Whether launch-at-sign-in is currently registered. */
export async function getAutostart(): Promise<boolean> {
  if (!inDesktopApp()) return false;
  try {
    return await invoke<boolean>("get_autostart");
  } catch {
    return false;
  }
}

/**
 * Register or unregister launch-at-sign-in. Resolves with the state
 * now in effect — off Windows there is no registration to write, so
 * it resolves `false` there and the toggle stays off.
 */
export async function setAutostart(enabled: boolean): Promise<boolean> {
  if (!inDesktopApp()) return false;
  try {
    return await invoke<boolean>("set_autostart", { enabled });
  } catch {
    return false;
  }
}

/* ---------- Cloud storage & reports ---------- */

/** The cloud sync folder found on this PC (e.g. OneDrive) and how
    much of it is stored locally; null when none is present. */
export async function getCloudStatus(): Promise<CloudStatus | null> {
  if (!inDesktopApp()) return null;
  try {
    return await invoke<CloudStatus | null>("get_cloud_status");
  } catch {
    return null;
  }
}

/**
 * Mark files or folders online-only with the cloud provider. The
 * files stay in the cloud and re-download when opened — nothing is
 * deleted.
 */
export async function freeUpSpace(paths: string[]): Promise<FreeUpResult> {
  requireDesktop();
  return invoke<FreeUpResult>("free_up_space", { paths });
}

/**
 * Save a standalone HTML report through the shell's save dialog.
 * Resolves with the saved path, or null when the user cancelled.
 */
export async function exportReport(html: string): Promise<string | null> {
  if (!inDesktopApp()) return null;
  try {
    return await invoke<string | null>("export_report", { html });
  } catch {
    return null;
  }
}

/* ---------- Shell integration ---------- */

/** Reveal a file or folder in Windows Explorer (selects the item). */
export async function revealInExplorer(path: string): Promise<void> {
  requireDesktop();
  await invoke<void>("reveal_in_explorer", { path });
}

export async function openExternal(url: string): Promise<void> {
  if (!inDesktopApp()) {
    window.open(url, "_blank", "noopener,noreferrer");
    return;
  }
  const { openUrl } = await import("@tauri-apps/plugin-opener");
  await openUrl(url);
}

/** Let the user pick a scan root folder via the native dialog. */
export async function pickFolder(defaultPath?: string): Promise<string | null> {
  if (!inDesktopApp()) return null;
  const { open } = await import("@tauri-apps/plugin-dialog");
  const selected = await open({
    directory: true,
    multiple: false,
    defaultPath,
    title: "Choose a folder to scan",
  });
  return typeof selected === "string" ? selected : null;
}

/* ---------- Scan events ---------- */

type Unlisten = () => void;

async function listenEvent<T>(name: string, handler: (payload: T) => void): Promise<Unlisten> {
  if (!inDesktopApp()) return () => undefined;
  const { listen } = await import("@tauri-apps/api/event");
  const unlisten = await listen<T>(name, (event) => handler(event.payload));
  return unlisten;
}

export function onScanProgress(handler: (p: ScanProgress) => void): Promise<Unlisten> {
  return listenEvent<ScanProgress>("scan://progress", handler);
}

export function onScanFinished(handler: (s: ScanSummary) => void): Promise<Unlisten> {
  return listenEvent<ScanSummary>("scan://finished", handler);
}

/** Tray menu navigation: payload is a page id or a special action string. */
export function onTrayNavigate(
  handler: (target: PageId | "emptyRecycleBin") => void,
): Promise<Unlisten> {
  return listenEvent<PageId | "emptyRecycleBin">("tray://navigate", handler);
}
