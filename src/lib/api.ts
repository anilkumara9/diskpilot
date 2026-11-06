import type {
  AppEntry,
  AppJunkGroup,
  AppSettings,
  AssistantReply,
  AvailableUpdates,
  BiggestChanges,
  CandidateGroup,
  ChatMessage,
  CleanAppJunkResult,
  CleanupMode,
  CleanupPlan,
  CleanupResult,
  CloudStatus,
  CompressionCandidate,
  DirectoryEntry,
  DiskInfo,
  DockerActionRequest,
  DockerActionPreview,
  DockerActionResult,
  DockerOverview,
  DriveHealth,
  DriveInfo,
  DuplicateGroup,
  EmptyRecycleBinProgress,
  EnvironmentInfo,
  ExtensionStat,
  FileEntry,
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
  RelocateResult,
  RestoreCleanupRunResult,
  ScanComparison,
  ScanHistoryEntry,
  ScanProgress,
  ScanSummary,
  SearchFilters,
  SearchHitsResponse,
  SearchResponse,
  SecretFinding,
  SetCompressionResult,
  SoftwareUpdateOutcome,
  StartupItem,
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

/**
 * Read-command contract (error honesty): a wrapper returns the
 * command's own result — including a legitimate `null` where the
 * shell uses one ("no scan yet", "no cloud folder") — and THROWS
 * when the invoke itself fails. A failed command must never
 * masquerade as an empty list or a missing feature on a page.
 * Off-desktop (plain browser) the wrappers keep returning their
 * empty defaults, since there is no shell to fail against.
 *
 * Deliberate exceptions, all fire-and-forget or state probes whose
 * failure has no page to report to: `cancelScan`,
 * `cancelEmptyRecycleBin`, and the autostart probes
 * (`getAutostart`/`setAutostart` resolve the state now in effect;
 * a failed probe reports "off" and is re-read on the next mount).
 */

/* ---------- Environment & disk ---------- */

export async function getEnvironment(): Promise<EnvironmentInfo | null> {
  if (!inDesktopApp()) return null;
  return invoke<EnvironmentInfo | null>("get_environment");
}

export async function getDiskInfo(root?: string): Promise<DiskInfo | null> {
  if (!inDesktopApp()) return null;
  return invoke<DiskInfo | null>("get_disk_info", root ? { path: root } : undefined);
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

/** The latest scan's summary; null when no scan has run yet. */
export async function getScanSummary(): Promise<ScanSummary | null> {
  if (!inDesktopApp()) return null;
  return invoke<ScanSummary | null>("get_scan_summary");
}

/** Current scan vs. the previous scan of the same root; null when
    there is no previous scan or the root changed. */
export async function getScanComparison(): Promise<ScanComparison | null> {
  if (!inDesktopApp()) return null;
  return invoke<ScanComparison | null>("get_scan_comparison");
}

/** Every drive on this PC with its capacity and free space. */
export async function getDrives(): Promise<DriveInfo[]> {
  if (!inDesktopApp()) return [];
  return invoke<DriveInfo[]>("get_drives");
}

/* ---------- Watch: history, forecast, biggest changes ---------- */

/** The persisted scan time series, oldest first. */
export async function getScanHistory(): Promise<ScanHistoryEntry[]> {
  if (!inDesktopApp()) return [];
  return invoke<ScanHistoryEntry[]>("get_scan_history");
}

/** Space forecast for the configured scan root; null when the
    history cannot support a fit yet. */
export async function getSpaceForecast(): Promise<SpaceForecast | null> {
  if (!inDesktopApp()) return null;
  return invoke<SpaceForecast | null>("get_space_forecast");
}

/** Biggest directory changes between the current and previous scan. */
export async function getBiggestChanges(limit: number): Promise<BiggestChanges | null> {
  if (!inDesktopApp()) return null;
  return invoke<BiggestChanges | null>("get_biggest_changes", { limit });
}

export async function listDirectory(path: string): Promise<DirectoryEntry[]> {
  if (!inDesktopApp()) return [];
  return invoke<DirectoryEntry[]>("list_directory", { path });
}

/* ---------- Recommendations & cleanup ---------- */

export async function getRecommendations(): Promise<Recommendation[]> {
  if (!inDesktopApp()) return [];
  return invoke<Recommendation[]>("get_recommendations");
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
  return invoke<CandidateGroup[]>("get_cleanup_cards");
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
  return invoke<HistoryEntry[]>("get_history");
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
  return invoke<DuplicateGroup[]>("find_duplicates");
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

/** Docker overview; null when Docker is not installed — a real
    state the page renders as such, distinct from a failed read. */
export async function getDockerOverview(): Promise<DockerOverview | null> {
  if (!inDesktopApp()) return null;
  return invoke<DockerOverview | null>("get_docker_overview");
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
  return invoke<ToolReport[]>("get_devtools_reports");
}

export async function getApplications(): Promise<AppEntry[]> {
  if (!inDesktopApp()) return [];
  return invoke<AppEntry[]>("get_applications");
}

/* ---------- Downloads & provenance ---------- */

/** The newest files in the Downloads folder (at most 25), each with
    its recorded download source when Windows stored one. */
export async function getRecentDownloads(): Promise<RecentFile[]> {
  if (!inDesktopApp()) return [];
  return invoke<RecentFile[]>("get_recent_downloads");
}

/** Downloads grouped by their recorded source. Empty on platforms
    without zone data — provenance is never guessed. */
export async function getDownloadProvenance(): Promise<ProvenanceGroup[]> {
  if (!inDesktopApp()) return [];
  return invoke<ProvenanceGroup[]>("get_download_provenance");
}

/** Media totals for the current scan (photos, videos, screenshots,
    and the biggest files of each kind); null until a scan has run. */
export async function getMediaSummary(): Promise<MediaSummary | null> {
  if (!inDesktopApp()) return null;
  return invoke<MediaSummary | null>("get_media_summary");
}

/** Installed Steam and Epic games with their disk usage, read from
    the launchers' manifest files at call time — no scan needed. A
    machine with neither launcher gets an empty report, not an error. */
export async function getGames(): Promise<GamesReport> {
  if (!inDesktopApp()) return { libraries: [], totalBytes: 0, totalGames: 0 };
  return invoke<GamesReport>("get_games");
}

/** EXIF metadata for one image file; null when none is readable. */
export async function getMediaInfo(path: string): Promise<MediaInfo | null> {
  if (!inDesktopApp()) return null;
  return invoke<MediaInfo | null>("get_media_info", { path });
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
  return invoke<SimilarGroup[]>("get_similar_images");
}

/* ---------- Recycle Bin ---------- */

export async function getRecycleBinItems(): Promise<RecycleBinItem[]> {
  if (!inDesktopApp()) return [];
  return invoke<RecycleBinItem[]>("get_recycle_bin_items");
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
  return invoke<SecretFinding[]>("scan_transcript_secrets");
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

/** Whether a key is stored for the provider. Throws when the
    credential store itself cannot be read, so Settings never
    describes one provider's key state using another's answer. */
export async function hasApiKey(provider: string): Promise<boolean> {
  if (!inDesktopApp()) return false;
  return invoke<boolean>("has_api_key", { provider });
}

export async function deleteApiKey(provider: string): Promise<void> {
  requireDesktop();
  await invoke<void>("delete_api_key", { provider });
}

/* ---------- Settings ---------- */

export async function getSettings(): Promise<AppSettings | null> {
  if (!inDesktopApp()) return null;
  return invoke<AppSettings | null>("get_settings");
}

export async function saveSettings(settings: AppSettings): Promise<void> {
  if (!inDesktopApp()) return;
  await invoke<void>("save_settings", { settings });
}

/* ---------- Startup items ---------- */

/** Every program registered to open at sign-in (registry Run keys
    and Startup folders, per-user and machine-wide). Read fresh at
    call time — no scan involved. Throws on failure: a page must
    never render a failed read as "nothing starts with Windows". */
export async function getStartupItems(): Promise<StartupItem[]> {
  if (!inDesktopApp()) return [];
  return invoke<StartupItem[]>("get_startup_items");
}

/**
 * Enable or disable one sign-in entry. Disabling is reversible —
 * the shell keeps the entry aside and restores it exactly when
 * re-enabled. Throws with the shell's human message on failure
 * (machine-wide entries need DiskPilot running as administrator).
 */
export async function setStartupItemEnabled(id: string, enabled: boolean): Promise<void> {
  requireDesktop();
  await invoke<void>("set_startup_item_enabled", { id, enabled });
}

/* ---------- Compression ---------- */

/** Large, long-untouched folders from the current scan that
    Windows could compress in place. Already-compressed folders
    are filtered out by the shell. Empty until a scan has run. */
export async function getCompressionCandidates(): Promise<CompressionCandidate[]> {
  if (!inDesktopApp()) return [];
  return invoke<CompressionCandidate[]>("get_compression_candidates");
}

/**
 * Compress or decompress one folder in place (NTFS compression):
 * nothing is deleted and the folder keeps working. The shell
 * reports the outcome in the result — `ok: false` with the reason
 * in `error` — rather than throwing, so the row can show it.
 */
export async function setFolderCompression(
  path: string,
  compress: boolean,
): Promise<SetCompressionResult> {
  requireDesktop();
  return invoke<SetCompressionResult>("set_folder_compression", { path, compress });
}

/* ---------- Extension statistics ---------- */

/** Allocated bytes and counts per file extension in the current
    scan, largest first. Empty until a scan has run. */
export async function getExtensionStats(): Promise<ExtensionStat[]> {
  if (!inDesktopApp()) return [];
  return invoke<ExtensionStat[]>("get_extension_stats");
}

/** The largest files carrying one extension (the bare extension,
    "" for files with none), in the same row shape as the scan's
    largest-files list. */
export async function getExtensionFiles(extension: string): Promise<FileEntry[]> {
  if (!inDesktopApp()) return [];
  return invoke<FileEntry[]>("get_extension_files", { extension });
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
  return invoke<CloudStatus | null>("get_cloud_status");
}

/**
 * Mark files or folders online-only with the cloud provider. The
 * files stay in the cloud and re-download when opened — nothing is
 * deleted. The space is released by the provider as it syncs, not
 * at the moment of marking.
 */
export async function freeUpSpace(paths: string[]): Promise<FreeUpResult> {
  requireDesktop();
  return invoke<FreeUpResult>("free_up_space", { paths });
}

/**
 * Save a standalone HTML report through the shell's save dialog.
 * Resolves with the saved path, or null when the user cancelled —
 * a failed save THROWS, so a cancellation stays silent while a
 * real failure can be reported.
 */
export async function exportReport(html: string): Promise<string | null> {
  if (!inDesktopApp()) return null;
  return invoke<string | null>("export_report", { html });
}

/**
 * Save a CSV export of the current scan (its files, or its
 * category totals) through the shell's save dialog. Same contract
 * as `exportReport`: the saved path, null on cancel, throws on a
 * failed save.
 */
export async function exportScanCsv(kind: "files" | "categories"): Promise<string | null> {
  if (!inDesktopApp()) return null;
  return invoke<string | null>("export_scan_csv", { kind });
}

/* ---------- App junk ---------- */

/** Cache locations of known applications, measured where each app
    keeps them. Throws on failure: a page must never render a
    failed read as "no app caches". */
export async function listAppJunk(): Promise<AppJunkGroup[]> {
  if (!inDesktopApp()) return [];
  return invoke<AppJunkGroup[]>("list_app_junk");
}

/**
 * Move the CONTENTS of the named junk groups' cache locations to
 * the Recycle Bin. The shell re-derives the locations from the
 * group ids — paths are never taken from the client. The result
 * reports each path's outcome; an invoke-level failure throws.
 */
export async function cleanAppJunk(groupIds: string[]): Promise<CleanAppJunkResult> {
  requireDesktop();
  return invoke<CleanAppJunkResult>("clean_app_junk", { groupIds });
}

/* ---------- Drive health (SMART) ---------- */

/** Each physical drive's SMART self-report. An empty list is a
    real answer — the system reports no drive health — distinct
    from a failed read, which throws. */
export async function getDriveHealth(): Promise<DriveHealth[]> {
  if (!inDesktopApp()) return [];
  return invoke<DriveHealth[]>("get_drive_health");
}

/* ---------- Software updates (winget) ---------- */

/** Outdated applications according to the Windows Package Manager.
    `available: false` in the result means winget itself is not
    available on this PC. Throws when the check itself fails. */
export async function listAvailableUpdates(): Promise<AvailableUpdates> {
  if (!inDesktopApp()) return { available: false, updates: [], error: null };
  return invoke<AvailableUpdates>("list_available_updates");
}

/**
 * Update the named applications via winget; an empty id list
 * updates everything outstanding. Resolves with one outcome per
 * attempted application — a failed update is an outcome with
 * `success: false`, not a throw.
 */
export async function updateSoftware(ids: string[]): Promise<SoftwareUpdateOutcome[]> {
  requireDesktop();
  return invoke<SoftwareUpdateOutcome[]>("update_software", { ids });
}

/* ---------- Move folder (relocation) ---------- */

/**
 * Move a folder's contents into `destDir` on another drive and
 * leave a link at the original location, so apps keep working.
 * Long-running for large folders. Throws with the shell's reason
 * when the move cannot be done safely.
 */
export async function relocateFolder(source: string, destDir: string): Promise<RelocateResult> {
  requireDesktop();
  return invoke<RelocateResult>("relocate_folder", { source, destDir });
}

/** Undo the relocation recorded by a History entry: move the
    contents back and remove the link. */
export async function undoRelocation(entryId: number): Promise<RelocateResult> {
  requireDesktop();
  return invoke<RelocateResult>("undo_relocation", { entryId });
}

/* ---------- Explorer integration & launch intent ---------- */

/** Whether "Analyze with DiskPilot" is registered in the Explorer
    right-click menu. Throws when the registration cannot be read —
    Settings must not describe a guessed state. */
export async function getExplorerIntegration(): Promise<boolean> {
  if (!inDesktopApp()) return false;
  return invoke<boolean>("get_explorer_integration");
}

/** Register or remove the Explorer right-click entry. */
export async function setExplorerIntegration(enabled: boolean): Promise<void> {
  requireDesktop();
  await invoke<void>("set_explorer_integration", { enabled });
}

/**
 * The folder this launch (or an Explorer "Analyze with
 * DiskPilot" click while running) asked to scan; null when the
 * app was launched normally. Reading consumes the intent in the
 * shell, so a path is delivered once per request.
 */
export async function getLaunchIntent(): Promise<string | null> {
  if (!inDesktopApp()) return null;
  return invoke<string | null>("get_launch_intent");
}

/* ---------- Shell integration ---------- */

/** Reveal a file or folder in Windows Explorer (selects the item). */
export async function revealInExplorer(path: string): Promise<void> {
  requireDesktop();
  await invoke<void>("reveal_in_explorer", { path });
}

/**
 * Open an external URL through the shell's `open_external` command —
 * the Rust side is the scheme whitelist (http/https/mailto only),
 * so links rendered from assistant markdown never reach the opener
 * plugin directly.
 */
export async function openExternal(url: string): Promise<void> {
  if (!inDesktopApp()) {
    window.open(url, "_blank", "noopener,noreferrer");
    return;
  }
  await invoke<void>("open_external", { url });
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

/** Explorer "Analyze with DiskPilot" was clicked while the app is
    already running. The payload carries nothing — the handler
    re-reads `getLaunchIntent` for the requested folder. */
export function onLaunchScan(handler: () => void): Promise<Unlisten> {
  return listenEvent<null>("launch-scan", () => handler());
}
