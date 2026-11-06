import { useEffect, useMemo, useState } from "react";
import { History as HistoryIcon, Sparkles } from "lucide-react";
import * as api from "@/lib/api";
import { useAppStore } from "@/lib/store";
import { displayPath, formatBytes, formatDate } from "@/lib/format";
import type { HistoryEntry } from "@/lib/types";
import { PageHeader } from "@/components/layout/PageHeader";
import { Input } from "@/components/ui/input";
import { SkeletonRows } from "@/components/ui/skeleton";
import { EmptyState } from "@/components/shared/EmptyState";
import { MarkerTag } from "@/components/shared/MarkerTag";

export function CleanupHistory({ entries, homeDir }: { entries: HistoryEntry[]; homeDir?: string }) {
  const total = entries.reduce((acc, e) => acc + e.reclaimedBytes, 0);
  return (
    <div>
      <p className="pb-3 text-[12px] text-muted">
        Total reclaimed across recorded actions:{" "}
        <span className="font-medium tabular text-foreground">{formatBytes(total)}</span>
      </p>
      <ul className="divide-y divide-border border-t border-border">
        {entries.map((entry) => {
          return (
            <li key={entry.id} className="py-3">
              <div className="flex items-center gap-2.5">
                <Sparkles className="size-4 shrink-0 text-muted" aria-hidden="true" />
                <p className="min-w-0 flex-1 truncate text-[13px] font-medium text-foreground">{entry.operation}</p>
                <MarkerTag>{entry.operation}</MarkerTag>
                <span className="text-[12px] tabular text-muted">{formatDate(entry.timestampMs)}</span>
                {entry.reclaimedBytes > 0 ? (
                  <span className="w-[92px] text-right text-[13px] tabular text-tier-safe">{formatBytes(entry.reclaimedBytes)}</span>
                ) : (
                  <span className="w-[92px] text-right text-[13px] tabular text-muted">—</span>
                )}
              </div>
              {entry.detail ? (
                <p className="mono-path mt-1 pl-[26px] text-subtle">{entry.detail}</p>
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

  useEffect(() => {
    let live = true;
    api.getHistory()
      .then((rows) => { if (live) setEntries([...rows].sort((a, b) => b.timestampMs - a.timestampMs)); })
      .catch((err: unknown) => {
        if (live) setError(err instanceof Error ? err.message : "History could not be loaded.");
      });
    return () => { live = false; };
  }, []);

  const visible = useMemo(() => {
    if (!entries) return [];
    const q = query.trim().toLowerCase();
    if (!q) return entries;
    return entries.filter(
      (e) =>
        e.operation.toLowerCase().includes(q) ||
        (e.detail ?? "").toLowerCase().includes(q) ||
        e.paths.some((p) => p.toLowerCase().includes(q)),
    );
  }, [entries, query]);

  return (
    <div className="h-full overflow-y-auto px-6 py-5">
      <div className="mx-auto max-w-[880px]">
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
            title="No history yet"
            description="Cleanup actions, Docker actions, Recycle Bin emptying, and scans are recorded here with their exact paths and results. Nothing has been recorded yet."
          />
        ) : (
          <>
            <div className="max-w-[420px] pb-1">
              <Input value={query} onChange={(e) => setQuery(e.target.value)} placeholder="Filter history" aria-label="Filter history" />
            </div>
            {visible.length === 0 ? (
              <p className="py-8 text-center text-[12px] text-muted">No history entries match the filter.</p>
            ) : (
              <CleanupHistory entries={visible} homeDir={homeDir} />
            )}
          </>
        )}
      </div>
    </div>
  );
}
