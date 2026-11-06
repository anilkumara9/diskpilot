import { useCallback, useEffect, useState } from "react";
import { AlertTriangle, CheckCircle2, Trash2, Undo2 } from "lucide-react";
import * as api from "@/lib/api";
import { useAppStore } from "@/lib/store";
import { displayPath, formatBytes, formatDate } from "@/lib/format";
import type { CleanupResult, EmptyRecycleBinProgress, RecycleBinItem } from "@/lib/types";
import { Button } from "@/components/ui/button";
import { Dialog } from "@/components/ui/dialog";
import { SkeletonRows } from "@/components/ui/skeleton";

/**
 * Empty Recycle Bin flow: the contents are listed first, then exactly those
 * items are removed by the shell with live progress and a Stop button.
 * The shell re-validates each item and reports a CleanupResult.
 */
export function EmptyRecycleBinDialog({ open, onClose }: { open: boolean; onClose: () => void }) {
  const homeDir = useAppStore((s) => s.env?.homeDir);
  const setBanner = useAppStore((s) => s.setBanner);
  const refreshDisk = useAppStore((s) => s.refreshDisk);
  const [items, setItems] = useState<RecycleBinItem[] | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [running, setRunning] = useState(false);
  const [progress, setProgress] = useState<EmptyRecycleBinProgress | null>(null);
  const [result, setResult] = useState<CleanupResult | null>(null);
  const [runError, setRunError] = useState<string | null>(null);
  const [restoringId, setRestoringId] = useState<string | null>(null);
  const [restoreError, setRestoreError] = useState<string | null>(null);
  const [confirmArmed, setConfirmArmed] = useState(false);

  const load = useCallback(async () => {
    setItems(null);
    setLoadError(null);
    setProgress(null);
    setResult(null);
    setRunError(null);
    setRestoreError(null);
    setConfirmArmed(false);
    try {
      setItems(await api.getRecycleBinItems());
    } catch (err) {
      setLoadError(err instanceof Error ? err.message : "Recycle Bin contents could not be read.");
    }
  }, []);

  useEffect(() => {
    if (open) void load();
  }, [open, load]);

  /* The armed "click again" state disarms itself after a few seconds
     so a stray first click can never linger as consent. */
  useEffect(() => {
    if (!confirmArmed) return;
    const timer = window.setTimeout(() => setConfirmArmed(false), 4000);
    return () => window.clearTimeout(timer);
  }, [confirmArmed]);

  const restore = async (item: RecycleBinItem) => {
    setRestoringId(item.id);
    setRestoreError(null);
    try {
      await api.restoreRecycleItem(item.id);
      await load();
    } catch (err) {
      setRestoreError(err instanceof Error ? err.message : "The item could not be restored.");
    } finally {
      setRestoringId(null);
    }
  };

  const totalBytes = (items ?? []).reduce((acc, item) => acc + item.bytes, 0);

  const start = async () => {
    if (!items || items.length === 0) return;
    setRunning(true);
    setRunError(null);
    setResult(null);
    setProgress(null);
    const unlisten = await api.onRecycleBinProgress((p) => {
      setProgress(p);
    });
    try {
      const cleanup = await api.emptyRecycleBin(items);
      setResult(cleanup);
      await refreshDisk();
      if (cleanup.failed.length === 0) {
        setBanner({
          kind: "success",
          message: `Recycle Bin emptied. ${formatBytes(cleanup.reclaimedBytes)} freed.`,
        });
      }
    } catch (err) {
      setRunError(err instanceof Error ? err.message : "The Recycle Bin could not be emptied.");
    } finally {
      unlisten();
      setRunning(false);
    }
  };

  const stop = () => {
    void api.cancelEmptyRecycleBin();
  };

  const finished = result != null;
  const pct =
    progress && progress.total > 0 ? Math.round((progress.completed / progress.total) * 100) : 0;

  return (
    <Dialog
      open={open}
      onClose={onClose}
      title="Empty Recycle Bin"
      description="These are the current contents of the Recycle Bin. Exactly these items will be permanently removed — this cannot be undone."
      dismissible={!running}
      wide
      footer={
        running ? (
          <>
            <span className="mr-auto text-[12px] tabular text-muted">
              {progress
                ? `${progress.completed} of ${progress.total} · ${formatBytes(progress.freedBytes)} freed`
                : "Working…"}
            </span>
            <Button variant="destructive" onClick={stop}>Stop</Button>
          </>
        ) : finished ? (
          <Button variant="primary" onClick={onClose}>Done</Button>
        ) : (
          <>
            <span className="mr-auto text-[12px] tabular text-muted">
              {items ? `${items.length} item${items.length === 1 ? "" : "s"} · ${formatBytes(totalBytes)}` : ""}
            </span>
            <Button variant="secondary" onClick={onClose}>Cancel</Button>
            <Button
              variant="destructive"
              onClick={() => {
                if (confirmArmed) {
                  setConfirmArmed(false);
                  void start();
                } else {
                  setConfirmArmed(true);
                }
              }}
              disabled={!items || items.length === 0}
            >
              <Trash2 className="size-4" aria-hidden="true" />
              {confirmArmed ? "Click again to permanently empty" : "Empty Recycle Bin"}
            </Button>
          </>
        )
      }
    >
      {loadError ? <p role="alert" className="text-[12px] text-critical">{loadError}</p> : null}
      {runError ? <p role="alert" className="text-[12px] text-critical">{runError}</p> : null}
      {restoreError ? <p role="alert" className="text-[12px] text-critical">{restoreError}</p> : null}

      {items === null && !loadError ? (
        <SkeletonRows rows={5} />
      ) : items && items.length === 0 ? (
        <p className="flex items-center gap-2 py-4 text-[12px] text-muted">
          <CheckCircle2 className="size-4 text-tier-safe" aria-hidden="true" />
          The Recycle Bin is already empty.
        </p>
      ) : items ? (
        <ul className="max-h-[260px] space-y-1 overflow-y-auto rounded-md border border-border p-2" aria-label="Recycle Bin contents">
          {items.map((item, index) => {
            const isCurrent = running && progress?.currentName === item.name;
            const isDone = progress != null && index < progress.completed;
            return (
              <li key={item.id} className="flex items-center justify-between gap-3 py-1">
                <span className="min-w-0">
                  <span className={`block truncate text-[13px] ${isDone ? "text-subtle line-through" : "text-foreground"}`} title={item.originalPath}>
                    {item.name}
                  </span>
                  <span className="mono-path block truncate text-subtle" title={item.originalPath}>
                    {displayPath(item.originalPath, homeDir)}
                  </span>
                  <span className="block text-[11px] text-subtle">Deleted {formatDate(item.deletedAtMs)}</span>
                </span>
                <span className="flex shrink-0 items-center gap-2 text-[12px] tabular text-muted">
                  {isCurrent ? <span className="text-accent">Removing…</span> : null}
                  {isDone && !isCurrent ? <CheckCircle2 className="size-3.5 text-tier-safe" aria-hidden="true" /> : null}
                  {formatBytes(item.bytes)}
                  {!running && !finished ? (
                    <Button
                      variant="ghost"
                      size="sm"
                      disabled={restoringId === item.id}
                      onClick={() => void restore(item)}
                    >
                      <Undo2 className="size-3.5" aria-hidden="true" />
                      {restoringId === item.id ? "Restoring…" : "Restore"}
                    </Button>
                  ) : null}
                </span>
              </li>
            );
          })}
        </ul>
      ) : null}

      {running || progress ? (
        <div className="mt-3">
          <div className="h-[6px] w-full overflow-hidden rounded-full bg-hover" role="progressbar" aria-valuenow={pct} aria-valuemin={0} aria-valuemax={100} aria-label="Emptying progress">
            <div className="h-full rounded-full bg-accent transition-[width] duration-200" style={{ width: `${pct}%` }} />
          </div>
          {progress?.currentName ? (
            <p className="mt-1.5 truncate text-subtle">{progress.currentName}</p>
          ) : null}
        </div>
      ) : null}

      {result ? (
        <div className="mt-3">
          <p className="text-[12px] text-muted">
            Finished — {result.succeeded.length} removed, {result.failed.length} failed,{" "}
            {formatBytes(result.reclaimedBytes)} freed.
          </p>
          {result.failed.length > 0 ? (
            <div className="mt-2">
              <p className="flex items-center gap-1.5 text-[12px] font-medium text-critical">
                <AlertTriangle className="size-3.5" aria-hidden="true" />
                Some items could not be removed
              </p>
              <ul className="mt-1 space-y-0.5">
                {result.failed.slice(0, 8).map((failure, i) => (
                  <li key={i} className="text-[12px] text-muted">
                    <span className="mono-path">{displayPath(failure.path, homeDir)}</span>
                    {failure.message ? <> — {failure.message}</> : null}
                  </li>
                ))}
              </ul>
            </div>
          ) : null}
        </div>
      ) : null}
    </Dialog>
  );
}
