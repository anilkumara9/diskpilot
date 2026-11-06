import { create } from "zustand";
import type {
  AppSettings,
  DiskInfo,
  EnvironmentInfo,
  PageId,
  Recommendation,
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
};

export type StatusBannerState = { kind: "error" | "info" | "success"; message: string } | null;

interface AppState {
  page: PageId;
  inDesktop: boolean;
  env: EnvironmentInfo | null;
  disk: DiskInfo | null;
  settings: AppSettings;
  scanning: boolean;
  scanError: string | null;
  progress: ScanProgress | null;
  summary: ScanSummary | null;
  recommendations: Recommendation[];
  banner: StatusBannerState;
  recycleBinOpen: boolean;
  highlightedRecommendationIds: string[];

  init: () => Promise<void>;
  navigate: (page: PageId) => void;
  startScan: (root?: string) => Promise<void>;
  cancelScan: () => Promise<void>;
  refreshRecommendations: () => Promise<void>;
  refreshDisk: () => Promise<void>;
  updateSettings: (patch: Partial<AppSettings>) => Promise<void>;
  setBanner: (banner: StatusBannerState) => void;
  setRecycleBinOpen: (open: boolean) => void;
  setHighlighted: (ids: string[]) => void;
  handleTrayNavigate: (target: PageId | "emptyRecycleBin") => void;
  applyScanProgress: (p: ScanProgress) => void;
  applyScanFinished: (s: ScanSummary) => void;
}

let listenersBound = false;

export const useAppStore = create<AppState>((set, get) => ({
  page: "overview",
  inDesktop: api.inDesktopApp(),
  env: null,
  disk: null,
  settings: DEFAULT_SETTINGS,
  scanning: false,
  scanError: null,
  progress: null,
  summary: null,
  recommendations: [],
  banner: null,
  recycleBinOpen: false,
  highlightedRecommendationIds: [],

  init: async () => {
    const inDesktop = api.inDesktopApp();
    set({ inDesktop });
    applyTheme(get().settings.theme);
    if (!inDesktop) return;

    const [env, savedSettings] = await Promise.all([api.getEnvironment(), api.getSettings()]);
    const settings: AppSettings = {
      ...DEFAULT_SETTINGS,
      ...(savedSettings ?? {}),
      scanRoot:
        savedSettings?.scanRoot || env?.homeDir || "",
    };
    set({ env, settings });
    applyTheme(settings.theme);

    const [disk, summary, recommendations] = await Promise.all([
      api.getDiskInfo(settings.scanRoot || undefined),
      api.getScanSummary(),
      api.getRecommendations(),
    ]);
    set({ disk, summary, recommendations });

    if (!listenersBound) {
      listenersBound = true;
      void api.onScanProgress((p) => get().applyScanProgress(p));
      void api.onScanFinished((s) => get().applyScanFinished(s));
      void api.onTrayNavigate((target) => get().handleTrayNavigate(target));
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
    const recommendations = await api.getRecommendations();
    set({ recommendations });
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
  },
}));

export function scanRootOf(state: AppState): string {
  return state.settings.scanRoot || state.env?.homeDir || "";
}

export function homeDirOf(state: AppState): string {
  return state.env?.homeDir ?? state.settings.scanRoot ?? "";
}
