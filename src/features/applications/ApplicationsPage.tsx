import { useEffect, useMemo, useState } from "react";
import { AppWindow, FolderOpen, PackageOpen, Search } from "lucide-react";
import * as api from "@/lib/api";
import { useAppStore } from "@/lib/store";
import { displayPath, formatBytes } from "@/lib/format";
import type { AppEntry } from "@/lib/types";
import { PageHeader } from "@/components/layout/PageHeader";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Section } from "@/components/ui/section";
import { SkeletonRows } from "@/components/ui/skeleton";
import { AppIcon } from "@/components/shared/AppIcon";
import { EmptyState } from "@/components/shared/EmptyState";
import { MarkerTag } from "@/components/shared/MarkerTag";
import { SizeBar } from "@/components/shared/SizeBar";
import { SortHeader } from "@/components/shared/SortHeader";
import type { SortDirection } from "@/lib/analysis";

type AppSortKey = "name" | "total" | "install" | "data";

function appTotal(app: AppEntry): number {
  return app.installBytes + app.dataBytes;
}

export function ApplicationsPage() {
  const homeDir = useAppStore((s) => s.env?.homeDir);
  const navigate = useAppStore((s) => s.navigate);
  const finishedAtMs = useAppStore((s) => s.summary?.finishedAtMs);
  const [apps, setApps] = useState<AppEntry[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  const [sortKey, setSortKey] = useState<AppSortKey>("total");
  const [direction, setDirection] = useState<SortDirection>("desc");
  const [revealError, setRevealError] = useState<string | null>(null);

  /* Reloads whenever a scan finishes — app data figures come from
     the scan, so a new scan is exactly when they change. */
  useEffect(() => {
    let live = true;
    api.getApplications()
      .then((rows) => { if (live) setApps(rows); })
      .catch((err: unknown) => {
        if (live) setError(err instanceof Error ? err.message : "Applications could not be loaded.");
      });
    return () => { live = false; };
  }, [finishedAtMs]);

  const installed = useMemo(() => {
    const rows = (apps ?? []).filter((a) => !a.leftover);
    const q = query.trim().toLowerCase();
    const filtered = q
      ? rows.filter((a) => a.name.toLowerCase().includes(q) || a.publisher.toLowerCase().includes(q))
      : rows;
    const factor = direction === "asc" ? 1 : -1;
    return [...filtered].sort((a, b) => {
      if (sortKey === "name") return a.name.localeCompare(b.name) * factor;
      if (sortKey === "install") return (a.installBytes - b.installBytes) * factor;
      if (sortKey === "data") return (a.dataBytes - b.dataBytes) * factor;
      return (appTotal(a) - appTotal(b)) * factor;
    });
  }, [apps, query, sortKey, direction]);

  const leftovers = useMemo(() => (apps ?? []).filter((a) => a.leftover), [apps]);
  const maxTotal = Math.max(...installed.map(appTotal), 1);

  const onSort = (key: string) => {
    const k = key as AppSortKey;
    if (k === sortKey) setDirection((d) => (d === "asc" ? "desc" : "asc"));
    else { setSortKey(k); setDirection(k === "name" ? "asc" : "desc"); }
  };

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
          title="Applications"
          subtitle="Installed size plus the data each app keeps in AppData. Sort by total to find the heaviest apps."
        />
        {error ? <p role="alert" className="pb-3 text-[12px] text-critical">{error}</p> : null}
        {revealError ? <p role="alert" className="pb-3 text-[12px] text-critical">{revealError}</p> : null}

        <div className="max-w-[420px] pb-3">
          <div className="relative">
            <Search className="pointer-events-none absolute left-2.5 top-1/2 size-4 -translate-y-1/2 text-subtle" aria-hidden="true" />
            <Input value={query} onChange={(e) => setQuery(e.target.value)} placeholder="Filter applications" aria-label="Filter applications" className="pl-8" />
          </div>
        </div>

        {apps === null ? (
          <SkeletonRows rows={7} />
        ) : installed.length === 0 && leftovers.length === 0 ? (
          <EmptyState
            icon={AppWindow}
            className="min-h-[360px]"
            title="No applications found"
            description="Installed applications and their AppData appear here once the inventory is available. Run a scan to measure application data."
          />
        ) : (
          <>
            <div className="overflow-hidden rounded-[10px] border border-border">
              <table className="w-full border-collapse">
                <thead>
                  <tr className="border-b border-border text-left">
                    <th scope="col" className="px-3 py-2"><SortHeader label="Application" sortKeyValue="name" activeKey={sortKey} direction={direction} onSort={onSort} /></th>
                    <th scope="col" className="px-3 py-2 text-right"><SortHeader label="Installed" sortKeyValue="install" activeKey={sortKey} direction={direction} onSort={onSort} align="right" /></th>
                    <th scope="col" className="px-3 py-2 text-right"><SortHeader label="App data" sortKeyValue="data" activeKey={sortKey} direction={direction} onSort={onSort} align="right" /></th>
                    <th scope="col" className="px-3 py-2 text-right"><SortHeader label="Total" sortKeyValue="total" activeKey={sortKey} direction={direction} onSort={onSort} align="right" /></th>
                  </tr>
                </thead>
                <tbody>
                  {installed.length === 0 ? (
                    <tr>
                      <td colSpan={4} className="px-3 py-8 text-center text-[12px] text-muted">
                        {query.trim()
                          ? "No applications match this filter."
                          : "No installed applications were found."}
                      </td>
                    </tr>
                  ) : null}
                  {installed.map((app) => (
                    <tr key={app.name} className="border-b border-border/60 transition-colors duration-120 hover:bg-hover">
                      <td className="px-3 py-2">
                        <span className="flex items-center gap-2.5">
                          <AppIcon name={app.name} />
                          <span className="min-w-0">
                            <span className="block truncate text-[13px] font-medium text-foreground">{app.name}</span>
                            <span className="block truncate text-[12px] text-muted">{app.publisher || "Unknown publisher"}</span>
                          </span>
                        </span>
                      </td>
                      <td className="px-3 py-2 text-right text-[13px] tabular text-muted">{formatBytes(app.installBytes)}</td>
                      <td className="px-3 py-2 text-right text-[13px] tabular text-muted">{formatBytes(app.dataBytes)}</td>
                      <td className="px-3 py-2 text-right">
                        <span className="text-[13px] font-medium tabular text-foreground">{formatBytes(appTotal(app))}</span>
                        <SizeBar fraction={appTotal(app) / maxTotal} height={2} className="ml-auto mt-0.5 w-[72px]" />
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>

            <Section
              title="Leftovers from uninstalled apps"
              description="Data folders left behind after an app was removed. These are reported for review only — DiskPilot never removes them automatically, because they can contain documents and saved work."
            >
              {leftovers.length === 0 ? (
                <p className="flex items-center gap-2 text-[12px] text-muted">
                  <PackageOpen className="size-4" aria-hidden="true" />
                  No leftover data folders were detected.
                </p>
              ) : (
                <ul className="divide-y divide-border">
                  {leftovers.map((app) => (
                    <li key={app.name} className="flex items-start gap-3 py-2.5">
                      <AppIcon name={app.name} />
                      <div className="min-w-0 flex-1">
                        <p className="flex items-center gap-2 text-[13px] font-medium text-foreground">
                          {app.name}
                          <MarkerTag tone="warn">Leftover</MarkerTag>
                        </p>
                        <p className="text-[12px] text-muted">{app.publisher || "Publisher unknown"} · {formatBytes(app.dataBytes)} of data remains</p>
                        <ul className="mt-1 space-y-0.5">
                          {app.dataPaths.map((path) => (
                            <li key={path} className="flex items-center gap-2">
                              <span className="mono-path truncate text-subtle">{displayPath(path, homeDir)}</span>
                              <Button variant="ghost" size="sm" onClick={() => void reveal(path)}>
                                <FolderOpen className="size-3.5" aria-hidden="true" />
                                Review in Explorer
                              </Button>
                            </li>
                          ))}
                        </ul>
                      </div>
                      <Button variant="secondary" size="sm" onClick={() => navigate("storage")}>
                        Open Storage
                      </Button>
                    </li>
                  ))}
                </ul>
              )}
            </Section>
          </>
        )}
      </div>
    </div>
  );
}
