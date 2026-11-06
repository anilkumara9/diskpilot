import { create } from "zustand";
import type {
  AppSettings,
  AssistantMessage,
  CandidateGroup,
  DiskInfo,
  DriveHealth,
  DuplicatesMode,
  EnvironmentInfo,
  PageId,
  Recommendation,
  ScanComparison,
  ScanProgress,
  ScanSummary,
} from "./types";
import * as api from "./api";
import { applyTheme } from "./theme";
import {
  readSoftwareUpdatesBadge,
  writeSoftwareUpdatesBadge,
  type SoftwareUpdatesBadge,
} from "./updates";
import {
  checkForAvailableUpdate,
  downloadAndInstallPending,
  IDLE_UPDATE_STATE,
  restartToApplyUpdate,
  type UpdateState,
} from "./updater";

const DEFAULT_SETTINGS: AppSettings = {
  scanRoot: "",
  theme: "system",
  assistantEnabled: false,
  assistantProvider: "anthropic",
  exclusions: [],
  savedSearches: [],
  autoScan: "off",
  lowSpaceAlertGb: 10,
  growthAlertGb: 5,
  autoStart: false,
  onboarded: false,
  ocrEnabled: true,
  liveIndex: true,
  autoMaintenance: { enabled: false, frequency: "weekly", minAgeDays: 30 },
  lastMaintenanceMs: null,
  sidebarCollapsed: false,
};

/**
 * The shell's settings struct does not (yet) carry
 * `sidebarCollapsed`, so a save through the shell would drop the
 * field. Until it does, the preference is mirrored to
 * localStorage and the mirror wins at startup — the setting
 * rides the normal settings flow in the app and genuinely
 * persists across restarts.
 */
const SIDEBAR_COLLAPSED_KEY = "diskpilot.sidebarCollapsed.v1";

function readSidebarCollapsedMirror(): boolean | null {
  try {
    const value = window.localStorage.getItem(SIDEBAR_COLLAPSED_KEY);
    return value === null ? null : value === "1";
  } catch {
    return null;
  }
}

function writeSidebarCollapsedMirror(collapsed: boolean): void {
  try {
    window.localStorage.setItem(SIDEBAR_COLLAPSED_KEY, collapsed ? "1" : "0");
  } catch {
    /* the in-session setting still applies */
  }
}

export type StatusBannerState = {
  kind: "error" | "warning" | "info" | "success";
  message: string;
} | null;

interface AppState {
  page: PageId;
  inDesktop: boolean;
  /** True once `init` has settled (successfully or not). The
      first-run welcome waits for this so it can never flash on
      screen before the saved settings are known. */
  ready: boolean;
  /** True when the shell actually returned a settings object at
      startup. If settings failed to load, the welcome stays hidden
      — the user is never trapped behind it. */
  settingsLoaded: boolean;
  env: EnvironmentInfo | null;
  disk: DiskInfo | null;
  settings: AppSettings;
  scanning: boolean;
  scanError: string | null;
  progress: ScanProgress | null;
  summary: ScanSummary | null;
  comparison: ScanComparison | null;
  recommendations: Recommendation[];
  cleanupCards: CandidateGroup[];
  banner: StatusBannerState;
  /** Message from the most recent failed settings save, cleared by
      the next successful one. Settings renders it inline: a change
      the shell could not persist must never look saved. */
  settingsSaveError: string | null;
  recycleBinOpen: boolean;
  highlightedRecommendationIds: string[];
  /** The assistant conversation. In the store (not page-local) so
      navigating away and back keeps the thread — the page unmounts
      on navigation, the conversation must not. */
  assistantMessages: AssistantMessage[];
  /** Which Duplicates view shows. In the store (not page-local) so
      other pages can deep-link — Media sends readers to the
      look-alike photos view. */
  duplicatesMode: DuplicatesMode;
  /** Signed-update flow state (Settings → About & updates, plus the
      sidebar flag). The launch check is silent unless an update
      exists — see `checkForUpdate`. */
  update: UpdateState;
  /** Physical drives' SMART reports, fetched once at startup for
      the sidebar health dot. Ambient chrome only: a failed read
      leaves this null and no UI complains. The Health page keeps
      its own full read with proper error states. */
  driveHealth: DriveHealth[] | null;
  /** The last completed software-update check (count + when),
      remembered across restarts for the Applications nav badge.
      Written only by the Applications page's own check/update
      flow — nothing refreshes it in the background. */
  softwareUpdatesBadge: SoftwareUpdatesBadge | null;

