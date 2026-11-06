import { create } from "zustand";
import type {
  AppSettings,
  CandidateGroup,
  DiskInfo,
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
};

export type StatusBannerState = { kind: "error" | "info" | "success"; message: string } | null;

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
  recycleBinOpen: boolean;
  highlightedRecommendationIds: string[];
  /** Which Duplicates view shows. In the store (not page-local) so
      other pages can deep-link — Media sends readers to the
      look-alike photos view. */
  duplicatesMode: DuplicatesMode;

  init: () => Promise<void>;
  navigate: (page: PageId) => void;
  startScan: (root?: string) => Promise<void>;
  cancelScan: () => Promise<void>;
  refreshRecommendations: () => Promise<void>;
  refreshComparison: () => Promise<void>;
  refreshDisk: () => Promise<void>;
  updateSettings: (patch: Partial<AppSettings>) => Promise<void>;
  setBanner: (banner: StatusBannerState) => void;
  setRecycleBinOpen: (open: boolean) => void;
  setHighlighted: (ids: string[]) => void;
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
  recycleBinOpen: false,
  highlightedRecommendationIds: [],
  duplicatesMode: "exact",

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

      if (!listenersBound) {
        listenersBound = true;
        void api.onScanProgress((p) => get().applyScanProgress(p));
        void api.onScanFinished((s) => get().applyScanFinished(s));
        void api.onTrayNavigate((target) => get().handleTrayNavigate(target));
      }
    } catch {
      /* A failed startup leaves the defaults in place and still
         settles: the app renders normally and the welcome (which
         requires settingsLoaded) stays out of the way. */
    } finally {
      set({ ready: true });
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
    // so the two always refresh together.
    const [recommendations, cleanupCards] = await Promise.all([
      api.getRecommendations(),
      api.getCleanupCards(),
    ]);
    set({ recommendations, cleanupCards });
  },

  refreshComparison: async () => {
    const comparison = await api.getScanComparison();
    set({ comparison });
  },

  refreshDisk: async () => {
    const disk = await api.getDiskInfo(get().settings.scanRoot || undefined);
    if (disk) set({ disk });
  },

  updateSettings: async (patch) => {
    const next = { ...get().settings, ...patch };
    set({ settings: next });
    if (patch.theme) applyTheme(next.theme);
    try {
      await api.saveSettings(next);
    } catch {
      /* settings persist on next successful save */
    }
  },

  setBanner: (banner) => set({ banner }),
  setRecycleBinOpen: (open) => set({ recycleBinOpen: open }),
  setHighlighted: (ids) => set({ highlightedRecommendationIds: ids }),
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
