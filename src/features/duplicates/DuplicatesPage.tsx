import { useEffect, useMemo, useState } from "react";
import { Copy, FolderOpen, ShieldCheck } from "lucide-react";
import * as api from "@/lib/api";
import { useAppStore } from "@/lib/store";
import { displayPath, formatBytes, formatDate } from "@/lib/format";
import type { CleanupFailure, CleanupResult, DuplicateGroup } from "@/lib/types";
import { PageHeader } from "@/components/layout/PageHeader";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/input";
import { Dialog } from "@/components/ui/dialog";
import { Section } from "@/components/ui/section";
import { SkeletonRows } from "@/components/ui/skeleton";
import { EmptyState } from "@/components/shared/EmptyState";
import { MarkerTag } from "@/components/shared/MarkerTag";

/**
 * Keeper model: for every group the user picks exactly one keeper, or leaves
 * the group untouched. Every other copy can be selected for cleanup, but one
 * copy always stays — the UI makes it impossible to select the keeper.
 * Copies are re-verified byte-for-byte by the engine just before anything
 * moves to the Recycle Bin.
 */

interface ReviewGroup {
  group: DuplicateGroup;
  keeper: string;
  paths: string[];
  bytes: number;
}

export function DuplicatesPage() {
  const homeDir = useAppStore((s) => s.env?.homeDir);
  const setBanner = useAppStore((s) => s.setBanner);
  const [groups, setGroups] = useState<DuplicateGroup[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [keepers, setKeepers] = useState<Record<string, string>>({});
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [reviewOpen, setReviewOpen] = useState(false);
  const [running, setRunning] = useState(false);
  const [runError, setRunError] = useState<string | null>(null);
  const [failures, setFailures] = useState<CleanupFailure[]>([]);
  const [revealError, setRevealError] = useState<string | null>(null);

  const loadGroups = async () => {
    const rows = await api.findDuplicates();
    setGroups(rows);
    setKeepers((prev) => {
      const next: Record<string, string> = {};
      for (const group of rows) {
        const existing = prev[group.hash];
        next[group.hash] =
          existing && group.files.some((f) => f.path === existing)
            ? existing
            : (group.files[0]?.path ?? "");
      }
      return next;
    });
    return rows;
  };

  useEffect(() => {
    let live = true;
    api
      .findDuplicates()
      .then((rows) => {
        if (!live) return;
        setGroups(rows);
        const defaults: Record<string, string> = {};
        for (const group of rows) defaults[group.hash] = group.files[0]?.path ?? "";
        setKeepers(defaults);
      })
      .catch((err: unknown) => {
        if (live) setError(err instanceof Error ? err.message : "Duplicates could not be loaded.");
      });
    return () => {
      live = false;
    };
  }, []);

  const wastedTotal = useMemo(
    () => (groups ?? []).reduce((acc, g) => acc + g.size * Math.max(0, g.files.length - 1), 0),
    [groups],
  );

  const selectedBytes = useMemo(() => {
    if (!groups) return 0;
    let total = 0;
    for (const group of groups) {
      for (const file of group.files) if (selected.has(file.path)) total += file.bytes;
    }
    return total;
  }, [groups, selected]);

  /** Selected paths grouped by duplicate group, with each group's keeper. */
  const reviewGroups = useMemo<ReviewGroup[]>(() => {
    if (!groups) return [];
    const out: ReviewGroup[] = [];
    for (const group of groups) {
      const paths = group.files.filter((f) => selected.has(f.path)).map((f) => f.path);
      if (paths.length === 0) continue;
      out.push({
        group,
        keeper: keepers[group.hash] ?? group.files[0]?.path ?? "",
        paths,
        bytes: paths.length * group.size,
      });
    }
    return out;
  }, [groups, selected, keepers]);

  const chooseKeeper = (group: DuplicateGroup, path: string) => {
    setKeepers((prev) => ({ ...prev, [group.hash]: path }));
    setSelected((prev) => {
      if (!prev.has(path)) return prev;
      const next = new Set(prev);
      next.delete(path);
      return next;
    });
  };

  const toggleCopy = (path: string, checked: boolean) => {
    setSelected((prev) => {
      const next = new Set(prev);
      if (checked) next.add(path);
      else next.delete(path);
      return next;
    });
  };

  const openReview = () => {
    setRunError(null);
    setFailures([]);
    setReviewOpen(true);
  };

  const confirmCleanup = async () => {
    setRunning(true);
    setRunError(null);
    setFailures([]);
    try {
      const results: CleanupResult[] = [];
      for (const reviewGroup of reviewGroups) {
        results.push(
          await api.removeDuplicateCopies(
            reviewGroup.group.hash,
            reviewGroup.keeper,
            reviewGroup.paths,
          ),
        );
      }
      const totalReclaimed = results.reduce((acc, r) => acc + r.reclaimedBytes, 0);
      const allFailures = results.flatMap((r) => r.failed);
      setFailures(allFailures);
      setSelected(new Set());
      setReviewOpen(false);
      setBanner({
        kind: allFailures.length > 0 ? "info" : "success",
        message:
          allFailures.length > 0
            ? `Duplicate cleanup finished. ${formatBytes(totalReclaimed)} reclaimed; ${allFailures.length} cop${allFailures.length === 1 ? "y was" : "ies were"} skipped — see the details on this page.`
            : `Duplicate cleanup complete. ${formatBytes(totalReclaimed)} reclaimed.`,
      });
      await loadGroups();
    } catch (err) {
      setRunError(err instanceof Error ? err.message : "Duplicate cleanup could not be completed.");
    } finally {
      setRunning(false);
    }
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
          title="Duplicates"
          subtitle="Files with identical size are confirmed with SHA-256 before they are grouped. Hidden folders, AppData, dependency folders, and package caches are skipped; hard links count once."
          showScanControl={false}
          actions={
            <Button variant="primary" disabled={selected.size === 0} onClick={openReview}>
              <ShieldCheck className="size-4" aria-hidden="true" />
              Review cleanup{selected.size > 0 ? ` · ${formatBytes(selectedBytes)}` : ""}
            </Button>
          }
        />
        {error ? <p role="alert" className="pb-3 text-[12px] text-critical">{error}</p> : null}
        {revealError ? <p role="alert" className="pb-3 text-[12px] text-critical">{revealError}</p> : null}
        {failures.length > 0 ? (
          <div className="mb-3 rounded-[10px] border border-border bg-panel-raised px-4 py-3" role="alert">
            <p className="text-[12px] font-medium text-foreground">
              {failures.length} selected cop{failures.length === 1 ? "y was" : "ies were"} not moved:
            </p>
            <ul className="mt-1 space-y-0.5">
              {failures.slice(0, 8).map((failure, i) => (
                <li key={i} className="text-[12px] text-muted">
                  <span className="mono-path text-foreground">{displayPath(failure.path, homeDir)}</span>
                  {" — "}
                  {failure.message}
                </li>
              ))}
              {failures.length > 8 ? (
                <li className="text-[12px] text-muted">and {failures.length - 8} more</li>
              ) : null}
            </ul>
          </div>
        ) : null}

        {groups === null ? (
          <SkeletonRows rows={6} />
        ) : groups.length === 0 ? (
          <EmptyState
            icon={Copy}
            className="min-h-[360px]"
            title="No duplicates found"
            description="Duplicate groups appear here after a scan — files verified identical by hash, with the space they waste. Run a scan if you have not yet."
          />
        ) : (
          <>
            <p className="pb-2 text-[12px] text-muted">
              {groups.length} groups · up to <span className="font-medium tabular text-foreground">{formatBytes(wastedTotal)}</span> wasted.
              Pick the copy to keep in each group; one copy always stays.
            </p>
            {groups.map((group) => {
              const keeper = keepers[group.hash] ?? group.files[0]?.path;
              const wasted = group.size * Math.max(0, group.files.length - 1);
              return (
                <Section
                  key={group.hash}
                  title={`${formatBytes(group.size)} each · ${group.files.length} copies`}
                  description={`SHA-256 ${group.hash.slice(0, 16)}… · ${formatBytes(wasted)} wasted`}
                >
                  <ul className="divide-y divide-border rounded-[10px] border border-border px-3">
                    {group.files.map((file) => {
                      const isKeeper = file.path === keeper;
                      const freesLittle = file.isClone || file.isHardlink;
                      return (
                        <li key={file.path} className="flex items-center gap-3 py-2">
                          <span className="w-[118px] shrink-0">
                            {isKeeper ? (
                              <MarkerTag tone="accent">Keeper</MarkerTag>
                            ) : (
                              <label className="flex cursor-pointer items-center gap-1.5 text-[12px] text-muted">
                                <Checkbox
                                  checked={selected.has(file.path)}
                                  onChange={(e) => toggleCopy(file.path, e.target.checked)}
                                  aria-label={`Select copy at ${file.path} for cleanup`}
                                />
                                Remove copy
                              </label>
                            )}
                          </span>
                          <div className="min-w-0 flex-1">
                            <p className="mono-path truncate text-foreground" title={file.path}>{displayPath(file.path, homeDir)}</p>
                            <p className="mt-0.5 flex flex-wrap items-center gap-1.5 text-[11px] text-muted">
                              {file.modifiedAtMs ? <span>{formatDate(file.modifiedAtMs)}</span> : null}
                              {file.isHardlink ? <MarkerTag>Hard link — counted once</MarkerTag> : null}
                              {file.isClone ? <MarkerTag tone="warn">Filesystem clone — removing frees little</MarkerTag> : null}
                              {freesLittle && !isKeeper ? <span>Removing this copy frees little or no space.</span> : null}
                            </p>
                          </div>
                          {!isKeeper ? (
                            <Button variant="ghost" size="sm" onClick={() => chooseKeeper(group, file.path)}>
                              Keep this one
                            </Button>
                          ) : null}
                          <Button variant="ghost" size="sm" onClick={() => void reveal(file.path)}>
                            <FolderOpen className="size-3.5" aria-hidden="true" />
                            Reveal
                          </Button>
                        </li>
                      );
                    })}
                  </ul>
                </Section>
              );
            })}
            <p className="mt-4 text-[12px] leading-4 text-muted">
              Every selected copy is re-verified byte-for-byte against the keeper immediately before it moves
              to the Recycle Bin. If a file changed since the scan, it is skipped.
            </p>
          </>
        )}
      </div>

      {reviewOpen ? (
        <Dialog
          open
          onClose={() => setReviewOpen(false)}
          title="Review duplicate cleanup"
          description="Exactly these copies will move to the Recycle Bin — nothing else. The keeper in each group always stays."
          wide
          dismissible={!running}
          footer={
            <>
              <span className="mr-auto text-[12px] tabular text-muted">
                Total {formatBytes(selectedBytes)} · {selected.size} cop{selected.size === 1 ? "y" : "ies"}
              </span>
              <Button variant="secondary" onClick={() => setReviewOpen(false)} disabled={running}>Cancel</Button>
              <Button variant="primary" onClick={() => void confirmCleanup()} disabled={running || reviewGroups.length === 0}>
                {running ? "Working…" : "Move to Recycle Bin"}
              </Button>
            </>
          }
        >
          {runError ? <p role="alert" className="mb-3 text-[12px] text-critical">{runError}</p> : null}
          <div className="max-h-[320px] space-y-4 overflow-y-auto">
            {reviewGroups.map(({ group, keeper, paths, bytes }) => (
              <div key={group.hash}>
                <h3 className="pb-1 text-[12px] font-semibold text-foreground">
                  {formatBytes(group.size)} each · SHA-256 {group.hash.slice(0, 16)}… · {formatBytes(bytes)} to reclaim
                </h3>
                <p className="pb-1 text-[12px] text-muted">
                  Keeper:{" "}
                  <span className="mono-path text-foreground" title={keeper}>{displayPath(keeper, homeDir)}</span>
                </p>
                <ul className="space-y-1 rounded-md border border-border bg-panel p-2">
                  {paths.map((path) => (
                    <li key={path} className="flex items-center justify-between gap-3 rounded px-1.5 py-1">
                      <span className="mono-path min-w-0 truncate text-foreground" title={path}>{displayPath(path, homeDir)}</span>
                      <span className="shrink-0 text-[12px] tabular text-muted">{formatBytes(group.size)}</span>
                    </li>
                  ))}
                </ul>
              </div>
            ))}
          </div>
          <p className="mt-3 text-[12px] leading-4 text-muted">
            Each copy is re-verified byte-for-byte against the keeper before moving. A copy whose
            contents changed since the scan is skipped and reported, never moved.
          </p>
        </Dialog>
      ) : null}
    </div>
  );
}
