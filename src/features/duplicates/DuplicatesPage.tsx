import { useEffect, useMemo, useState } from "react";
import { Copy, FolderOpen, Images, ShieldCheck } from "lucide-react";
import * as api from "@/lib/api";
import { useAppStore } from "@/lib/store";
import { basename, displayPath, formatBytes, formatCount, formatDate } from "@/lib/format";
import type { CleanupFailure, CleanupResult, DuplicateGroup, SimilarGroup } from "@/lib/types";
import { cn } from "@/lib/utils";
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

/**
 * Allocated bytes freed by removing every copy except the keeper —
 * mirrors the core rule (`DuplicateGroup::wasted_bytes`): per-file
 * figures are allocated bytes, and copies that share the first file's
 * inode (its hard links) free nothing while any member of that inode
 * set stays. The shared inode counts once, and only when every member
 * is being removed (the keeper always stays, so that requires the
 * keeper to be a different file).
 */
export function groupWastedBytes(group: DuplicateGroup, keeperPath: string | undefined): number {
  const keeper = keeperPath ?? group.files[0]?.path;
  const inodeSet = group.files.filter((f, i) => i === 0 || f.isHardlink);
  const inodeRemoved = inodeSet.filter((f) => f.path !== keeper);
  const inodeFreed =
    inodeSet.length > 0 && inodeRemoved.length === inodeSet.length
      ? (inodeSet[0]?.bytes ?? 0)
      : 0;
  const rest = group.files
    .filter((f, i) => i !== 0 && !f.isHardlink && f.path !== keeper)
    .reduce((acc, f) => acc + f.bytes, 0);
  return inodeFreed + rest;
}

export type DuplicatesMode = "exact" | "similar";

/** Segmented control switching the exact-copies and look-alikes
    views; same pattern as the Storage tabs. Rendered in each view's
    header actions so the page title stays on top. */
export function DuplicatesModeSwitch({
  mode,
  onChange,
}: {
  mode: DuplicatesMode;
  onChange: (mode: DuplicatesMode) => void;
}) {
  const options: { id: DuplicatesMode; label: string }[] = [
    { id: "exact", label: "Exact copies" },
    { id: "similar", label: "Look-alike photos" },
  ];
  return (
    <div role="tablist" aria-label="Duplicate views" className="flex gap-1 rounded-md bg-panel-raised p-1">
      {options.map((option) => (
        <button
          key={option.id}
          role="tab"
          aria-selected={mode === option.id}
          onClick={() => onChange(option.id)}
          className={cn(
            "h-6 rounded-[5px] px-3 text-[12px] font-medium transition-colors duration-120",
            mode === option.id
              ? "bg-panel text-foreground shadow-[0_1px_3px_rgba(0,0,0,0.14)] ring-1 ring-border"
              : "text-muted hover:text-foreground",
          )}
        >
          {option.label}
        </button>
      ))}
    </div>
  );
}

