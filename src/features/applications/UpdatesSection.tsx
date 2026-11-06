import { useState } from "react";
import { CheckCircle2, RefreshCw } from "lucide-react";
import * as api from "@/lib/api";
import { useAppStore } from "@/lib/store";
import {
  actionableIds,
  anyRunning,
  markRunning,
  mergeOutcomes,
  outstandingUpdateCount,
  PENDING,
  type UpdateRowState,
} from "@/lib/updates";
import type { AvailableUpdates, SoftwareUpdateOutcome } from "@/lib/types";
import { Button } from "@/components/ui/button";
import { Section } from "@/components/ui/section";
import { SkeletonRows } from "@/components/ui/skeleton";

/**
 * Software updates via the Windows Package Manager (winget).
 * Checking changes nothing on the PC; updating runs winget per
 * application and every row wears its own outcome — a failure is
 * shown with winget's reason under the row, never folded into a
 * generic "done". After a run, "Check again" re-reads the list
 * rather than trusting the outcomes to describe the new state.
 */
export function UpdatesSection() {
  const [phase, setPhase] = useState<"idle" | "checking" | "done">("idle");
  const [report, setReport] = useState<AvailableUpdates | null>(null);
  const [checkError, setCheckError] = useState<string | null>(null);
  const [rowStates, setRowStates] = useState<Record<string, UpdateRowState>>({});
  const [hasRun, setHasRun] = useState(false);
  const setSoftwareUpdatesBadge = useAppStore((s) => s.setSoftwareUpdatesBadge);

  const check = async () => {
    setPhase("checking");
    setCheckError(null);
    setHasRun(false);
    setRowStates({});
    try {
      const result = await api.listAvailableUpdates();
      setReport(result);
      /* The Applications nav badge remembers this completed
         check (count + when) across restarts. A failed check
         leaves the previous memory untouched — it still
         describes the last check that actually completed. */
      setSoftwareUpdatesBadge({
        count: result.available ? result.updates.length : 0,
        checkedAtMs: Date.now(),
      });
    } catch (err) {
      setReport(null);
      setCheckError(err instanceof Error ? err.message : "The update check failed.");
    } finally {
      setPhase("done");
    }
  };

  const runUpdate = async (ids: string[]) => {
    if (ids.length === 0) return;
    const marked = markRunning(rowStates, ids);
    setRowStates(marked);
    let outcomes: SoftwareUpdateOutcome[];
    try {
      /* An empty id list means "everything outstanding" — the
         shell expands it, so Update all is one winget queue. */
      outcomes = await api.updateSoftware(ids.length === 1 ? ids : []);
    } catch (err) {
      const message = err instanceof Error ? err.message : "The update could not be run.";
      outcomes = ids.map((id) => ({ id, success: false, message }));
    }
    const merged = mergeOutcomes(marked, outcomes, ids);
    setRowStates(merged);
    /* The badge count follows the run: rows that updated stop
       counting, rows that failed (or were never attempted) keep
       counting. The check timestamp stays the last check's —
       the run freshened the count, not the check. */
    const badge = useAppStore.getState().softwareUpdatesBadge;
    setSoftwareUpdatesBadge({
      count: outstandingUpdateCount(report?.updates ?? [], merged),
      checkedAtMs: badge?.checkedAtMs ?? Date.now(),
    });
    setHasRun(true);
  };

  const updates = report?.updates ?? [];
  const running = anyRunning(rowStates);
  const pendingIds = actionableIds(rowStates, updates.map((u) => u.id));

  return (
    <Section
      title="Updates"
      description="Outdated applications, checked and updated through the Windows Package Manager (winget)."
      action={
        phase === "done" && report?.available && updates.length > 0 ? (
          <span className="flex items-center gap-1">
            {pendingIds.length > 1 ? (
              <Button
                variant="secondary"
                size="sm"
                disabled={running}
                onClick={() => void runUpdate(pendingIds)}
              >
                Update all
              </Button>
            ) : null}
            {hasRun ? (
              <Button variant="subtle" size="sm" disabled={running} onClick={() => void check()}>
                <RefreshCw className="size-3.5" aria-hidden="true" />
                Check again
              </Button>
            ) : null}
          </span>
        ) : undefined
      }
    >
      {phase === "idle" ? (
        <div className="flex flex-wrap items-center gap-3">
          <p className="min-w-0 flex-1 text-[12px] leading-4 text-muted">
            Checking only asks winget what is outdated — nothing is installed or changed.
          </p>
          <Button variant="secondary" onClick={() => void check()}>
            Check for updates
          </Button>
        </div>
      ) : phase === "checking" ? (
        <div>
          <p className="pb-2 text-[12px] text-muted">Checking with winget…</p>
          <SkeletonRows rows={3} />
        </div>
      ) : checkError ? (
        <div className="flex items-center gap-3 rounded-[10px] border border-border bg-panel-raised px-4 py-3">
          <p role="alert" className="flex-1 text-[12px] leading-4 text-critical">{checkError}</p>
          <Button variant="secondary" size="sm" onClick={() => void check()}>
            Retry
          </Button>
        </div>
      ) : report && !report.available ? (
        <p className="rounded-[10px] border border-border px-4 py-3 text-[12px] leading-4 text-muted">
          Windows Package Manager (winget) isn&rsquo;t available on this PC.
        </p>
      ) : (
        <>
          {report?.error ? (
            <div className="mb-3 flex items-center gap-3 rounded-[10px] border border-border bg-panel-raised px-4 py-3">
              <p role="alert" className="flex-1 text-[12px] leading-4 text-critical">
                {report.error}
              </p>
              <Button variant="secondary" size="sm" onClick={() => void check()}>
                Retry
              </Button>
            </div>
          ) : null}
          {updates.length === 0 ? (
            /* "Up to date" is only earned by a clean check: when
               winget also reported an error, the error banner
               above stands alone. */
            report?.error ? null : (
              <p className="flex items-center gap-2 text-[12px] leading-4 text-foreground">
                <CheckCircle2 className="size-4 shrink-0 text-tier-safe" aria-hidden="true" />
                Everything is up to date.
              </p>
            )
          ) : (
            <>
              {updates.length > 3 ? (
                <p className="pb-2 text-right text-[12px] text-muted">
                  Updating many apps can take a while — each app updates one at a time.
                </p>
              ) : null}
              <ul className="divide-y divide-border rounded-[10px] border border-border px-3">
              {updates.map((update) => {
                const state = rowStates[update.id] ?? PENDING;
                return (
                  <li key={update.id} className="py-2">
                    <div className="flex items-center gap-3">
                      <div className="min-w-0 flex-1">
                        <p className="truncate text-[13px] font-medium text-foreground" title={update.id}>
                          {update.name}
                        </p>
                        <p className="mt-0.5 text-[12px] tabular text-muted">
                          {update.currentVersion} → {update.availableVersion}
                        </p>
                      </div>
                      {state.phase === "updated" ? (
                        <span className="flex shrink-0 items-center gap-1.5 text-[12px] font-medium text-tier-safe">
                          <CheckCircle2 className="size-4" aria-hidden="true" />
                          Updated
                        </span>
                      ) : state.phase === "running" ? (
                        <span className="shrink-0 text-[12px] text-muted">Updating…</span>
                      ) : (
                        <Button
                          variant="secondary"
                          size="sm"
                          disabled={running}
                          onClick={() => void runUpdate([update.id])}
                        >
                          Update
                        </Button>
                      )}
                    </div>
                    {state.phase === "failed" ? (
                      <p role="alert" className="pt-1 text-[12px] text-critical">
                        {state.message}
                      </p>
                    ) : null}
                  </li>
                );
              })}
              </ul>
            </>
          )}
        </>
      )}
    </Section>
  );
}