  init: () => Promise<void>;
  checkForUpdate: (manual: boolean) => Promise<void>;
  installUpdate: () => Promise<void>;
  restartToApply: () => Promise<void>;
  navigate: (page: PageId) => void;
  startScan: (root?: string) => Promise<void>;
  cancelScan: () => Promise<void>;
  refreshRecommendations: () => Promise<void>;
  refreshComparison: () => Promise<void>;
  refreshDisk: () => Promise<void>;
  refreshDriveHealth: () => Promise<void>;
  setSoftwareUpdatesBadge: (badge: SoftwareUpdatesBadge | null) => void;
  /** Applies a settings patch optimistically and persists it.
      Resolves `true` when the shell saved it; on failure resolves
      `false` and records `settingsSaveError` so the caller can
      surface it instead of proceeding as if the change stuck. */
  updateSettings: (patch: Partial<AppSettings>) => Promise<boolean>;
  setBanner: (banner: StatusBannerState) => void;
  setRecycleBinOpen: (open: boolean) => void;
  setHighlighted: (ids: string[]) => void;
  setAssistantMessages: (messages: AssistantMessage[]) => void;
  setDuplicatesMode: (mode: DuplicatesMode) => void;
  handleTrayNavigate: (target: PageId | "emptyRecycleBin") => void;
  applyScanProgress: (p: ScanProgress) => void;
  applyScanFinished: (s: ScanSummary) => void;
}

let listenersBound = false;

