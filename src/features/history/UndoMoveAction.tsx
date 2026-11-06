import { useState } from "react";
import { CheckCircle2, Undo2 } from "lucide-react";
import * as api from "@/lib/api";
import { useAppStore } from "@/lib/store";
import { displayPath } from "@/lib/format";
import type { HistoryEntry, RelocateResult } from "@/lib/types";
import { Button } from "@/components/ui/button";
import { Dialog } from "@/components/ui/dialog";

/**
 * Undo for one folder relocation, living in the History entry's
 * detail area like RestoreRunAction: the confirm names both
 * locations in a dialog (a move is heavier than a restore), the
 * outcome stays visible in the entry after the list refreshes,
 * and the result is held by the parent so the refresh cannot
 * unmount it.
 */

type Phase = "idle" | "confirming" | "undoing";

export function UndoMoveResultBanner({ result }: { result: RelocateResult }) {
  return (
    <div className="rounded-[10px] border border-border bg-panel-raised px-4 py-3">
      <p className="flex items-center gap-2 text-[12px] leading-4 text-foreground">
        <CheckCircle2 className="size-4 shrink-0 text-tier-safe" aria-hidden="true" />
        Move undone — the folder is back at its original location.
      </p>
      <p className="mono-path mt-1 truncate pl-6 text-subtle" title={result.source}>
        {result.source}
      </p>
    </div>
  );
}

export function UndoMoveAction({
  entry,
  result,
  onCompleted,
}: {
  entry: HistoryEntry;
  result: RelocateResult | null;
  onCompleted: (result: RelocateResult) => void;
}) {
  const homeDir = useAppStore((s) => s.env?.homeDir);
  const [phase, setPhase] = useState<Phase>("idle");
  const [error, setError] = useState<string | null>(null);
  const [source, target] = entry.paths;

  if (result) return <UndoMoveResultBanner result={result} />;
  if (!source || !target) return null;

  const run = async () => {
    setPhase("undoing");
    setError(null);
    try {
      const undone = await api.undoRelocation(entry.id);
      onCompleted(undone);
    } catch (err) {
      /* The dialog stays open with the reason — the same failure
         treatment as the cleanup dialog. The error lives ONLY in
         the dialog: rendering it under the button as well would
         announce it twice, and once the dialog closes the entry
         itself (with its refreshed record) is the surface. */
      setError(err instanceof Error ? err.message : "The move could not be undone.");
      setPhase("confirming");
    }
  };

  const close = () => {
    setPhase("idle");
    setError(null);
  };

  return (
    <div>
      <Button
        variant="secondary"
        size="sm"
        disabled={phase === "undoing"}
        onClick={() => setPhase("confirming")}
      >
        <Undo2 className="size-3.5" aria-hidden="true" />
        {phase === "undoing" ? "Moving back…" : "Undo move"}
      </Button>

      {phase !== "idle" ? (
        <Dialog
          open
          onClose={close}
          title="Undo this move?"
          description="The folder's contents move back to the original location, and the link left there is removed once everything is back. Apps keep working throughout."
          dismissible={phase !== "undoing"}
          footer={
            <>
              <Button
                variant="secondary"
                onClick={close}
                disabled={phase === "undoing"}
              >
                Cancel
              </Button>
              <Button
                variant="primary"
                onClick={() => void run()}
                disabled={phase === "undoing"}
              >
                {phase === "undoing" ? "Moving back…" : "Undo move"}
              </Button>
            </>
          }
        >
          {error ? (
            <p role="alert" className="mb-3 text-[12px] text-critical">{error}</p>
          ) : null}
          <p className="text-[12px] font-medium text-foreground">Currently at</p>
          <p className="mono-path mt-0.5 truncate text-muted" title={target}>
            {displayPath(target, homeDir)}
          </p>
          <p className="mt-3 text-[12px] font-medium text-foreground">Moves back to</p>
          <p className="mono-path mt-0.5 truncate text-muted" title={source}>
            {displayPath(source, homeDir)}
          </p>
          {phase === "undoing" ? (
            <p role="status" className="mt-4 text-[12px] text-muted">
              Moving… large folders can take a while. Don&rsquo;t close DiskPilot.
            </p>
          ) : null}
        </Dialog>
      ) : null}
    </div>
  );
}
