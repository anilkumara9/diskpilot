import { useEffect, useState } from "react";
import { AlertTriangle, CheckCircle2, HelpCircle, XCircle, type LucideIcon } from "lucide-react";
import * as api from "@/lib/api";
import { driveHealthFacts, driveHealthStatusLabel, lifeBarColorVar } from "@/lib/driveHealth";
import { formatBytes } from "@/lib/format";
import type { DriveHealth, DriveHealthStatus } from "@/lib/types";
import { Button } from "@/components/ui/button";
import { Section } from "@/components/ui/section";
import { SkeletonRows } from "@/components/ui/skeleton";
import { SizeBar } from "@/components/shared/SizeBar";
import { cn } from "@/lib/utils";

/**
 * Drive health: each physical drive's SMART self-report — wear,
 * temperature, and the drive's own failure prediction. Read-only
 * and scan-independent. A system that reports no drive health
 * gets one quiet note; a failed read gets an error and a Retry —
 * the two are never confused.
 */

const STATUS_META: Record<
  DriveHealthStatus,
  { icon: LucideIcon; textClass: string; bgSoftClass: string }
> = {
  healthy: { icon: CheckCircle2, textClass: "text-tier-safe", bgSoftClass: "bg-tier-safe/12" },
  caution: { icon: AlertTriangle, textClass: "text-tier-review", bgSoftClass: "bg-tier-review/12" },
  critical: { icon: XCircle, textClass: "text-critical", bgSoftClass: "bg-critical/12" },
  unknown: { icon: HelpCircle, textClass: "text-muted", bgSoftClass: "bg-hover" },
};

function StatusPill({ status }: { status: DriveHealthStatus }) {
  const meta = STATUS_META[status];
  const Icon = meta.icon;
  return (
    <span
      className={cn(
        "inline-flex shrink-0 items-center gap-1 rounded-[4px] px-1.5 py-0.5 text-[11px] font-medium leading-[14px]",
        meta.textClass,
        meta.bgSoftClass,
      )}
    >
      <Icon className="size-3" strokeWidth={1.75} aria-hidden="true" />
      {driveHealthStatusLabel(status)}
    </span>
  );
}

function DriveCard({ health }: { health: DriveHealth }) {
  const facts = driveHealthFacts(health);
  return (
    <div className="rounded-[10px] border border-border bg-panel-raised px-4 py-3">
      <div className="flex items-start gap-3">
        <div className="min-w-0 flex-1">
          <p className="truncate text-[13px] font-semibold text-foreground" title={health.model}>
            {health.model}
          </p>
          <p className="mt-0.5 text-[12px] text-muted">
            {health.interface} · {formatBytes(health.capacityBytes)}
          </p>
        </div>
        <StatusPill status={health.status} />
      </div>

      {health.predictedFailure ? (
        <p
          role="alert"
          className="mt-3 flex items-center gap-2 rounded-md bg-critical/10 px-3 py-2 text-[12px] leading-4 text-critical"
        >
          <AlertTriangle className="size-4 shrink-0" aria-hidden="true" />
          This drive reports it is likely to fail — back up now.
        </p>
      ) : null}

      {health.lifeLeftPercent !== null ? (
        <div className="mt-3">
          <p className="flex items-baseline justify-between gap-2 text-[12px]">
            <span className="text-muted">SSD life left</span>
            <span className="font-medium tabular text-foreground">{health.lifeLeftPercent}%</span>
          </p>
          <SizeBar
            fraction={Math.min(1, Math.max(0, health.lifeLeftPercent / 100))}
            colorVar={lifeBarColorVar(health.lifeLeftPercent)}
            height={4}
            className="mt-1.5"
          />
        </div>
      ) : null}

      <dl className="mt-3 grid grid-cols-2 gap-x-8 gap-y-3 border-t border-border pt-3 text-[12px] leading-4 md:grid-cols-4">
        {facts.map((fact) => (
          <div key={fact.label}>
            <dt className="text-muted">{fact.label}</dt>
            <dd className="mt-0.5 tabular text-foreground">{fact.value}</dd>
          </div>
        ))}
      </dl>
    </div>
  );
}

export function DriveHealthSection() {
  const [drives, setDrives] = useState<DriveHealth[] | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [reloadKey, setReloadKey] = useState(0);

  useEffect(() => {
    let live = true;
    api
      .getDriveHealth()
      .then((rows) => {
        if (!live) return;
        setDrives(rows);
        setLoadError(null);
      })
      .catch((err: unknown) => {
        if (live) {
          setLoadError(err instanceof Error ? err.message : "Drive health could not be read.");
        }
      });
    return () => {
      live = false;
    };
  }, [reloadKey]);

  return (
    <Section
      title="Drive health"
      description="Each drive's own self-monitoring (SMART): wear, temperature, and failure warnings."
    >
      {loadError ? (
        <div className="flex items-center gap-3 rounded-[10px] border border-border bg-panel-raised px-4 py-3">
          <p role="alert" className="flex-1 text-[12px] leading-4 text-critical">{loadError}</p>
          <Button variant="secondary" size="sm" onClick={() => setReloadKey((n) => n + 1)}>
            Retry
          </Button>
        </div>
      ) : drives === null ? (
        <SkeletonRows rows={2} />
      ) : drives.length === 0 ? (
        <p className="rounded-[10px] border border-border px-4 py-3 text-[12px] leading-4 text-muted">
          Drive health isn&rsquo;t reported on this system.
        </p>
      ) : (
        <div className="space-y-3">
          {drives.map((health, i) => (
            <DriveCard key={`${health.model}:${i}`} health={health} />
          ))}
        </div>
      )}
    </Section>
  );
}
