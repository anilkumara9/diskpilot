import { useMemo, useState } from "react";
import {
  AlertTriangle,
  ChevronDown,
  ChevronRight,
  Command,
  ShieldCheck,
  Sparkles,
} from "lucide-react";
import { useAppStore } from "@/lib/store";
import * as api from "@/lib/api";
import { actionableBytes, groupRecommendationsByTier } from "@/lib/analysis";
import { displayPath, formatBytes, formatDate } from "@/lib/format";
import { CONFIDENCE_LABEL, tierMeta } from "@/lib/tiers";
import type { CleanupMode, CleanupPlan, Recommendation } from "@/lib/types";
import { PageHeader } from "@/components/layout/PageHeader";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/input";
import { Dialog } from "@/components/ui/dialog";
import { Section } from "@/components/ui/section";
import { CopyButton } from "@/components/shared/CopyButton";
import { EmptyState } from "@/components/shared/EmptyState";
import { TierBadge } from "@/components/shared/TierBadge";
import { cn } from "@/lib/utils";

/* ---------------- Cleanup dialog ---------------- */

export function CleanupDialog({
  plan,
  onClose,
  onDone,
}: {
  plan: CleanupPlan;
  onClose: () => void;
  onDone: (reclaimed: number) => void;
}) {
  const homeDir = useAppStore((s) => s.env?.homeDir);
  const refreshRecommendations = useAppStore((s) => s.refreshRecommendations);
  const refreshDisk = useAppStore((s) => s.refreshDisk);
  const [mode, setMode] = useState<CleanupMode>("recycleBin");
  const [running, setRunning] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const expired = Date.now() > plan.expiresAtMs;

  const execute = async () => {
    setRunning(true);
    setError(null);
    try {
      const result = await api.executeCleanupPlan(plan.id, mode);
      await Promise.all([refreshRecommendations(), refreshDisk()]);
      onDone(result.reclaimedBytes);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Cleanup could not be completed.");
      setRunning(false);
    }
  };

  return (
    <Dialog
      open
      onClose={onClose}
      title="Review cleanup plan"
      description="Exactly these items will be acted on — nothing else. Plans are single-use and expire after 10 minutes."
      wide
      dismissible={!running}
      footer={
        <>
          <span className="mr-auto text-[12px] tabular text-muted">
            Total {formatBytes(plan.totalBytes)} · expires {formatDate(plan.expiresAtMs)}
          </span>
          <Button variant="secondary" onClick={onClose} disabled={running}>Cancel</Button>
          <Button variant={mode === "permanent" ? "destructive" : "primary"} onClick={() => void execute()} disabled={running || expired}>
            {running
              ? "Working…"
              : mode === "permanent"
                ? `Permanently delete ${formatBytes(plan.totalBytes)}`
                : `Move ${formatBytes(plan.totalBytes)} to Recycle Bin`}
          </Button>
        </>
      }
    >
      {expired ? (
        <p role="alert" className="mb-3 flex items-center gap-2 rounded-md bg-critical/10 px-3 py-2 text-[12px] text-critical">
          <AlertTriangle className="size-4" aria-hidden="true" />
          This plan has expired. Close it and review the cleanup again to build a fresh plan.
        </p>
      ) : null}
      {error ? <p role="alert" className="mb-3 text-[12px] text-critical">{error}</p> : null}

      <h3 className="pb-1 text-[12px] font-semibold text-foreground">Items ({plan.items.length})</h3>
      <ul className="max-h-[220px] space-y-1 overflow-y-auto rounded-md border border-border p-2">
        {plan.items.map((item) => (
          <li key={item.path} className="flex items-center justify-between gap-3">
            <span className="mono-path min-w-0 truncate text-foreground" title={item.path}>{displayPath(item.path, homeDir)}</span>
            <span className="shrink-0 text-[12px] tabular text-muted">{formatBytes(item.bytes)}</span>
          </li>
        ))}
      </ul>

      {plan.warnings.length > 0 ? (
        <div className="mt-3">
          <h3 className="pb-1 text-[12px] font-semibold text-tier-review">Warnings</h3>
          <ul className="space-y-1">
            {plan.warnings.map((warning, i) => (
              <li key={i} className="flex gap-2 text-[12px] leading-4 text-muted">
                <AlertTriangle className="mt-0.5 size-3.5 shrink-0 text-tier-review" aria-hidden="true" />
                {warning}
              </li>
            ))}
          </ul>
        </div>
      ) : null}

      {plan.rejected.length > 0 ? (
        <div className="mt-3">
          <h3 className="pb-1 text-[12px] font-semibold text-foreground">Rejected by the safety check</h3>
          <ul className="space-y-1">
            {plan.rejected.map((path, i) => (
              <li key={i} className="mono-path text-subtle">{displayPath(path, homeDir)}</li>
            ))}
          </ul>
          <p className="mt-1 text-[12px] text-muted">Rejected items are never touched. They may be protected, outside the scanned folder, or changed since the scan.</p>
        </div>
      ) : null}

      <fieldset className="mt-4">
        <legend className="pb-1.5 text-[12px] font-semibold text-foreground">How should these be removed?</legend>
        <label className="flex cursor-pointer items-start gap-2 rounded-md border border-border px-3 py-2">
          <input type="radio" name="cleanup-mode" value="recycleBin" checked={mode === "recycleBin"} onChange={() => setMode("recycleBin")} className="mt-0.5 accent-[#5856d6]" />
          <span>
            <span className="block text-[13px] font-medium text-foreground">Move to Recycle Bin (recommended)</span>
            <span className="block text-[12px] text-muted">Recoverable until you empty the Recycle Bin. Space is freed when it is emptied.</span>
          </span>
        </label>
        <label className={cn("mt-2 flex items-start gap-2 rounded-md border border-border px-3 py-2", plan.allowPermanent ? "cursor-pointer" : "cursor-not-allowed opacity-50")}>
          <input
            type="radio"
            name="cleanup-mode"
            value="permanent"
            checked={mode === "permanent"}
            disabled={!plan.allowPermanent}
            onChange={() => setMode("permanent")}
            className="mt-0.5 accent-[#5856d6]"
          />
          <span>
            <span className="block text-[13px] font-medium text-foreground">Permanently delete</span>
            <span className="block text-[12px] text-muted">
              {plan.allowPermanent
                ? "Every selected item is regenerable, so permanent deletion is allowed. This cannot be undone."
                : "Available only when every selected item is regenerable. At least one selected item is not, so this plan must use the Recycle Bin."}
            </span>
          </span>
        </label>
      </fieldset>
    </Dialog>
  );
}

