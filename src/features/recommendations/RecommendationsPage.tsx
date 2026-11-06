import { useEffect, useMemo, useState } from "react";
import {
  AlertTriangle,
  ChevronDown,
  ChevronRight,
  Command,
  Download,
  Film,
  Images,
  Info,
  PackageOpen,
  ShieldCheck,
  Sparkles,
  type LucideIcon,
} from "lucide-react";
import { useAppStore } from "@/lib/store";
import * as api from "@/lib/api";
import { actionableBytes, groupRecommendationsByTier } from "@/lib/analysis";
import { cardSubtitle, selectedCandidateBytes } from "@/lib/cards";
import { cleanupBanner } from "@/lib/cleanup";
import { displayPath, formatBytes, formatCountdown, formatRelative } from "@/lib/format";
import { CONFIDENCE_LABEL, tierMeta } from "@/lib/tiers";
import type {
  CandidateGroup,
  CleanupMode,
  CleanupPlan,
  CleanupResult,
  Recommendation,
} from "@/lib/types";
import { AppJunkSection } from "@/features/recommendations/AppJunkSection";
import { CompressSaveSection } from "@/features/recommendations/CompressSaveSection";
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
  /** The FULL execution result — the page's banner is derived from
      it, so partial and total failures are never hidden behind a
      reclaimed-bytes figure. */
  onDone: (result: CleanupResult) => void;
}) {
  const homeDir = useAppStore((s) => s.env?.homeDir);
  const refreshRecommendations = useAppStore((s) => s.refreshRecommendations);
  const refreshDisk = useAppStore((s) => s.refreshDisk);
  const [mode, setMode] = useState<CleanupMode>("recycleBin");
  const [running, setRunning] = useState(false);
  const [error, setError] = useState<string | null>(null);

  /* The expiry is live: a one-second tick drives both the footer
     countdown and the moment the execute button disables. */
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const timer = window.setInterval(() => setNow(Date.now()), 1000);
    return () => window.clearInterval(timer);
  }, []);
  const expired = now > plan.expiresAtMs;

  const execute = async () => {
    setRunning(true);
    setError(null);
    try {
      const result = await api.executeCleanupPlan(plan.id, mode);
      await Promise.all([refreshRecommendations(), refreshDisk()]);
      onDone(result);
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
            Total {formatBytes(plan.totalBytes)} ·{" "}
            {expired ? (
              <span className="text-critical">Expired</span>
            ) : (
              <>Expires in {formatCountdown(plan.expiresAtMs - now)}</>
            )}
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
      <ul className="max-h-[220px] space-y-1 overflow-y-auto rounded-md border border-border bg-panel p-2">
        {plan.items.map((item) => (
          <li key={item.path} className="flex items-center justify-between gap-3 rounded px-1.5 py-1">
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
        <label className="flex cursor-pointer items-start gap-2.5 rounded-md border border-border px-3.5 py-2.5 transition-colors duration-120 hover:border-border-strong has-checked:border-accent has-checked:bg-accent/5">
          <input type="radio" name="cleanup-mode" value="recycleBin" checked={mode === "recycleBin"} onChange={() => setMode("recycleBin")} className="mt-0.5 accent-[var(--accent-fill)]" />
          <span>
            <span className="block text-[13px] font-medium text-foreground">Move to Recycle Bin (recommended)</span>
            <span className="block text-[12px] text-muted">Recoverable until you empty the Recycle Bin. Space is freed when it is emptied.</span>
          </span>
        </label>
        <label className={cn("mt-2 flex items-start gap-2.5 rounded-md border border-border px-3.5 py-2.5 transition-colors duration-120 has-checked:border-accent has-checked:bg-accent/5", plan.allowPermanent ? "cursor-pointer hover:border-border-strong" : "cursor-not-allowed opacity-50")}>
          <input
            type="radio"
            name="cleanup-mode"
            value="permanent"
            checked={mode === "permanent"}
            disabled={!plan.allowPermanent}
            onChange={() => setMode("permanent")}
            className="mt-0.5 accent-[var(--accent-fill)]"
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

/* ---------------- Clean Up cards ---------------- */

const CARD_ICONS: Record<string, LucideIcon> = {
  "old-installers": PackageOpen,
  "forgotten-downloads": Download,
  "old-screenshots": Images,
  "big-old-videos": Film,
};

export function CleanupCard({
  group,
  onReview,
}: {
  group: CandidateGroup;
  onReview: () => void;
}) {
  const Icon = CARD_ICONS[group.card] ?? Sparkles;
  return (
    <button
      type="button"
      onClick={onReview}
      className="flex items-start gap-3 rounded-[10px] border border-border bg-panel-raised px-4 py-3 text-left transition-colors duration-120 hover:border-border-strong"
    >
      <span className="mt-0.5 flex size-8 shrink-0 items-center justify-center rounded-md bg-accent/10 text-accent">
        <Icon className="size-4" aria-hidden="true" />
      </span>
      <span className="min-w-0 flex-1">
        <span className="flex items-baseline justify-between gap-2">
          <span className="truncate text-[13px] font-medium text-foreground">{group.title}</span>
          <span className="shrink-0 text-[13px] tabular text-foreground">{formatBytes(group.totalBytes)}</span>
        </span>
        <span className="mt-0.5 block text-[12px] leading-4 text-muted">{cardSubtitle(group.card)}</span>
        <span className="mt-1.5 flex items-center justify-between gap-2">
          <span className="text-[12px] tabular text-subtle">
            {group.files.length} file{group.files.length === 1 ? "" : "s"}
          </span>
          <span className="flex items-center gap-0.5 text-[12px] font-medium text-accent">
            Review
            <ChevronRight className="size-3.5" aria-hidden="true" />
          </span>
        </span>
      </span>
    </button>
  );
}

/** Pick exactly which of a card's files go into a cleanup plan. The
    plan itself is built by the core from candidate ids and reviewed
    in the shared CleanupDialog; card plans are always Recycle Bin
    only, which that dialog reflects via the plan's allowPermanent. */
export function CardReviewDialog({
  group,
  onClose,
  onPlan,
}: {
  group: CandidateGroup;
  onClose: () => void;
  onPlan: (plan: CleanupPlan) => void;
}) {
  const homeDir = useAppStore((s) => s.env?.homeDir);
  const [selected, setSelected] = useState<ReadonlySet<string>>(
    () => new Set(group.files.map((f) => f.id)),
  );
  const [building, setBuilding] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const selectedBytes = selectedCandidateBytes(group.files, selected);

  const toggle = (id: string, checked: boolean) => {
    setSelected((prev) => {
      const next = new Set(prev);
      if (checked) next.add(id); else next.delete(id);
      return next;
    });
  };

  const buildPlan = async () => {
    setBuilding(true);
    setError(null);
    try {
      const built = await api.buildCardCleanupPlan([...selected]);
      onPlan(built);
    } catch (err) {
      setError(err instanceof Error ? err.message : "A cleanup plan could not be built.");
      setBuilding(false);
    }
  };

  return (
    <Dialog
      open
      onClose={onClose}
      title={group.title}
      description={`${cardSubtitle(group.card)}. Card cleanups always move files to the Recycle Bin — they are never permanently deleted.`}
      wide
      dismissible={!building}
      footer={
        <>
          <span className="mr-auto text-[12px] tabular text-muted">
            {selected.size} of {group.files.length} files · {formatBytes(selectedBytes)} selected
          </span>
          <Button variant="secondary" onClick={onClose} disabled={building}>Cancel</Button>
          <Button
            variant="primary"
            onClick={() => void buildPlan()}
            disabled={building || selected.size === 0}
          >
            <ShieldCheck className="size-4" aria-hidden="true" />
            {building ? "Building plan…" : "Review cleanup plan"}
          </Button>
        </>
      }
    >
      {error ? <p role="alert" className="mb-3 text-[12px] text-critical">{error}</p> : null}
      <ul className="max-h-[320px] space-y-0.5 overflow-y-auto rounded-md border border-border bg-panel p-2">
        {group.files.map((file) => (
          <li
            key={file.id}
            className="flex items-center gap-2.5 rounded px-1.5 py-1.5 transition-colors duration-120 hover:bg-hover"
          >
            <Checkbox
              checked={selected.has(file.id)}
              onChange={(e) => toggle(file.id, e.target.checked)}
              aria-label={`Select ${file.name}`}
            />
            <span className="min-w-0 flex-1">
              <span className="block truncate text-[13px] text-foreground" title={file.path}>{file.name}</span>
              <span className="mono-path block truncate text-subtle" title={file.path}>{displayPath(file.path, homeDir)}</span>
            </span>
            <span className="shrink-0 text-right">
              <span className="block text-[12px] tabular text-foreground">{formatBytes(file.bytes)}</span>
              <span className="block text-[11px] text-subtle">{formatRelative(file.modifiedMs)}</span>
            </span>
          </li>
        ))}
      </ul>
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
  /* Guided items (e.g. Windows.old) belong to another tool: DiskPilot
     explains the route but never selects or plans them. */
  const selectable = recommendation.tier !== "keep" && !recommendation.guidedOnly;
  const Icon = expanded ? ChevronDown : ChevronRight;

  return (
    <div
      className={cn(
        "border-b border-border/60 transition-colors duration-120",
        highlighted && "bg-accent/8",
      )}
    >
      <div className="flex items-center gap-3 px-1 py-2 transition-colors duration-120 hover:bg-hover">
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
        {recommendation.guidedOnly ? (
          <span className="shrink-0 rounded bg-panel-raised px-1.5 py-0.5 text-[11px] font-medium leading-[14px] text-muted ring-1 ring-border">
            Guided
          </span>
        ) : null}
        <TierBadge tier={recommendation.tier} />
        <span className="w-[92px] shrink-0 text-right text-[13px] tabular text-foreground">{formatBytes(recommendation.bytes)}</span>
      </div>

      {expanded ? (
        <div className="animate-fade-in space-y-3 pb-3 pl-[52px] pr-2">
          {recommendation.guidedOnly && recommendation.guidance ? (
            <div className="flex gap-3 rounded-[10px] border border-border bg-panel-raised px-4 py-3">
              <Info className="mt-0.5 size-4 shrink-0 text-muted" aria-hidden="true" />
              <p className="text-[12px] leading-4 text-muted">{recommendation.guidance}</p>
            </div>
          ) : null}
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
  const cleanupCards = useAppStore((s) => s.cleanupCards);
  const highlightedIds = useAppStore((s) => s.highlightedRecommendationIds);
  const setBanner = useAppStore((s) => s.setBanner);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [plan, setPlan] = useState<CleanupPlan | null>(null);
  const [building, setBuilding] = useState(false);
  const [planError, setPlanError] = useState<string | null>(null);
  const [reviewGroup, setReviewGroup] = useState<CandidateGroup | null>(null);

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
      render without a checkbox (Keep tier, guided-only items) are
      never selectable and are skipped, matching their disabled state
      in the list. */
  const setTierSelection = (items: Recommendation[], select: boolean) => {
    setSelected((prev) => {
      const next = new Set(prev);
      for (const rec of items) {
        if (rec.tier === "keep" || rec.guidedOnly) continue;
        if (select) next.add(rec.id); else next.delete(rec.id);
      }
      return next;
    });
  };

  const review = async () => {
    /* Defensive: a guided-only id must never reach plan building,
       even if one somehow landed in the selection set. */
    const ids = [...selected].filter((id) => {
      const rec = recommendations.find((r) => r.id === id);
      return rec !== undefined && !rec.guidedOnly;
    });
    if (ids.length === 0) return;
    setBuilding(true);
    setPlanError(null);
    try {
      const built = await api.buildCleanupPlan(ids);
      setPlan(built);
    } catch (err) {
      setPlanError(err instanceof Error ? err.message : "A cleanup plan could not be built.");
    } finally {
      setBuilding(false);
    }
  };

  return (
    <div className="h-full overflow-y-auto px-6 py-6">
      <div className="max-w-[880px]">
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

        {recommendations.length === 0 && cleanupCards.length === 0 ? (
          <EmptyState
            icon={Sparkles}
            className="min-h-[360px]"
            title="No recommendations yet"
            description="Run a scan first. DiskPilot then groups cleanup candidates into Likely safe, Review first, and Keep — with the reasoning shown for each one."
          />
        ) : (
          <>
            {recommendations.length > 0 ? (
              <p className="pb-2 text-[12px] text-muted">
                <span className="font-medium tabular text-foreground">{formatBytes(totalActionable)}</span> may be
                reclaimable across {recommendations.filter((r) => r.tier !== "keep").length} recommendations.
                Select items, then review the exact plan before anything happens.
              </p>
            ) : null}
            {cleanupCards.length > 0 ? (
              <Section
                title="Cards"
                description="Individual files the scan picked out by kind and age. Review a card to choose exactly which files go into a cleanup plan."
              >
                <div className="grid gap-3 sm:grid-cols-2">
                  {cleanupCards.map((group) => (
                    <CleanupCard
                      key={group.card}
                      group={group}
                      onReview={() => setReviewGroup(group)}
                    />
                  ))}
                </div>
              </Section>
            ) : null}
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

        {/* Below the cleanup content proper: app caches are
            measured live (not from the scan) and compression is a
            different offer (nothing is removed). Both sections
            hide themselves when there is nothing to offer. */}
        <AppJunkSection />
        <CompressSaveSection />
      </div>

      {reviewGroup ? (
        <CardReviewDialog
          group={reviewGroup}
          onClose={() => setReviewGroup(null)}
          onPlan={(built) => {
            setReviewGroup(null);
            setPlan(built);
          }}
        />
      ) : null}

      {plan ? (
        <CleanupDialog
          plan={plan}
          onClose={() => setPlan(null)}
          onDone={(result) => {
            setPlan(null);
            setSelected(new Set());
            setBanner(cleanupBanner(result));
          }}
        />
      ) : null}
    </div>
  );
}
