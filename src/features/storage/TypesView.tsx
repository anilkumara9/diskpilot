import { useCallback, useEffect, useMemo, useState } from "react";
import { ChevronDown, ChevronRight, FolderOpen, Layers } from "lucide-react";
import * as api from "@/lib/api";
import { useAppStore } from "@/lib/store";
import { extensionLabel, topTypesCoverage } from "@/lib/extensions";
import { displayPath, formatBytes, formatCount, formatPercent } from "@/lib/format";
import type { ExtensionStat, FileEntry } from "@/lib/types";
import { Button } from "@/components/ui/button";
import { SkeletonRows } from "@/components/ui/skeleton";
import { EmptyState } from "@/components/shared/EmptyState";
import { SizeBar } from "@/components/shared/SizeBar";
import { StatStrip } from "@/components/shared/StatStrip";
import { cn } from "@/lib/utils";

/**
 * Types: the scan ranked by file extension. Each row expands in
 * place to that type's largest files (fetched on first expansion
 * and cached for the visit), so "what is eating the disk" and
 * "show me the files" are one gesture apart. Read-only, like the
 * rest of Storage.
 */

const MAX_ROWS = 50;

function TypeFiles({ extension }: { extension: string }) {
  const homeDir = useAppStore((s) => s.env?.homeDir);
  const [files, setFiles] = useState<FileEntry[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [revealError, setRevealError] = useState<string | null>(null);
  const [reloadKey, setReloadKey] = useState(0);

  useEffect(() => {
    let live = true;
    api
      .getExtensionFiles(extension)
      .then((rows) => {
        if (live) {
          /* The view promises largest first; the sort is cheap
             insurance over the shell's order. */
          setFiles([...rows].sort((a, b) => b.bytes - a.bytes));
          setError(null);
        }
      })
      .catch((err: unknown) => {
        if (live) {
          setError(err instanceof Error ? err.message : "These files could not be loaded.");
        }
      });
    return () => {
      live = false;
    };
  }, [extension, reloadKey]);

  const reveal = async (path: string) => {
    setRevealError(null);
    try {
      await api.revealInExplorer(path);
    } catch (err) {
      setRevealError(err instanceof Error ? err.message : "Could not open this location in Explorer.");
    }
  };

  if (error) {
    return (
      <div className="flex items-center gap-3 py-2 pl-[40px] pr-3">
        <p role="alert" className="flex-1 text-[12px] text-critical">{error}</p>
        <Button variant="secondary" size="sm" onClick={() => setReloadKey((n) => n + 1)}>
          Retry
        </Button>
      </div>
    );
  }
  if (files === null) {
    return (
      <div className="py-1 pl-[40px] pr-3">
        <SkeletonRows rows={3} />
      </div>
    );
  }
  if (files.length === 0) {
    return <p className="py-2 pl-[40px] pr-3 text-[12px] text-muted">No files of this type in the scan.</p>;
  }
  return (
    <div className="py-1 pl-[40px] pr-3">
      {revealError ? <p role="alert" className="pb-1 text-[12px] text-critical">{revealError}</p> : null}
      <ul className="divide-y divide-border/60">
        {files.map((file) => (
          <li key={file.path} className="flex items-center gap-3 py-1.5">
            <div className="min-w-0 flex-1">
              <p className="file-name truncate text-[13px] text-foreground" title={file.path}>
                {file.name}
              </p>
              <p className="mono-path truncate text-subtle" title={file.path}>
                {displayPath(file.path, homeDir)}
              </p>
            </div>
            <span className="w-[92px] shrink-0 text-right text-[13px] tabular text-foreground">
              {formatBytes(file.bytes)}
            </span>
            <Button variant="ghost" size="sm" onClick={() => void reveal(file.path)}>
              <FolderOpen className="size-3.5" aria-hidden="true" />
              Reveal
            </Button>
          </li>
        ))}
      </ul>
    </div>
  );
}

export function TypesView() {
  const summary = useAppStore((s) => s.summary);
  const finishedAtMs = summary?.finishedAtMs;
  const [stats, setStats] = useState<ExtensionStat[] | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [reloadKey, setReloadKey] = useState(0);
  const [expanded, setExpanded] = useState<string | null>(null);

  const load = useCallback(async () => {
    const rows = await api.getExtensionStats();
    setStats([...rows].sort((a, b) => b.bytes - a.bytes));
    setLoadError(null);
  }, []);

  /* Extension statistics describe the current scan: loaded on
     mount and re-read when a scan finishes. A failed read is an
     error state, never an empty list. */
  useEffect(() => {
    let live = true;
    load().catch((err: unknown) => {
      if (live) {
        setLoadError(err instanceof Error ? err.message : "File types could not be loaded.");
      }
    });
    return () => {
      live = false;
    };
  }, [load, finishedAtMs, reloadKey]);

  const coverage = useMemo(
    () => topTypesCoverage(stats ?? [], summary?.totalBytes ?? 0),
    [stats, summary],
  );

  if (loadError) {
    return (
      <EmptyState
        icon={Layers}
        className="min-h-[320px]"
        title="File types couldn't be loaded"
        description={loadError}
        action={
          <Button variant="secondary" onClick={() => setReloadKey((n) => n + 1)}>
            Retry
          </Button>
        }
      />
    );
  }
  if (stats === null) return <SkeletonRows rows={8} />;
  if (stats.length === 0) {
    return (
      <EmptyState
        icon={Layers}
        className="min-h-[320px]"
        title="No file types yet"
        description="After a scan, the files it measured are ranked here by extension — the quickest way to see what kind of data fills the disk."
      />
    );
  }

  const shown = stats.slice(0, MAX_ROWS);
  const max = shown[0]?.bytes ?? 1;
  const largest = shown[0];

  return (
    <div>
      <StatStrip
        className="pb-4"
        stats={[
          { label: "File types", value: formatCount(stats.length) },
          {
            label: "Largest type",
            value: largest ? `${extensionLabel(largest.extension)} · ${formatBytes(largest.bytes)}` : "—",
          },
          { label: "Coverage of top 10", value: formatPercent(coverage) },
        ]}
      />

      <ul className="divide-y divide-border rounded-[10px] border border-border">
        {shown.map((stat) => {
          const isOpen = expanded === stat.extension;
          const Icon = isOpen ? ChevronDown : ChevronRight;
          return (
            <li key={stat.extension}>
              <button
                type="button"
                aria-expanded={isOpen}
                onClick={() => setExpanded(isOpen ? null : stat.extension)}
                className="flex w-full items-center gap-3 px-3 py-2 text-left transition-colors duration-120 hover:bg-hover"
              >
                <Icon className="size-4 shrink-0 text-subtle" aria-hidden="true" />
                <span
                  className={cn(
                    "w-[120px] shrink-0 truncate",
                    stat.extension === ""
                      ? "text-[12px] text-muted"
                      : "rounded bg-panel-raised px-1.5 py-0.5 font-mono text-[12px] text-foreground ring-1 ring-border",
                  )}
                >
                  {extensionLabel(stat.extension)}
                </span>
                <span className="w-[96px] shrink-0 text-[12px] tabular text-muted">
                  {formatCount(stat.count)} files
                </span>
                <SizeBar fraction={stat.bytes / max} height={4} className="min-w-0 flex-1" />
                <span className="w-[92px] shrink-0 text-right text-[13px] tabular text-foreground">
                  {formatBytes(stat.bytes)}
                </span>
              </button>
              {isOpen ? (
                <div className="border-t border-border/60 bg-panel/50">
                  <TypeFiles extension={stat.extension} />
                </div>
              ) : null}
            </li>
          );
        })}
      </ul>
      {stats.length > MAX_ROWS ? (
        <p className="mt-2 text-[12px] text-muted">
          Showing the {MAX_ROWS} largest types of {formatCount(stats.length)}.
        </p>
      ) : null}
    </div>
  );
}
