import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { ChevronRight, CloudUpload, File, Folder, FolderOpen, Search, X } from "lucide-react";
import * as api from "@/lib/api";
import { useAppStore } from "@/lib/store";
import { filterByQuery, sortExplorerRows, type FileSortKey, type SortDirection } from "@/lib/analysis";
import { categoryMeta } from "@/lib/categories";
import { basename, displayPath, formatBytes, formatDate, parentPath } from "@/lib/format";
import { tierMeta } from "@/lib/tiers";
import type { CloudStatus, DirectoryEntry, MediaInfo, Recommendation } from "@/lib/types";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Treemap, type TreemapBlock } from "@/components/Treemap";
import { CategoryBadge } from "@/components/shared/CategoryBadge";
import { CategorySelect, type CategoryFilter } from "@/components/shared/CategorySelect";
import { ClassificationCard } from "@/components/shared/ClassificationCard";
import { DetailsPanel, DetailRow } from "@/components/shared/DetailsPanel";
import { EmptyState } from "@/components/shared/EmptyState";
import { FreeUpSpaceDialog } from "@/components/shared/FreeUpSpaceDialog";
import { SizeBar } from "@/components/shared/SizeBar";
import { SortHeader } from "@/components/shared/SortHeader";
import { TierBadge } from "@/components/shared/TierBadge";
import { cn } from "@/lib/utils";

/** Extensions the shell's media-info reader understands (EXIF). */
const IMAGE_EXTENSIONS = new Set(["png", "jpg", "jpeg", "gif", "bmp", "webp", "tif", "tiff", "heic", "heif"]);

/** How many individual file blocks the map shows before the rest are
    folded into one aggregate block. */
const MAP_FILE_LIMIT = 48;