export const useAppStore = create<AppState>((set, get) => ({
  page: "overview",
  inDesktop: api.inDesktopApp(),
  ready: false,
  settingsLoaded: false,
  env: null,
  disk: null,
  settings: DEFAULT_SETTINGS,
  scanning: false,
  scanError: null,
  progress: null,
  summary: null,
  comparison: null,
  recommendations: [],
  cleanupCards: [],
  banner: null,
  settingsSaveError: null,
  recycleBinOpen: false,
  highlightedRecommendationIds: [],
  assistantMessages: [],
  duplicatesMode: "exact",
  update: IDLE_UPDATE_STATE,
  driveHealth: null,
  softwareUpdatesBadge: readSoftwareUpdatesBadge(),

  init: async () => {
    const inDesktop = api.inDesktopApp();
    set({ inDesktop });
    applyTheme(get().settings.theme);
    try {
      if (!inDesktop) return;

      const [env, savedSettings] = await Promise.all([api.getEnvironment(), api.getSettings()]);
      const settings: AppSettings = {
        ...DEFAULT_SETTINGS,
        ...(savedSettings ?? {}),
        scanRoot:
          savedSettings?.scanRoot || env?.homeDir || "",
      };
      /* The shell does not return `sidebarCollapsed` (its struct
         predates the field); the local mirror is the durable
         copy and wins when present. */
      const collapsedMirror = readSidebarCollapsedMirror();
      if (collapsedMirror !== null) settings.sidebarCollapsed = collapsedMirror;
      set({ env, settings, settingsLoaded: savedSettings !== null });
      applyTheme(settings.theme);

      const [disk, summary, recommendations, cleanupCards, comparison] = await Promise.all([
        api.getDiskInfo(settings.scanRoot || undefined),
        api.getScanSummary(),
        api.getRecommendations(),
        api.getCleanupCards(),
        api.getScanComparison(),
      ]);
      set({ disk, summary, recommendations, cleanupCards, comparison });

      // Ambient sidebar health dot: one SMART read per launch,
      // silent on failure (see refreshDriveHealth).
      void get().refreshDriveHealth();

      if (!listenersBound) {
        listenersBound = true;
        void api.onScanProgress((p) => get().applyScanProgress(p));
        void api.onScanFinished((s) => get().applyScanFinished(s));
        void api.onTrayNavigate((target) => get().handleTrayNavigate(target));
      }

      // One quiet update check per launch. It only ever surfaces
      // something when a signed update actually exists; every
      // failure (offline, no release yet) stays silent by design.
      void get().checkForUpdate(false);
    } catch {
      /* A failed startup leaves the defaults in place and still
         settles: the app renders normally and the welcome (which
         requires settingsLoaded) stays out of the way. */
    } finally {
      set({ ready: true });
    }
  },

  checkForUpdate: async (manual) => {
    const current = get().update.status;
    if (current === "checking" || current === "downloading" || current === "readyToRestart") return;
    if (!get().inDesktop) {
      if (manual) set({ update: { status: "error" } });
      return;
    }
    set({ update: { status: "checking" } });
    try {
      const found = await checkForAvailableUpdate();
      if (found) {
        set({ update: { status: "available", version: found.version, notes: found.notes } });
      } else {
        // The automatic check returns to idle so it leaves no trace;
        // a manual check gets the explicit "latest version" answer.
        set({ update: manual ? { status: "upToDate" } : IDLE_UPDATE_STATE });
      }
    } catch {
      set({ update: manual ? { status: "error" } : IDLE_UPDATE_STATE });
    }
  },

  installUpdate: async () => {
    const current = get().update;
    if (current.status !== "available") return;
    const { version } = current;
    set({ update: { status: "downloading", version, downloaded: 0, total: null } });
    try {
      await downloadAndInstallPending((downloaded, total) => {
        if (get().update.status === "downloading") {
          set({ update: { status: "downloading", version, downloaded, total } });
        }
      });
      set({ update: { status: "readyToRestart", version } });
    } catch {
      set({ update: { status: "error" } });
    }
  },

  restartToApply: async () => {
    try {
      await restartToApplyUpdate();
    } catch {
      /* If the restart itself fails the installed update still
         applies on the next manual launch — nothing to surface. */
    }
  },

  navigate: (page) => set({ page, banner: null }),

  startScan: async (root) => {
    const target = root ?? get().settings.scanRoot;
    if (!target) {
      set({ scanError: "Choose a folder to scan in Settings first." });
      return;
    }
    set({ scanning: true, scanError: null, progress: null });
    try {
      await api.startScan(target);
    } catch (err) {
      set({
        scanning: false,
        scanError: err instanceof Error ? err.message : "The scan could not be started.",
      });
    }
  },

  cancelScan: async () => {
    await api.cancelScan();
    set({ scanning: false });
  },

  refreshRecommendations: async () => {
    // Cards derive from the same stored scan as the recommendations,
    // so the two always refresh together. A failed background refresh
    // keeps the last good data; the next scan or page load retries.
    try {
      const [recommendations, cleanupCards] = await Promise.all([
        api.getRecommendations(),
        api.getCleanupCards(),
      ]);
      set({ recommendations, cleanupCards });
    } catch {
      /* keep the previous recommendations */
    }
  },

  refreshComparison: async () => {
    try {
      const comparison = await api.getScanComparison();
      set({ comparison });
    } catch {
      /* keep the previous comparison */
    }
  },

  refreshDisk: async () => {
    try {
      const disk = await api.getDiskInfo(get().settings.scanRoot || undefined);
      if (disk) set({ disk });
    } catch {
      /* keep the previous disk figures */
    }
  },

  refreshDriveHealth: async () => {
    try {
      const driveHealth = await api.getDriveHealth();
      set({ driveHealth });
    } catch {
      /* Ambient chrome: keep whatever the last read produced
         (usually null → no dot) and never surface an error. */
    }
  },

  setSoftwareUpdatesBadge: (badge) => {
    writeSoftwareUpdatesBadge(badge);
    set({ softwareUpdatesBadge: badge });
  },

  updateSettings: async (patch) => {
    if (patch.sidebarCollapsed !== undefined) {
      writeSidebarCollapsedMirror(patch.sidebarCollapsed);
    }
    const next = { ...get().settings, ...patch };
    set({ settings: next });
    if (patch.theme) applyTheme(next.theme);
    try {
      await api.saveSettings(next);
      set({ settingsSaveError: null });
      return true;
    } catch (err) {
      set({
        settingsSaveError:
          err instanceof Error ? err.message : "Settings could not be saved.",
      });
      return false;
    }
  },

  setBanner: (banner) => set({ banner }),
  setRecycleBinOpen: (open) => set({ recycleBinOpen: open }),
  setHighlighted: (ids) => set({ highlightedRecommendationIds: ids }),
  setAssistantMessages: (messages) => set({ assistantMessages: messages }),
  setDuplicatesMode: (mode) => set({ duplicatesMode: mode }),

  handleTrayNavigate: (target) => {
    if (target === "emptyRecycleBin") {
      set({ recycleBinOpen: true });
      return;
    }
    set({ page: target });
  },

  applyScanProgress: (p) => set({ progress: p, scanning: true }),
  applyScanFinished: (s) => {
    set({ summary: s, progress: null, scanning: false, scanError: null });
    void get().refreshRecommendations();
    void get().refreshDisk();
    void get().refreshComparison();
  },
}));

export function scanRootOf(state: AppState): string {
  return state.settings.scanRoot || state.env?.homeDir || "";
}

export function homeDirOf(state: AppState): string {
  return state.env?.homeDir ?? state.settings.scanRoot ?? "";
}