/* ---------------- Recommendation row ---------------- */

export function RecommendationRow({
  recommendation,
  checked,
  onToggle,
  highlighted,
}: {
  recommendation: Recommendation;
  checked: boolean;
  onToggle: (id: string, checked: boolean) => void;
  highlighted?: boolean;
}) {
  const homeDir = useAppStore((s) => s.env?.homeDir);
  const [expanded, setExpanded] = useState(false);
  const selectable = recommendation.tier !== "keep";
  const Icon = expanded ? ChevronDown : ChevronRight;

  return (
    <div
      className={cn(
        "border-b border-border/60 transition-colors duration-120",
        highlighted && "bg-accent/8",
      )}
    >
      <div className="flex items-center gap-3 px-1 py-2">
        {selectable ? (
          <Checkbox
            checked={checked}
            onChange={(e) => onToggle(recommendation.id, e.target.checked)}
            aria-label={`Select ${recommendation.title}`}
          />
        ) : (
          <span className="size-4 shrink-0" aria-hidden="true" />
        )}
        <button
          type="button"
          onClick={() => setExpanded((v) => !v)}
          aria-expanded={expanded}
          className="flex min-w-0 flex-1 items-center gap-2 text-left"
        >
          <Icon className="size-4 shrink-0 text-subtle" aria-hidden="true" />
          <span className="min-w-0 flex-1">
            <span className="block truncate text-[13px] font-medium text-foreground">{recommendation.title}</span>
            <span className="block truncate text-[12px] text-muted">{recommendation.source} · {recommendation.paths.length} item{recommendation.paths.length === 1 ? "" : "s"}</span>
          </span>
        </button>
        <TierBadge tier={recommendation.tier} />
        <span className="w-[92px] shrink-0 text-right text-[13px] tabular text-foreground">{formatBytes(recommendation.bytes)}</span>
      </div>

      {expanded ? (
        <div className="animate-fade-in space-y-3 pb-3 pl-[52px] pr-2">
          <dl className="grid gap-3 md:grid-cols-3">
            <div>
              <dt className="text-[12px] font-medium text-foreground">Why this was detected</dt>
              <dd className="mt-0.5 text-[12px] leading-4 text-muted">{recommendation.whyDetected}</dd>
            </div>
            <div>
              <dt className="text-[12px] font-medium text-foreground">Why it may be safe</dt>
              <dd className="mt-0.5 text-[12px] leading-4 text-muted">{recommendation.whySafe}</dd>
            </div>
            <div>
              <dt className="text-[12px] font-medium text-foreground">Risks</dt>
              <dd className="mt-0.5 text-[12px] leading-4 text-muted">{recommendation.risks}</dd>
            </div>
          </dl>
          <p className="text-[12px] text-muted">
            {CONFIDENCE_LABEL[recommendation.confidence]} · Estimated reclaim{" "}
            <span className="font-medium tabular text-foreground">{formatBytes(recommendation.bytes)}</span>
            {recommendation.methods.length > 0 ? <> · Proposed action: {recommendation.methods.join(", ")}</> : null}
          </p>
          {recommendation.alternativeCommand ? (
            <div className="rounded-md bg-panel-raised px-3 py-2">
              <p className="flex items-center gap-1.5 text-[12px] font-medium text-foreground">
                <Command className="size-3.5" aria-hidden="true" />
                Prefer the tool's own command
              </p>
              <p className="mt-1 text-[12px] text-muted">
                This tool manages its own storage. Running its command is safer than deleting files directly:
              </p>
              <div className="mt-1.5 flex items-center gap-2">
                <code className="mono-path flex-1 rounded bg-panel px-2 py-1 text-foreground">{recommendation.alternativeCommand}</code>
                <CopyButton text={recommendation.alternativeCommand} />
              </div>
            </div>
          ) : null}
          <ul className="space-y-0.5">
            {recommendation.paths.slice(0, 8).map((path) => (
              <li key={path} className="mono-path truncate text-subtle" title={path}>{displayPath(path, homeDir)}</li>
            ))}
            {recommendation.paths.length > 8 ? (
              <li className="text-[11px] text-subtle">and {recommendation.paths.length - 8} more — every path is listed in the cleanup plan.</li>
            ) : null}
          </ul>
        </div>
      ) : null}
    </div>
  );
}

