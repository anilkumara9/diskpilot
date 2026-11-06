import { useEffect, useState } from "react";
import { FolderOpen, Gamepad2, Info } from "lucide-react";
import { useAppStore } from "@/lib/store";
import * as api from "@/lib/api";
import { displayPath, formatBytes, formatCount } from "@/lib/format";
import type { GameEntry, GamesReport } from "@/lib/types";
import { PageHeader } from "@/components/layout/PageHeader";
import { Button } from "@/components/ui/button";
import { Section } from "@/components/ui/section";
import { SkeletonRows } from "@/components/ui/skeleton";
import { EmptyState } from "@/components/shared/EmptyState";
import { MoveFolderDialog } from "@/components/shared/MoveFolderDialog";
import { SizeBar } from "@/components/shared/SizeBar";
import { StatStrip } from "@/components/shared/StatStrip";

/* ---------------- Page ---------------- */

export function GamesPage() {
  const homeDir = useAppStore((s) => s.env?.homeDir);
  const finishedAtMs = useAppStore((s) => s.summary?.finishedAtMs);
  const startScan = useAppStore((s) => s.startScan);
  const [report, setReport] = useState<GamesReport | null>(null);
  const [revealError, setRevealError] = useState<string | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [reloadKey, setReloadKey] = useState(0);
  const [movingGame, setMovingGame] = useState<GameEntry | null>(null);

  /* Games come from the launchers' manifests, not from a scan; the
     page reloads when a scan finishes so installs measured by the
     scan and installs read from manifests stay in step. A failed
     read is an error state — NOT the "no libraries" empty report. */
  useEffect(() => {
    let live = true;
    api
      .getGames()
      .then((rows) => {
        if (!live) return;
        setReport(rows);
        setLoadError(null);
      })
      .catch((err: unknown) => {
        if (live) {
          setLoadError(err instanceof Error ? err.message : "Games could not be loaded.");
        }
      });
    return () => {
      live = false;
    };
  }, [finishedAtMs, reloadKey]);

  const reveal = async (path: string) => {
    setRevealError(null);
    try {
      await api.revealInExplorer(path);
    } catch (err) {
      setRevealError(err instanceof Error ? err.message : "Could not open this location in Explorer.");
    }
  };

  return (
    <div className="h-full overflow-y-auto px-6 py-6">
      <div className="max-w-[880px]">
        <PageHeader
          title="Games"
          subtitle="Games installed through Steam and Epic, and the space each one takes — read from the launchers' own records."
          showScanControl={false}
        />
        {revealError ? (
          <p role="alert" className="pb-3 text-[12px] text-critical">{revealError}</p>
        ) : null}

        {loadError ? (
          <EmptyState
            icon={Gamepad2}
            className="min-h-[360px]"
            title="Games couldn't be loaded"
            description={loadError}
            action={
              <Button variant="secondary" onClick={() => setReloadKey((n) => n + 1)}>
                Retry
              </Button>
            }
          />
        ) : report === null ? (
          <SkeletonRows rows={7} />
        ) : report.totalGames === 0 ? (
          <EmptyState
            icon={Gamepad2}
            className="min-h-[360px]"
            title="No game libraries found"
            description="DiskPilot looks for Steam and Epic Games libraries in their standard locations. Games kept elsewhere still count toward your Storage totals when their folder is scanned."
          />
        ) : (
          <>
            <StatStrip
              stats={[
                { label: "Total in games", value: formatBytes(report.totalBytes) },
                { label: "Games", value: formatCount(report.totalGames) },
                { label: "Libraries", value: formatCount(report.libraries.length) },
              ]}
            />

            {report.libraries.map((library) => {
              const max = Math.max(...library.games.map((g) => g.bytes), 1);
              return (
                <Section
                  key={`${library.source}:${library.path}`}
                  title={`${library.source} library`}
                  description={displayPath(library.path, homeDir)}
                  action={
                    <span className="text-[13px] font-semibold tabular text-foreground">
                      {formatBytes(library.bytes)}
                    </span>
                  }
                >
                  <ul className="divide-y divide-border">
                    {library.games.map((game) => (
                      <li key={game.path} className="flex items-center gap-3 py-2">
                        <div className="min-w-0 flex-1">
                          <p className="truncate text-[13px] font-medium text-foreground" title={game.path}>
                            {game.name}
                          </p>
                          <SizeBar fraction={game.bytes / max} height={2} className="mt-1 w-[200px]" />
                        </div>
                        <span className="w-[92px] shrink-0 text-right text-[13px] tabular text-foreground">
                          {formatBytes(game.bytes)}
                        </span>
                        <Button variant="ghost" size="sm" onClick={() => setMovingGame(game)}>
                          Move…
                        </Button>
                        <Button variant="ghost" size="sm" onClick={() => void reveal(game.path)}>
                          <FolderOpen className="size-3.5" aria-hidden="true" />
                          Reveal
                        </Button>
                      </li>
                    ))}
                  </ul>
                </Section>
              );
            })}

            <div className="mt-6 flex gap-3 rounded-[10px] border border-border bg-panel-raised px-4 py-3">
              <Info className="mt-0.5 size-4 shrink-0 text-muted" aria-hidden="true" />
              <p className="text-[12px] leading-4 text-muted">
                Uninstall games from Steam or Epic — DiskPilot shows where the space goes; it
                never removes game files.
              </p>
            </div>
          </>
        )}

        {movingGame ? (
          <MoveFolderDialog
            sourcePath={movingGame.path}
            sourceBytes={movingGame.bytes}
            onClose={() => setMovingGame(null)}
            onMoved={() => {
              /* The manifests are re-read and the scan root is
                 rescanned so both views of the game stay true. */
              setReloadKey((n) => n + 1);
              void startScan();
            }}
          />
        ) : null}
      </div>
    </div>
  );
}
