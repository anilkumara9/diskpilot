import type {
  AppEntry,
  AppSettings,
  AssistantReply,
  ChatMessage,
  CleanupMode,
  CleanupPlan,
  CleanupResult,
  DirectoryEntry,
  DiskInfo,
  DockerActionRequest,
  DockerActionPreview,
  DockerActionResult,
  DockerOverview,
  DuplicateGroup,
  EmptyRecycleBinProgress,
  EnvironmentInfo,
  HistoryEntry,
  PageId,
  Recommendation,
  RecycleBinItem,
  ScanComparison,
  ScanProgress,
  ScanSummary,
  SecretFinding,
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