function ExactDuplicatesView({
  mode,
  onModeChange,
}: {
  mode: DuplicatesMode;
  onModeChange: (mode: DuplicatesMode) => void;
}) {
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
    () =>
      (groups ?? []).reduce(
        (acc, g) => acc + groupWastedBytes(g, keepers[g.hash] ?? g.files[0]?.path),
        0,
      ),
    [groups, keepers],
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
        bytes: paths.reduce(
          (acc, p) => acc + (group.files.find((f) => f.path === p)?.bytes ?? 0),
          0,
        ),
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
            <>
              <DuplicatesModeSwitch mode={mode} onChange={onModeChange} />
              <Button variant="primary" disabled={selected.size === 0} onClick={openReview}>
                <ShieldCheck className="size-4" aria-hidden="true" />
                Review cleanup{selected.size > 0 ? ` · ${formatBytes(selectedBytes)}` : ""}
              </Button>
            </>
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
              const wasted = groupWastedBytes(group, keeper);
              return (
                <Section
                  key={group.hash}
                  title={`${group.files.length} identical copies`}
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

/**
 * Look-alike photos: groups the core found by perceptual hashing.
 * Deliberately READ-ONLY. Similar is not identical — there is no
 * byte-for-byte proof to re-verify before a removal, so this view
 * never offers one; the exact-copies flow is the only duplicate
 * cleanup path. The keeper (the core returns files largest first;
 * the first entry is its designated keeper) is marked for guidance,
 * and every row can be revealed in Explorer.
 */
function SimilarImagesView({
  mode,
  onModeChange,
}: {
  mode: DuplicatesMode;
  onModeChange: (mode: DuplicatesMode) => void;
}) {
  const homeDir = useAppStore((s) => s.env?.homeDir);
  const [groups, setGroups] = useState<SimilarGroup[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [revealError, setRevealError] = useState<string | null>(null);

  useEffect(() => {
    let live = true;
    api
      .getSimilarImages()
      .then((rows) => {
        if (live) setGroups(rows);
      })
      .catch((err: unknown) => {
        if (live) setError(err instanceof Error ? err.message : "Look-alike photos could not be loaded.");
      });
    return () => {
      live = false;
    };
  }, []);

  const wastedTotal = useMemo(
    () => (groups ?? []).reduce((acc, g) => acc + g.wastedBytes, 0),
    [groups],
  );

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
          subtitle="Photos that look alike — re-saves, resized copies, and near-identical shots — grouped by comparing image content rather than bytes."
          showScanControl={false}
          actions={<DuplicatesModeSwitch mode={mode} onChange={onModeChange} />}
        />
        {error ? <p role="alert" className="pb-3 text-[12px] text-critical">{error}</p> : null}
        {revealError ? <p role="alert" className="pb-3 text-[12px] text-critical">{revealError}</p> : null}

        {groups === null ? (
          <SkeletonRows rows={6} />
        ) : groups.length === 0 ? (
          <EmptyState
            icon={Images}
            className="min-h-[360px]"
            title="No look-alike photos"
            description="Groups of near-identical photos under the scanned folder appear here — resized copies, re-saves, and burst neighbours. Run a scan if you have not yet."
          />
        ) : (
          <>
            <p className="pb-2 text-[12px] text-muted">
              {groups.length} group{groups.length === 1 ? "" : "s"} · up to{" "}
              <span className="font-medium tabular text-foreground">{formatBytes(wastedTotal)}</span>{" "}
              in extra copies. The largest copy in each group is marked as the keeper.
            </p>
            {groups.map((group, groupIndex) => (
              <Section
                key={group.files[0]?.path ?? groupIndex}
                title={`${group.files.length} look-alike photos`}
                description={`Up to ${formatBytes(group.wastedBytes)} in extra copies`}
              >
                <ul className="divide-y divide-border rounded-[10px] border border-border px-3">
                  {group.files.map((file, fileIndex) => {
                    const isKeeper = fileIndex === 0;
                    return (
                      <li key={file.path} className="flex items-center gap-3 py-2">
                        <span className="w-[64px] shrink-0">
                          {isKeeper ? <MarkerTag tone="accent">Keeper</MarkerTag> : null}
                        </span>
                        <div className="min-w-0 flex-1">
                          <p className="truncate text-[13px] text-foreground" title={file.path}>
                            {basename(file.path)}
                          </p>
                          <p className="mono-path mt-0.5 truncate text-subtle" title={file.path}>
                            {displayPath(file.path, homeDir)}
                          </p>
                        </div>
                        <span className="shrink-0 text-[12px] tabular text-muted">
                          {formatCount(file.width)} × {formatCount(file.height)}
                        </span>
                        <span className="w-[92px] shrink-0 text-right text-[13px] tabular text-foreground">
                          {formatBytes(file.bytes)}
                        </span>
                        <Button variant="ghost" size="sm" onClick={() => void reveal(file.path)}>
                          <FolderOpen className="size-3.5" aria-hidden="true" />
                          Reveal
                        </Button>
                      </li>
                    );
                  })}
                </ul>
              </Section>
            ))}
            <p className="mt-4 text-[12px] leading-4 text-muted">
              Look-alike photos are shown for review only. They are similar, not identical, so
              DiskPilot does not offer to remove them here — reveal a copy in Explorer to manage
              it yourself. Verified identical files are on the Exact copies tab, where every
              copy is re-verified byte-for-byte before anything moves.
            </p>
          </>
        )}
      </div>
    </div>
  );
}

export function DuplicatesPage() {
  const [mode, setMode] = useState<DuplicatesMode>("exact");
  return mode === "exact" ? (
    <ExactDuplicatesView mode={mode} onModeChange={setMode} />
  ) : (
    <SimilarImagesView mode={mode} onModeChange={setMode} />
  );
}