/** True when `path` is the cloud root itself or lives under it. */
export function isPathUnder(path: string, root: string): boolean {
  const p = path.replace(/\//g, "\\").replace(/\\+$/, "").toLowerCase();
  const r = root.replace(/\//g, "\\").replace(/\\+$/, "").toLowerCase();
  return p === r || p.startsWith(r + "\\");
}

export interface ExplorerRow {
  kind: "folder" | "file";
  path: string;
  name: string;
  bytes: number;
  category: DirectoryEntry["category"];
  entry: DirectoryEntry;
}

/** Directory listing entries become table rows, folders first. Sorting,
    filtering and the size bar are applied by the page afterwards. */
export function entriesToRows(entries: DirectoryEntry[]): ExplorerRow[] {
  return entries.map((entry) => ({
    kind: entry.isDir ? ("folder" as const) : ("file" as const),
    path: entry.path,
    name: entry.name,
    bytes: entry.bytes,
    category: entry.category,
    entry,
  }));
}

interface Crumb {
  name: string;
  path: string;
}

/** Breadcrumb trail from the scan root down to `currentPath`. Paths
    outside the root (should not happen) fall back to a single crumb. */
export function breadcrumbsBetween(rootPath: string, currentPath: string): Crumb[] {
  const root = rootPath.replace(/[\\/]+$/, "");
  const current = currentPath.replace(/[\\/]+$/, "");
  if (current.toLowerCase() === root.toLowerCase()) {
    return [{ name: basename(root) || root, path: rootPath }];
  }
  const prefix = root + "\\";
  if (!current.toLowerCase().startsWith(prefix.toLowerCase())) {
    return [{ name: basename(current) || current, path: currentPath }];
  }
  const crumbs: Crumb[] = [{ name: basename(root) || root, path: rootPath }];
  let acc = root;
  for (const part of current.slice(prefix.length).split(/[\\/]+/).filter(Boolean)) {
    acc = `${acc}\\${part}`;
    crumbs.push({ name: part, path: acc });
  }
  return crumbs;
}

function recommendationFor(path: string, recommendations: Recommendation[]): Recommendation | null {
  const lower = path.toLowerCase();
  return (
    recommendations.find((r) =>
      r.paths.some((p) => {
        const rp = p.toLowerCase();
        return lower === rp || lower.startsWith(rp + "\\");
      }),
    ) ?? null
  );
}

export function ExplorerView({
  rootPath,
  embedded = false,
  mode = "list",
}: {
  rootPath: string;
  embedded?: boolean;
  /** "list" is the file table; "map" renders the same level as a
      treemap. Both share navigation, filters and the inspector. */
  mode?: "list" | "map";
}) {
  const homeDir = useAppStore((s) => s.env?.homeDir);
  const recommendations = useAppStore((s) => s.recommendations);
  const setBanner = useAppStore((s) => s.setBanner);
  const [currentPath, setCurrentPath] = useState(rootPath);
  const [entries, setEntries] = useState<DirectoryEntry[] | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  const [categoryFilter, setCategoryFilter] = useState<CategoryFilter>("all");
  const [sortKey, setSortKey] = useState<FileSortKey>("size");
  const [direction, setDirection] = useState<SortDirection>("desc");
  const [selectedPath, setSelectedPath] = useState<string | null>(null);
  const [revealError, setRevealError] = useState<string | null>(null);
  const [cloud, setCloud] = useState<CloudStatus | null>(null);
  const [mediaInfo, setMediaInfo] = useState<MediaInfo | null>(null);
  const [freeUpOpen, setFreeUpOpen] = useState(false);
  const requestSeq = useRef(0);

  /* The cloud folder (if any) decides whether the inspector offers
     "Free up space" for the selected file. Best-effort. */
  useEffect(() => {
    let live = true;
    void api.getCloudStatus().then((status) => {
      if (live) setCloud(status);
    });
    return () => {
      live = false;
    };
  }, []);

  // A new scan (new root) restarts browsing at the root.
  useEffect(() => {
    setCurrentPath(rootPath);
    setSelectedPath(null);
  }, [rootPath]);

  const load = useCallback(async (path: string) => {
    const seq = ++requestSeq.current;
    setEntries(null);
    setLoadError(null);
    try {
      const rows = await api.listDirectory(path);
      if (requestSeq.current !== seq) return;
      setEntries(rows);
    } catch (err) {
      if (requestSeq.current !== seq) return;
      setEntries([]);
      setLoadError(err instanceof Error ? err.message : "This folder could not be listed.");
    }
  }, []);

  useEffect(() => {
    void load(currentPath);
  }, [currentPath, load]);

  const crumbs = useMemo(
    () => breadcrumbsBetween(rootPath, currentPath),
    [rootPath, currentPath],
  );

  const rows = useMemo(() => {
    let list = entriesToRows(entries ?? []);
    list = filterByQuery(list, query);
    if (categoryFilter !== "all") list = list.filter((r) => r.category === categoryFilter);
    const sorted = sortExplorerRows(list, sortKey, direction);
    // Folders stay grouped before files, matching file-manager convention.
    return [...sorted.filter((r) => r.kind === "folder"), ...sorted.filter((r) => r.kind === "file")];
  }, [entries, query, categoryFilter, sortKey, direction]);

  const selected = rows.find((r) => r.path === selectedPath) ?? null;
  const selectedRec = selected ? recommendationFor(selected.path, recommendations) : null;
  const classification = selected?.entry.classification ?? null;
  const maxBytes = rows.length > 0 ? Math.max(...rows.map((r) => r.bytes), 1) : 1;

  /* EXIF for the selected image, loaded lazily; the cancelled flag
     keeps a slow read from landing on a newer selection. */
  const selectedFilePath = selected?.kind === "file" ? selected.path : null;
  useEffect(() => {
    setMediaInfo(null);
    if (!selectedFilePath) return;
    const ext = selectedFilePath.split(".").pop()?.toLowerCase() ?? "";
    if (!IMAGE_EXTENSIONS.has(ext)) return;
    let live = true;
    void api.getMediaInfo(selectedFilePath).then((info) => {
      if (live) setMediaInfo(info);
    });
    return () => {
      live = false;
    };
  }, [selectedFilePath]);

  const selectedInCloud =
    selected?.kind === "file" && cloud ? isPathUnder(selected.path, cloud.rootPath) : false;

  /* Map blocks for the current level: every folder, the largest
     files individually, and the long tail folded into one aggregate. */
  const treemapBlocks = useMemo<TreemapBlock[]>(() => {
    const blocks: TreemapBlock[] = [];
    const folders = rows.filter((r) => r.kind === "folder");
    const files = [...rows.filter((r) => r.kind === "file")].sort((a, b) => b.bytes - a.bytes);
    for (const row of folders) {
      blocks.push({
        id: row.path,
        name: row.name,
        bytes: row.bytes,
        colorVar: categoryMeta(row.category).colorVar,
        isDir: true,
        path: row.path,
      });
    }
    const shownFiles = files.slice(0, MAP_FILE_LIMIT);
    const restFiles = files.slice(MAP_FILE_LIMIT);
    for (const row of shownFiles) {
      blocks.push({
        id: row.path,
        name: row.name,
        bytes: row.bytes,
        colorVar: categoryMeta(row.category).colorVar,
        isDir: false,
        path: row.path,
      });
    }
    if (restFiles.length > 0) {
      blocks.push({
        id: `${currentPath}::other-files`,
        name: `${restFiles.length} more files`,
        bytes: restFiles.reduce((acc, r) => acc + r.bytes, 0),
        colorVar: categoryMeta("unknown").colorVar,
        isDir: false,
        path: currentPath,
        aggregate: true,
      });
    }
    return blocks;
  }, [rows, currentPath]);

  const activateBlock = (block: TreemapBlock) => {
    if (block.aggregate) return;
    if (block.isDir) {
      navigateTo(block.path);
    } else {
      setSelectedPath(block.path);
    }
  };

  const onSort = (key: string) => {
    const k = key as FileSortKey;
    if (k === sortKey) {
      setDirection((d) => (d === "asc" ? "desc" : "asc"));
    } else {
      setSortKey(k);
      setDirection(k === "name" ? "asc" : "desc");
    }
  };

  const navigateTo = (path: string) => {
    setCurrentPath(path);
    setSelectedPath(null);
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
    <div className={cn("flex min-h-0 gap-4", embedded ? "h-full" : "")}>
      <div className="flex min-w-0 flex-1 flex-col">
        {/* Breadcrumb */}
        <nav aria-label="Breadcrumb" className="flex flex-wrap items-center gap-0.5 pb-2">
          {crumbs.map((crumb, i) => (
            <span key={crumb.path} className="flex items-center gap-0.5">
              {i > 0 ? <ChevronRight className="size-3.5 text-subtle" aria-hidden="true" /> : null}
              <button
                type="button"
                onClick={() => navigateTo(crumb.path)}
                aria-current={crumb.path === currentPath ? "location" : undefined}
                className={cn(
                  "rounded-md px-1.5 py-1 text-[12px] transition-colors duration-120 hover:bg-hover",
                  crumb.path === currentPath ? "font-semibold text-foreground" : "text-muted",
                )}
              >
                {i === 0 ? displayPath(crumb.path, homeDir) : crumb.name}
              </button>
            </span>
          ))}
        </nav>

        {/* Filters */}
        <div className="flex items-center gap-2 pb-2">
          <div className="relative flex-1">
            <Search className="pointer-events-none absolute left-2.5 top-1/2 size-4 -translate-y-1/2 text-subtle" aria-hidden="true" />
            <Input
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder="Filter by name"
              aria-label="Filter by name"
              className={cn("pl-8", query ? "pr-8" : "")}
            />
            {query ? (
              <button
                type="button"
                onClick={() => setQuery("")}
                aria-label="Clear filter"
                className="absolute right-2 top-1/2 -translate-y-1/2 text-subtle transition-colors duration-120 hover:text-foreground"
              >
                <X className="size-4" aria-hidden="true" />
              </button>
            ) : null}
          </div>
          <CategorySelect value={categoryFilter} onChange={setCategoryFilter} />
        </div>

        {revealError ? (
          <p role="alert" className="pb-2 text-[12px] text-critical">{revealError}</p>
        ) : null}
        {loadError ? (
          <p role="alert" className="pb-2 text-[12px] text-critical">{loadError}</p>
        ) : null}

        {/* Table or map */}
        {mode === "map" ? (
          <div className="min-h-0 flex-1">
            {entries === null ? (
              <div className="flex h-full items-center justify-center rounded-[10px] border border-border text-[12px] text-muted">
                Loading folder…
              </div>
            ) : (
              <Treemap blocks={treemapBlocks} selectedId={selectedPath} onActivate={activateBlock} />
            )}
          </div>
        ) : (
        <div className="min-h-0 flex-1 overflow-y-auto rounded-[10px] border border-border">
          <table className="w-full border-collapse">
            <thead className="sticky top-0 bg-panel">
              <tr className="border-b border-border text-left">
                <th scope="col" className="px-3 py-2">
                  <SortHeader label="Name" sortKeyValue="name" activeKey={sortKey} direction={direction} onSort={onSort} />
                </th>
                <th scope="col" className="px-3 py-2">
                  <SortHeader label="Category" sortKeyValue="category" activeKey={sortKey} direction={direction} onSort={onSort} />
                </th>
                <th scope="col" className="px-3 py-2 text-[12px] font-medium text-muted">Cleanup</th>
                <th scope="col" className="px-3 py-2 text-right">
                  <SortHeader label="Size" sortKeyValue="size" activeKey={sortKey} direction={direction} onSort={onSort} align="right" />
                </th>
              </tr>
            </thead>
            <tbody>
              {rows.map((row) => {
                const rec = recommendationFor(row.path, recommendations);
                const isSelected = row.path === selectedPath;
                return (
                  <tr
                    key={row.path}
                    onClick={() => setSelectedPath(row.path)}
                    onDoubleClick={() => {
                      if (row.kind === "folder") navigateTo(row.path);
                    }}
                    aria-selected={isSelected}
                    className={cn(
                      "h-8 cursor-default border-b border-border/60 transition-colors duration-120 hover:bg-hover",
                      isSelected && "bg-selected hover:bg-selected",
                    )}
                  >
                    <td className="max-w-[320px] px-3 py-1">
                      <span className="flex items-center gap-2">
                        {row.kind === "folder" ? (
                          <Folder className="size-4 shrink-0 text-muted" aria-hidden="true" />
                        ) : (
                          <File className="size-4 shrink-0 text-subtle" aria-hidden="true" />
                        )}
                        <span className="truncate text-[13px] text-foreground" title={row.path}>{row.name}</span>
                      </span>
                    </td>
                    <td className="px-3 py-1"><CategoryBadge category={row.category} /></td>
                    <td className="px-3 py-1">
                      {rec ? (
                        <span className={cn("text-[12px]", tierMeta(rec.tier).textClass)}>
                          {rec.tier === "keep" ? "Keep" : `${tierMeta(rec.tier).label} · ${formatBytes(rec.bytes)}`}
                        </span>
                      ) : (
                        <span className="text-[12px] text-subtle">—</span>
                      )}
                    </td>
                    <td className="px-3 py-1 text-right">
                      <span className="text-[13px] tabular text-foreground">{formatBytes(row.bytes)}</span>
                      <SizeBar
                        fraction={row.bytes / maxBytes}
                        colorVar={categoryMeta(row.category).colorVar}
                        height={2}
                        className="mt-0.5"
                      />
                    </td>
                  </tr>
                );
              })}
              {entries !== null && rows.length === 0 ? (
                <tr>
                  <td colSpan={4} className="px-3 py-8 text-center text-[12px] text-muted">
                    {entries.length === 0
                      ? "This folder is empty in the scan index."
                      : "No items match"}
                  </td>
                </tr>
              ) : null}
              {entries === null ? (
                <tr>
                  <td colSpan={4} className="px-3 py-8 text-center text-[12px] text-muted">
                    Loading folder…
                  </td>
                </tr>
              ) : null}
            </tbody>
          </table>
        </div>
        )}
        <p className="pt-2 text-[12px] text-muted">
          {mode === "map"
            ? "Block size is the item's size. Click a folder to open it; click a file to inspect it."
            : "Double-click a folder to open it. Select any row to see its classification and details."}
          {currentPath.replace(/[\\/]+$/, "").toLowerCase() !== rootPath.replace(/[\\/]+$/, "").toLowerCase() ? (
            <>
              {" "}
              <button
                type="button"
                className="font-medium text-foreground underline-offset-2 hover:underline"
                onClick={() => navigateTo(parentPath(currentPath) === currentPath ? rootPath : parentPath(currentPath))}
              >
                Go up one level
              </button>
            </>
          ) : null}
        </p>
      </div>

      {selected ? (
        <DetailsPanel
          title={selected.name}
          subtitle={<span className="mono-path block text-subtle">{displayPath(selected.path, homeDir)}</span>}
          onClose={() => setSelectedPath(null)}
          footer={
            <div className="space-y-2">
              <Button variant="secondary" className="w-full" onClick={() => void reveal(selected.path)}>
                <FolderOpen className="size-4" aria-hidden="true" />
                Reveal in Explorer
              </Button>
              {selectedInCloud && cloud ? (
                <Button variant="secondary" className="w-full" onClick={() => setFreeUpOpen(true)}>
                  <CloudUpload className="size-4" aria-hidden="true" />
                  Free up space
                </Button>
              ) : null}
            </div>
          }
        >
          <div>
            <DetailRow label="Size"><span className="tabular">{formatBytes(selected.bytes)}</span></DetailRow>
            <DetailRow label="Type">{selected.kind === "folder" ? "Folder" : "File"}</DetailRow>
            <DetailRow label="Category"><CategoryBadge category={selected.category} /></DetailRow>
          </div>
          {mediaInfo && (mediaInfo.width || mediaInfo.height || mediaInfo.make || mediaInfo.model || mediaInfo.takenAtMs) ? (
            <div className="mt-4 border-t border-border pt-3">
              <h4 className="pb-2 text-[12px] font-semibold text-foreground">Media</h4>
              {mediaInfo.width && mediaInfo.height ? (
                <DetailRow label="Dimensions">
                  <span className="tabular">{mediaInfo.width} × {mediaInfo.height}</span>
                </DetailRow>
              ) : null}
              {mediaInfo.make || mediaInfo.model ? (
                <DetailRow label="Camera">
                  {[mediaInfo.make, mediaInfo.model].filter(Boolean).join(" ")}
                </DetailRow>
              ) : null}
              {mediaInfo.takenAtMs ? (
                <DetailRow label="Captured">{formatDate(mediaInfo.takenAtMs)}</DetailRow>
              ) : null}
            </div>
          ) : null}
          {classification ? (
            <div className="mt-4 border-t border-border pt-3">
              <h4 className="pb-2 text-[12px] font-semibold text-foreground">Classification</h4>
              <ClassificationCard classification={classification} />
            </div>
          ) : null}
          {selectedRec ? (
            <div className="mt-4 border-t border-border pt-3">
              <h4 className="pb-2 text-[12px] font-semibold text-foreground">Cleanup guidance</h4>
              <TierBadge tier={selectedRec.tier} />
              <p className="mt-2 text-[12px] leading-4 text-muted">{selectedRec.whySafe}</p>
              <p className="mt-1 text-[12px] leading-4 text-muted">{selectedRec.risks}</p>
            </div>
          ) : null}
        </DetailsPanel>
      ) : null}

      {cloud && selected && selectedInCloud ? (
        <FreeUpSpaceDialog
          open={freeUpOpen}
          provider={cloud.provider}
          paths={[selected.path]}
          bytes={selected.bytes}
          onClose={() => setFreeUpOpen(false)}
          onDone={(result) => {
            if (result.failed.length > 0) {
              setBanner({
                kind: "error",
                message: `Could not free up space: ${result.failed[0].error}`,
              });
            } else {
              setBanner({
                kind: "success",
                message: `${selected.name} is now online-only in ${cloud.provider}. It downloads again when opened.`,
              });
            }
            void api.getCloudStatus().then(setCloud);
          }}
        />
      ) : null}
    </div>
  );
}

export function ExplorerPage() {
  const summary = useAppStore((s) => s.summary);
  if (!summary) {
    return (
      <EmptyState
        icon={Folder}
        className="h-full"
        title="No folder data yet"
        description="Run a scan first. The explorer then lets you drill into every measured folder with its classification."
      />
    );
  }
  return <ExplorerView rootPath={summary.root} />;
}
