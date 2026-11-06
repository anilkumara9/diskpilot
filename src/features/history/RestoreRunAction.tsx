import { useState } from "react";
import { AlertTriangle, CheckCircle2, Undo2 } from "lucide-react";
import * as api from "@/lib/api";
import type { HistoryEntry, RestoreCleanupRunResult } from "@/lib/types";
import { Button } from "@/components/ui/button";

/**
 * Undo for one cleanup run, living in the entry's detail area.
 * The action only exists while the shell says some of the run's
 * Recycle Bin items can still be restored; the confirm is inline
 * (no dialog — the stakes are spelled out where the button was),
 * and the outcome stays visible in the entry after the list
 * refreshes and the action itself disappears. The result is held
 * by the parent so the refresh cannot unmount it.
 */

type Phase = "idle" | "confirming" | "restoring";

function items(count: number): string {
  return `${count} item${count === 1 ? "" : "s"}`;
}

export function RestoreResultBanner({ result }: { result: RestoreCleanupRunResult }) {
  /* Zero restored is not a success and must not wear its colours:
     the banner leads with what actually happened and the reason. */
  const nothingRestored = result.restored.length === 0;
  return (
    <div className="rounded-[10px] border border-border bg-panel-raised px-4 py-3">
      {nothingRestored ? (
        <p className="flex items-center gap-2 text-[12px] leading-4 text-foreground">
          <AlertTriangle className="size-4 shrink-0 text-tier-review" aria-hidden="true" />
          Nothing could be restored
          {result.failed.length > 0 ? <> — {result.failed[0].error}</> : null}
          {result.failed.length === 0 && result.skippedPermanent > 0 ? (
            <> — the run&rsquo;s items were permanently deleted.</>
          ) : null}
        </p>
      ) : (
        <p className="flex items-center gap-2 text-[12px] leading-4 text-foreground">
          <CheckCircle2 className="size-4 shrink-0 text-tier-safe" aria-hidden="true" />
          Restored {items(result.restored.length)} to their original locations.
        </p>
      )}
      {!nothingRestored && result.failed.length > 0 ? (
        <p className="mt-1 pl-6 text-[12px] leading-4 text-critical">
          {result.failed.length} couldn&rsquo;t be restored — {result.failed[0].error}
        </p>
      ) : null}
      {result.skippedPermanent > 0 && !(nothingRestored && result.failed.length === 0) ? (
        <p className="mt-1 pl-6 text-[12px] leading-4 text-muted">
          {items(result.skippedPermanent)} were permanently deleted and can&rsquo;t be restored.
        </p>
      ) : null}
    </div>
  );
}

export function RestoreRunAction({
  entry,
  result,
  onCompleted,
}: {
  entry: HistoryEntry;
  result: RestoreCleanupRunResult | null;
  onCompleted: (result: RestoreCleanupRunResult) => void;
}) {
  const [phase, setPhase] = useState<Phase>("idle");
  const [error, setError] = useState<string | null>(null);
  const count = entry.restorableCount ?? 0;

  if (result) return <RestoreResultBanner result={result} />;
  if (count <= 0) return null;

  const run = async () => {
    setPhase("restoring");
    setError(null);
    try {
      const restored = await api.restoreCleanupRun(String(entry.id));
      onCompleted(restored);
    } catch (err) {
      setError(err instanceof Error ? err.message : "The items could not be restored.");
      setPhase("idle");
    }
  };

  return (
    <div>
      {phase === "confirming" ? (
        <div className="flex flex-wrap items-center gap-2">
          <span className="text-[12px] text-foreground">
            Restore {items(count)} to their original locations?
          </span>
          <Button variant="primary" size="sm" onClick={() => void run()}>
            Restore
          </Button>
          <Button variant="ghost" size="sm" onClick={() => setPhase("idle")}>
            Cancel
          </Button>
        </div>
      ) : (
        <Button
          variant="secondary"
          size="sm"
          disabled={phase === "restoring"}
          onClick={() => setPhase("confirming")}
        >
          <Undo2 className="size-3.5" aria-hidden="true" />
          {phase === "restoring" ? "Restoring…" : `Restore ${items(count)}`}
        </Button>
      )}
      {error ? (
        <p role="alert" className="mt-1 text-[12px] text-critical">
          {error}
        </p>
      ) : null}
    </div>
  );
}
