import { useCallback, useEffect, useMemo, useState } from "react";
import { History as HistoryIcon, Search, Sparkles } from "lucide-react";
import * as api from "@/lib/api";
import { useAppStore } from "@/lib/store";
import { displayPath, formatBytes, formatDate } from "@/lib/format";
import { historyDetail, operationTitle } from "@/lib/history";
import type { HistoryEntry, RestoreCleanupRunResult } from "@/lib/types";
import { PageHeader } from "@/components/layout/PageHeader";
import { Input } from "@/components/ui/input";
import { SkeletonRows } from "@/components/ui/skeleton";
import { CopyButton } from "@/components/shared/CopyButton";
import { EmptyState } from "@/components/shared/EmptyState";
import { RestoreRunAction } from "./RestoreRunAction";

export function CleanupHistory({
  entries,
  homeDir,
  onRefresh,
}: {
  entries: HistoryEntry[];
  homeDir?: string;
  onRefresh?: () => void;
}) {
  /* Restore outcomes live here, not in the row: refreshing the
     entries after a restore must not unmount the result. */
  const [restoreResults, setRestoreResults] = useState<Record<number, RestoreCleanupRunResult>>({});
  const total = entries.reduce((acc, e) => acc + e.reclaimedBytes, 0);
  return (
    <div>
      <p className="pb-3 text-[12px] text-muted">
        Total reclaimed across recorded actions:{" "}
        <span className="font-medium tabular text-foreground">{formatBytes(total)}</span>
      </p>
      <ul className="divide-y divide-border border-t border-border">
        {entries.map((entry) => {
          const detail = historyDetail(entry.detail);
          return (
            <li key={entry.id} className="py-3">
              <div className="flex items-center gap-2.5">
                <Sparkles className="size-4 shrink-0 text-muted" aria-hidden="true" />
                <p className="min-w-0 flex-1 truncate text-[13px] font-medium text-foreground">{operationTitle(entry.operation)}</p>
                <span className="text-[12px] tabular text-muted">{formatDate(entry.timestampMs)}</span>
                {entry.reclaimedBytes > 0 ? (
                  <span className="w-[92px] text-right text-[13px] tabular text-tier-safe">{formatBytes(entry.reclaimedBytes)}</span>
                ) : (
                  <span className="w-[92px] text-right text-[13px] tabular text-muted">—</span>
                )}
              </div>
              {detail?.kind === "text" ? (
                <p className="mt-1 pl-[26px] text-[12px] leading-4 text-muted">{detail.text}</p>
              ) : detail?.kind === "reference" ? (
                <p className="mt-1 pl-[26px] text-[11px] leading-[14px] text-subtle">{detail.label}</p>
              ) : null}
              {entry.paths.length > 0 ? (
                <ul className="mt-1 space-y-0.5 pl-[26px]">
                  {entry.paths.slice(0, 6).map((path) => (
                    <li key={path} className="mono-path truncate text-subtle" title={path}>{displayPath(path, homeDir)}</li>
                  ))}
                  {entry.paths.length > 6 ? (
                    <li className="text-[11px] text-subtle">and {entry.paths.length - 6} more paths</li>
                  ) : null}
                </ul>
              ) : null}
              {entry.errors.length > 0 ? (
                <ul className="mt-1 space-y-0.5 pl-[26px]">
                  {entry.errors.map((err, i) => (
                    <li key={i} className="text-[12px] text-critical">{err}</li>
                  ))}
                </ul>
              ) : null}
              {(entry.restorableCount ?? 0) > 0 || restoreResults[entry.id] ? (
                <div className="mt-2 pl-[26px]">
                  <RestoreRunAction
                    entry={entry}
                    result={restoreResults[entry.id] ?? null}
                    onCompleted={(result) => {
                      setRestoreResults((prev) => ({ ...prev, [entry.id]: result }));
                      onRefresh?.();
                    }}
                  />
                </div>
              ) : null}
            </li>
          );
        })}
      </ul>
    </div>
  );
}

export function HistoryPage() {
  const homeDir = useAppStore((s) => s.env?.homeDir);
  const [entries, setEntries] = useState<HistoryEntry[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [query, setQuery] = useState("");

  const load = useCallback(async () => {
    const rows = await api.getHistory();
    setEntries([...rows].sort((a, b) => b.timestampMs - a.timestampMs));
  }, []);

  useEffect(() => {
    let live = true;
    load().catch((err: unknown) => {
      if (live) setError(err instanceof Error ? err.message : "History could not be loaded.");
    });
    return () => { live = false; };
  }, [load]);

  const visible = useMemo(() => {
    if (!entries) return [];
    const q = query.trim().toLowerCase();
    if (!q) return entries;
    return entries.filter(
      (e) =>
        e.operation.toLowerCase().includes(q) ||
        operationTitle(e.operation).toLowerCase().includes(q) ||
        (e.detail ?? "").toLowerCase().includes(q) ||
        e.paths.some((p) => p.toLowerCase().includes(q)),
    );
  }, [entries, query]);

  return (
    <div className="h-full overflow-y-auto px-6 py-6">
      <div className="max-w-[880px]">
        <PageHeader
          title="History"
          subtitle="A local record of every cleanup action and scan — what ran, which paths it touched, how much space came back, and any errors."
          showScanControl={false}
        />
        {error ? <p role="alert" className="pb-3 text-[12px] text-critical">{error}</p> : null}
        {entries === null ? (
          <SkeletonRows rows={6} />
        ) : entries.length === 0 ? (
          <EmptyState
            icon={HistoryIcon}
            className="min-h-[360px]"
            title="No history yet"
            description="Cleanup actions, Docker actions, Recycle Bin emptying, and scans are recorded here with their exact paths and results. Nothing has been recorded yet."
          />
        ) : (
          <>
            <div className="flex max-w-[560px] items-center gap-2 pb-1">
              <div className="relative min-w-0 flex-1">
                <Search className="pointer-events-none absolute left-2.5 top-1/2 size-4 -translate-y-1/2 text-subtle" aria-hidden="true" />
                <Input value={query} onChange={(e) => setQuery(e.target.value)} placeholder="Filter history" aria-label="Filter history" className="pl-8" />
              </div>
              <CopyButton
                text={JSON.stringify(visible, null, 2)}
                label="Copy as JSON"
              />
            </div>
            {visible.length === 0 ? (
              <p className="py-8 text-center text-[12px] text-muted">No history entries match the filter.</p>
            ) : (
              <CleanupHistory entries={visible} homeDir={homeDir} onRefresh={() => void load()} />
            )}
          </>
        )}
      </div>
    </div>
  );
}