/* ---------------- Page ---------------- */

export function RecommendationsPage() {
  const recommendations = useAppStore((s) => s.recommendations);
  const highlightedIds = useAppStore((s) => s.highlightedRecommendationIds);
  const setBanner = useAppStore((s) => s.setBanner);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [plan, setPlan] = useState<CleanupPlan | null>(null);
  const [building, setBuilding] = useState(false);
  const [planError, setPlanError] = useState<string | null>(null);

  const groups = useMemo(() => groupRecommendationsByTier(recommendations), [recommendations]);
  const selectedBytes = useMemo(
    () => recommendations.filter((r) => selected.has(r.id)).reduce((acc, r) => acc + r.bytes, 0),
    [recommendations, selected],
  );
  const totalActionable = useMemo(() => actionableBytes(recommendations), [recommendations]);

  const toggle = (id: string, checked: boolean) => {
    setSelected((prev) => {
      const next = new Set(prev);
      if (checked) next.add(id); else next.delete(id);
      return next;
    });
  };

  /** Select or clear exactly one tier's recommendations. Items that
      render without a checkbox (Keep tier) are never selectable and
      are skipped, matching their disabled state in the list. */
  const setTierSelection = (items: Recommendation[], select: boolean) => {
    setSelected((prev) => {
      const next = new Set(prev);
      for (const rec of items) {
        if (rec.tier === "keep") continue;
        if (select) next.add(rec.id); else next.delete(rec.id);
      }
      return next;
    });
  };

  const review = async () => {
    setBuilding(true);
    setPlanError(null);
    try {
      const built = await api.buildCleanupPlan([...selected]);
      setPlan(built);
    } catch (err) {
      setPlanError(err instanceof Error ? err.message : "A cleanup plan could not be built.");
    } finally {
      setBuilding(false);
    }
  };

  return (
    <div className="h-full overflow-y-auto px-6 py-5">
      <div className="mx-auto max-w-[880px]">
        <PageHeader
          title="Cleanup"
          subtitle="Every recommendation explains why it was detected, why it may be safe, and what could go wrong. Nothing is removed until you review an exact plan."
          showScanControl={false}
          actions={
            <Button
              variant="primary"
              disabled={selected.size === 0 || building}
              onClick={() => void review()}
            >
              <ShieldCheck className="size-4" aria-hidden="true" />
              Review cleanup{selected.size > 0 ? ` · ${formatBytes(selectedBytes)}` : ""}
            </Button>
          }
        />

        {planError ? <p role="alert" className="pb-3 text-[12px] text-critical">{planError}</p> : null}

        {recommendations.length === 0 ? (
          <EmptyState
            icon={Sparkles}
            title="No recommendations yet"
            description="Run a scan first. DiskPilot then groups cleanup candidates into Likely safe, Review first, and Keep — with the reasoning shown for each one."
          />
        ) : (
          <>
            <p className="pb-2 text-[12px] text-muted">
              <span className="font-medium tabular text-foreground">{formatBytes(totalActionable)}</span> may be
              reclaimable across {recommendations.filter((r) => r.tier !== "keep").length} recommendations.
              Select items, then review the exact plan before anything happens.
            </p>
            {groups.map((group) => (
              <Section
                key={group.tier}
                title={tierMeta(group.tier).label}
                description={tierMeta(group.tier).description}
                action={
                  group.tier === "likelySafe" ? (
                    <span className="flex items-center gap-1">
                      <Button
                        variant="subtle"
                        size="sm"
                        onClick={() => setTierSelection(group.items, true)}
                      >
                        Select all
                      </Button>
                      <Button
                        variant="subtle"
                        size="sm"
                        onClick={() => setTierSelection(group.items, false)}
                      >
                        Clear
                      </Button>
                    </span>
                  ) : undefined
                }
              >
                <div className="border-t border-border">
                  {group.items.map((rec) => (
                    <RecommendationRow
                      key={rec.id}
                      recommendation={rec}
                      checked={selected.has(rec.id)}
                      onToggle={toggle}
                      highlighted={highlightedIds.includes(rec.id)}
                    />
                  ))}
                </div>
              </Section>
            ))}
          </>
        )}
      </div>

      {plan ? (
        <CleanupDialog
          plan={plan}
          onClose={() => setPlan(null)}
          onDone={(reclaimed) => {
            setPlan(null);
            setSelected(new Set());
            setBanner({ kind: "success", message: `Cleanup complete. ${formatBytes(reclaimed)} reclaimed — see History for the full record.` });
          }}
        />
      ) : null}
    </div>
  );
}
