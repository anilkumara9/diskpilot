import { lazy, Suspense, useEffect } from "react";
import { AlertTriangle, CheckCircle2, Info, MonitorX, X } from "lucide-react";
import { useAppStore } from "@/lib/store";
import { watchSystemTheme } from "@/lib/theme";
import type { PageId } from "@/lib/types";
import { Sidebar } from "@/components/layout/Sidebar";
import { PageHeader } from "@/components/layout/PageHeader";
import { Button } from "@/components/ui/button";
import { SkeletonRows } from "@/components/ui/skeleton";
import { OverviewPage } from "@/features/overview/OverviewPage";
import { HealthPage } from "@/features/health/HealthPage";
import { StoragePage } from "@/features/storage/StoragePage";
import { MediaPage } from "@/features/media/MediaPage";
import { DuplicatesPage } from "@/features/duplicates/DuplicatesPage";
import { ApplicationsPage } from "@/features/applications/ApplicationsPage";
import { GamesPage } from "@/features/games/GamesPage";
import { DeveloperToolsPage } from "@/features/developer/DeveloperToolsPage";
import { DockerPage } from "@/features/docker/DockerPage";
import { DownloadsPage } from "@/features/downloads/DownloadsPage";
import { RecommendationsPage } from "@/features/recommendations/RecommendationsPage";
import { HistoryPage } from "@/features/history/HistoryPage";
import { SettingsPage } from "@/features/settings/SettingsPage";
import { EmptyRecycleBinDialog } from "@/features/trash/EmptyRecycleBinDialog";

/**
 * The Assistant page is loaded on demand: it carries the markdown
 * renderer, which would otherwise add ~170 KB to the startup bundle
 * for a page many sessions never open.
 */
const AssistantPage = lazy(() =>
  import("@/features/assistant/AssistantPage").then((m) => ({ default: m.AssistantPage })),
);

const PAGE_COMPONENTS: Record<Exclude<PageId, "assistant">, () => React.JSX.Element> = {
  overview: OverviewPage,
  health: HealthPage,
  storage: StoragePage,
  media: MediaPage,
  duplicates: DuplicatesPage,
  applications: ApplicationsPage,
  games: GamesPage,
  developer: DeveloperToolsPage,
  docker: DockerPage,
  downloads: DownloadsPage,
  cleanup: RecommendationsPage,
  history: HistoryPage,
  settings: SettingsPage,
};

/** The Assistant page's own shell, shown while its chunk loads. */
function AssistantLoadingFallback() {
  return (
    <div className="flex h-full flex-col px-6 py-6">
      <div className="flex min-h-0 w-full max-w-[760px] flex-1 flex-col">
        <PageHeader
          title="Assistant"
          subtitle="Ask about this PC's scan results. The assistant is read-only: it can explain and point at recommendations, but it cannot delete, move, or run anything."
          showScanControl={false}
        />
        <SkeletonRows rows={6} />
      </div>
    </div>
  );
}

function StatusBanner() {
  const banner = useAppStore((s) => s.banner);
  const scanError = useAppStore((s) => s.scanError);
  const setBanner = useAppStore((s) => s.setBanner);

  const shown =
    banner ??
    (scanError ? ({ kind: "error", message: scanError } as const) : null);
  if (!shown) return null;

  const Icon =
    shown.kind === "error" ? AlertTriangle : shown.kind === "success" ? CheckCircle2 : Info;

  return (
    <div
      role={shown.kind === "error" ? "alert" : "status"}
      className="flex items-center gap-2.5 border-b border-border bg-panel-raised px-6 py-2"
    >
      <Icon
        className={
          shown.kind === "error"
            ? "size-4 shrink-0 text-critical"
            : shown.kind === "success"
              ? "size-4 shrink-0 text-tier-safe"
              : "size-4 shrink-0 text-accent"
        }
        aria-hidden="true"
      />
      <p className="min-w-0 flex-1 text-[12px] leading-4 text-foreground">{shown.message}</p>
      <Button
        variant="ghost"
        size="icon"
        aria-label="Dismiss"
        onClick={() => {
          setBanner(null);
          useAppStore.setState({ scanError: null });
        }}
      >
        <X className="size-4" />
      </Button>
    </div>
  );
}

/** Shown in a plain web browser: DiskPilot only runs as a desktop app. */
function DesktopOnlyState() {
  return (
    <main className="flex h-full flex-col items-center justify-center bg-background px-6 text-center">
      <img src="/logo.svg" alt="" width={52} height={52} className="rounded-[12px]" aria-hidden="true" />
      <MonitorX className="mt-6 size-8 text-subtle" strokeWidth={1.5} aria-hidden="true" />
      <h1 className="mt-3 text-[20px] font-semibold leading-6 text-foreground">
        DiskPilot runs as a desktop app
      </h1>
      <p className="mt-2 max-w-[440px] text-[13px] leading-[18px] text-muted">
        This page is only the interface. Scanning, classification, and cleanup happen in the native
        Windows app, which needs direct access to your folders — a browser tab cannot do that work.
      </p>
      <p className="mt-4 text-[13px] text-muted">
        Start it in development with{" "}
        <code className="rounded bg-panel-raised px-1.5 py-0.5 font-mono text-[12px] text-foreground">
          pnpm app:dev
        </code>
        , or install the Windows build (MSI or NSIS) from a release.
      </p>
    </main>
  );
}

export default function App() {
  const inDesktop = useAppStore((s) => s.inDesktop);
  const page = useAppStore((s) => s.page);
  const assistantEnabled = useAppStore((s) => s.settings.assistantEnabled);
  const init = useAppStore((s) => s.init);
  const recycleBinOpen = useAppStore((s) => s.recycleBinOpen);
  const setRecycleBinOpen = useAppStore((s) => s.setRecycleBinOpen);

  useEffect(() => {
    void init();
  }, [init]);

  useEffect(() => watchSystemTheme(() => useAppStore.getState().settings.theme), []);

  if (!inDesktop) return <DesktopOnlyState />;

  const effectivePage: PageId = page === "assistant" && !assistantEnabled ? "overview" : page;
  const PageComponent = effectivePage === "assistant" ? null : PAGE_COMPONENTS[effectivePage];

  return (
    <div className="flex h-full bg-background text-foreground">
      <Sidebar />
      <div className="flex min-w-0 flex-1 flex-col">
        {/* 38px drag region above page content */}
        <div data-tauri-drag-region className="h-[38px] w-full shrink-0" aria-hidden="true" />
        <StatusBanner />
        <main className="min-h-0 flex-1 overflow-hidden" key={effectivePage}>
          {PageComponent ? (
            <PageComponent />
          ) : (
            <Suspense fallback={<AssistantLoadingFallback />}>
              <AssistantPage />
            </Suspense>
          )}
        </main>
      </div>
      <EmptyRecycleBinDialog open={recycleBinOpen} onClose={() => setRecycleBinOpen(false)} />
    </div>
  );
}
