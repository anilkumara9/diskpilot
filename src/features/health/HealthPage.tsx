import { useEffect, useMemo, useState } from "react";
import {
  AlertTriangle,
  CheckCircle2,
  Eye,
  HardDrive,
  TrendingUp,
  type LucideIcon,
} from "lucide-react";
import { useAppStore } from "@/lib/store";
import * as api from "@/lib/api";
import { driveStatus, healthHeadline, type DriveStatus } from "@/lib/health";
import { formatBytes, formatCount, formatDate, formatRelative } from "@/lib/format";
import type {
  DriveInfo,
  ScanHistoryEntry,
  SpaceForecast,
} from "@/lib/types";
import { sameRootPath } from "@/features/overview/OverviewPage";
import { PageHeader } from "@/components/layout/PageHeader";
import { Button } from "@/components/ui/button";
import { Section } from "@/components/ui/section";
import { SkeletonRows } from "@/components/ui/skeleton";
import { EmptyState } from "@/components/shared/EmptyState";
import { SizeBar } from "@/components/shared/SizeBar";
import { cn } from "@/lib/utils";

/* ---------------- Drive status chip ---------------- */

/** The tier tag treatment, with drive-health labels: colour always
    carries the risk, exactly as cleanup tiers do elsewhere. */
const STATUS_META: Record<
  DriveStatus,
  { label: string; icon: LucideIcon; textClass: string; bgSoftClass: string }
> = {
  healthy: { label: "Healthy", icon: CheckCircle2, textClass: "text-tier-safe", bgSoftClass: "bg-tier-safe/12" },
  gettingLow: { label: "Getting low", icon: Eye, textClass: "text-tier-review", bgSoftClass: "bg-tier-review/12" },
  lowSpace: { label: "Low space", icon: AlertTriangle, textClass: "text-critical", bgSoftClass: "bg-critical/12" },
};

function DriveStatusChip({ drive }: { drive: DriveInfo }) {
  const meta = STATUS_META[driveStatus(drive)];
  const Icon = meta.icon;
  return (
    <span
      className={cn(
        "inline-flex items-center gap-1 rounded-[4px] px-1.5 py-0.5 text-[11px] font-medium leading-[14px]",
        meta.textClass,
        meta.bgSoftClass,
      )}
    >
      <Icon className="size-3" strokeWidth={1.75} aria-hidden="true" />
      {meta.label}
    </span>
  );
}

/* ---------------- Figures ---------------- */

function Figure({ label, value }: { label: string; value: string }) {
  return (
    <div className="min-w-[140px]">
      <p className="text-[12px] text-muted">{label}</p>
      <p className="mt-0.5 text-[20px] font-semibold leading-6 tabular text-foreground">{value}</p>
    </div>
  );
}

/* ---------------- Growth chart ---------------- */

const dayFormat = new Intl.DateTimeFormat("en-US", {
  year: "numeric",
  month: "short",
  day: "numeric",
});

function formatDay(timestampMs: number): string {
  return dayFormat.format(new Date(timestampMs));
}

interface ChartPoint {
  ms: number;
  total: number;
  free: number | null;
}

/**
 * The scan-history time series at page scale: scanned size as the
 * accent area (the forecast sparkline's pattern, taller), free space
 * as a muted line over the same scale so the two can be compared.
 * Static SVG — no motion to reduce.
 */
function GrowthChart({ entries }: { entries: ScanHistoryEntry[] }) {
  const width = 848;
  const height = 140;
  const pad = 6;

  const points: ChartPoint[] = useMemo(
    () =>
      entries.map((entry) => ({
        ms: entry.scannedAtMs,
        total: entry.totalBytes,
        free: entry.driveFreeBytes,
      })),
    [entries],
  );

  const model = useMemo(() => {
    if (points.length === 0) return null;
    const values = points.flatMap((p) => (p.free !== null ? [p.total, p.free] : [p.total]));
    const min = Math.min(...values);
    const max = Math.max(...values);
    const span = max - min;
    const x = (i: number) =>
      points.length === 1 ? width / 2 : pad + (i / (points.length - 1)) * (width - pad * 2);
    const y = (v: number) =>
      span === 0 ? height / 2 : height - pad - ((v - min) / span) * (height - pad * 2);
    const totalCoords = points.map((p, i) => `${x(i).toFixed(1)},${y(p.total).toFixed(1)}`);
    const freeCoords = points
      .map((p, i) => (p.free !== null ? `${x(i).toFixed(1)},${y(p.free).toFixed(1)}` : null))
      .filter((c): c is string => c !== null);
    const freeValues = points
      .map((p) => p.free)
      .filter((v): v is number => v !== null);
    return {
      area: `${x(0).toFixed(1)},${(height - pad).toFixed(1)} ${totalCoords.join(" ")} ${x(points.length - 1).toFixed(1)},${(height - pad).toFixed(1)}`,
      totalLine: totalCoords.join(" "),
      freeLine: freeCoords.join(" "),
      hasFree: freeValues.length > 0,
      baselineY: y(min),
      minTotal: Math.min(...points.map((p) => p.total)),
      maxTotal: Math.max(...points.map((p) => p.total)),
      minFree: freeValues.length > 0 ? Math.min(...freeValues) : null,
      maxFree: freeValues.length > 0 ? Math.max(...freeValues) : null,
    };
  }, [points]);

  if (!model) return null;
  const first = points[0];
  const last = points[points.length - 1];

  return (
    <div>
      <div className="flex items-center justify-end gap-4 pb-2 text-[11px] text-muted" aria-hidden="true">
        <span className="flex items-center gap-1.5">
          <span className="inline-block h-[3px] w-4 rounded-full bg-accent" />
          Scanned size
        </span>
        {model.hasFree ? (
          <span className="flex items-center gap-1.5">
            <span className="inline-block h-[3px] w-4 rounded-full bg-muted" />
            Free space
          </span>
        ) : null}
      </div>
      <svg
        width="100%"
        height={height}
        viewBox={`0 0 ${width} ${height}`}
        preserveAspectRatio="none"
        role="img"
        aria-label="Scanned size and free space across recent scans"
        className="block w-full"
      >
        <polygon points={model.area} fill="var(--accent)" opacity={0.1} />
        <line
          x1={pad}
          x2={width - pad}
          y1={model.baselineY}
          y2={model.baselineY}
          stroke="var(--border-strong)"
          strokeWidth={1}
          strokeDasharray="3 3"
          vectorEffect="non-scaling-stroke"
        />
        <polyline
          points={model.totalLine}
          fill="none"
          stroke="var(--accent)"
          strokeWidth={1.5}
          strokeLinejoin="round"
          strokeLinecap="round"
          vectorEffect="non-scaling-stroke"
        />
        {model.hasFree ? (
          <polyline
            points={model.freeLine}
            fill="none"
            stroke="var(--muted)"
            strokeWidth={1.5}
            strokeLinejoin="round"
            strokeLinecap="round"
            vectorEffect="non-scaling-stroke"
          />
        ) : null}
      </svg>
      <div className="flex items-baseline justify-between gap-4 pt-1.5 text-[11px] text-subtle">
        <span>{formatDay(first.ms)}</span>
        <span className="tabular">
          Scanned size {formatBytes(model.minTotal)} to {formatBytes(model.maxTotal)}
          {model.hasFree && model.minFree !== null && model.maxFree !== null
            ? ` · Free space ${formatBytes(model.minFree)} to ${formatBytes(model.maxFree)}`
            : ""}
        </span>
        <span>{formatDay(last.ms)}</span>
      </div>
    </div>
  );
}

/* ---------------- Page ---------------- */

const SCHEDULE_LABEL: Record<string, string> = {
  off: "Off",
  daily: "Daily",
  weekly: "Weekly",
};

export function HealthPage() {
  const summary = useAppStore((s) => s.summary);
  const settings = useAppStore((s) => s.settings);
  const navigate = useAppStore((s) => s.navigate);

  const [drives, setDrives] = useState<DriveInfo[] | null>(null);
  const [forecast, setForecast] = useState<SpaceForecast | null>(null);
  const [history, setHistory] = useState<ScanHistoryEntry[] | null>(null);
  const [autostart, setAutostart] = useState<boolean | null>(null);

  /* Drives, forecast and history refresh whenever a scan finishes —
     the finished timestamp is the trigger, as on the Overview. */
  const finishedAtMs = summary?.finishedAtMs;
  useEffect(() => {
    let live = true;
    void (async () => {
      const [d, f, h, a] = await Promise.all([
        api.getDrives(),
        api.getSpaceForecast(),
        api.getScanHistory(),
        api.getAutostart(),
      ]);
      if (!live) return;
      setDrives(d);
      setForecast(f);
      setHistory(h);
      setAutostart(a);
    })();
    return () => {
      live = false;
    };
  }, [finishedAtMs]);

  const rootHistory = useMemo(() => {
    if (!history || !summary) return [];
    return history
      .filter((entry) => sameRootPath(entry.rootPath, summary.root))
      .sort((a, b) => a.scannedAtMs - b.scannedAtMs);
  }, [history, summary]);

  const headline = drives ? healthHeadline(drives) : null;
  const filling =
    forecast !== null && forecast.daysUntilFull !== null && forecast.projectedFullAtMs !== null;
  const hasTrend = rootHistory.length >= 2;

  return (
    <div className="h-full overflow-y-auto px-6 py-6">
      <div className="max-w-[880px]">
        <PageHeader
          title="Health"
          subtitle={headline ?? "Drive space, growth, and the background watch at a glance."}
        />

        <Section title="Drives" description="Every drive on this PC, judged by the room it has left.">
          {drives === null ? (
            <SkeletonRows rows={3} />
          ) : drives.length === 0 ? (
            <p className="text-[12px] text-muted">No drives were found on this PC.</p>
          ) : (
            <ul className="divide-y divide-border">
              {drives.map((drive) => {
                const usedFraction =
                  drive.totalBytes > 0
                    ? Math.min(1, Math.max(0, 1 - drive.freeBytes / drive.totalBytes))
                    : 0;
                const label = drive.rootPath.replace(/[\\/]+$/, "");
                return (
                  <li key={drive.rootPath} className="flex items-center gap-3 py-2">
                    <HardDrive className="size-4 shrink-0 text-muted" aria-hidden="true" />
                    <span
                      className="w-[132px] shrink-0 truncate text-[13px] font-medium text-foreground"
                      title={drive.rootPath}
                    >
                      {label}
                    </span>
                    <span className="min-w-0 flex-1">
                      <SizeBar fraction={usedFraction} colorVar="var(--accent)" height={4} />
                    </span>
                    <span className="w-[168px] shrink-0 text-right text-[12px] tabular text-muted">
                      {formatBytes(drive.freeBytes)} free of {formatBytes(drive.totalBytes)}
                    </span>
                    <span className="flex w-[104px] shrink-0 justify-end">
                      <DriveStatusChip drive={drive} />
                    </span>
                  </li>
                );
              })}
            </ul>
          )}
        </Section>

        <Section title="Forecast" description="Projected from your scan history — the trend, not a promise.">
          {history === null ? (
            <SkeletonRows rows={4} />
          ) : !hasTrend ? (
            <EmptyState
              icon={TrendingUp}
              title="Not enough history yet"
              description="Your growth chart and forecast appear after a few scans of the same folder. Every scan adds a point; two points make a trend."
            />
          ) : (
            <div className="rounded-[10px] border border-border bg-panel-raised px-4 py-3">
              {forecast && forecast.pointsUsed >= 2 ? (
                filling ? (
                  <div className="flex flex-wrap gap-x-10 gap-y-4">
                    <Figure
                      label="Fills in"
                      value={`About ${Math.round(forecast.daysUntilFull as number)} days`}
                    />
                    <Figure
                      label="Daily growth"
                      value={`${formatBytes(Math.max(0, forecast.dailyGrowthBytes))} per day`}
                    />
                    <Figure label="Projected full" value={formatDay(forecast.projectedFullAtMs as number)} />
                  </div>
                ) : (
                  <>
                    <p className="text-[15px] font-semibold leading-5 text-foreground">
                      Your disk isn't growing — no fill-up date in sight.
                    </p>
                    <p className="mt-0.5 text-[12px] leading-4 text-muted">
                      {forecast.dailyGrowthBytes < 0
                        ? `Usage is shrinking by ${formatBytes(-forecast.dailyGrowthBytes)} per day on average across the last ${forecast.pointsUsed} scans.`
                        : `Usage has been flat across the last ${forecast.pointsUsed} scans.`}
                    </p>
                  </>
                )
              ) : null}
              <div className={forecast && forecast.pointsUsed >= 2 ? "mt-4" : undefined}>
                <GrowthChart entries={rootHistory} />
              </div>
            </div>
          )}
        </Section>

        <Section
          title="Watch"
          description="The background schedule and alerts, as configured."
          action={
            <Button variant="subtle" size="sm" onClick={() => navigate("settings")}>
              Open Settings
            </Button>
          }
        >
          <div className="rounded-[10px] border border-border px-4 py-3">
            <dl className="grid grid-cols-2 gap-x-8 gap-y-3 text-[12px] leading-4 md:grid-cols-3">
              <div>
                <dt className="text-muted">Schedule</dt>
                <dd className="mt-0.5 text-foreground">{SCHEDULE_LABEL[settings.autoScan] ?? settings.autoScan}</dd>
              </div>
              <div>
                <dt className="text-muted">Low-space alert</dt>
                <dd className="mt-0.5 tabular text-foreground">
                  Below {settings.lowSpaceAlertGb} GB free
                </dd>
              </div>
              <div>
                <dt className="text-muted">Growth alert</dt>
                <dd className="mt-0.5 tabular text-foreground">
                  More than {settings.growthAlertGb} GB between checks
                </dd>
              </div>
              <div>
                <dt className="text-muted">Starts with Windows</dt>
                <dd className="mt-0.5 text-foreground">
                  {autostart === null ? "Checking…" : autostart ? "On" : "Off"}
                </dd>
              </div>
              <div>
                <dt className="text-muted">Last scan</dt>
                <dd className="mt-0.5 text-foreground" title={summary ? formatDate(summary.finishedAtMs) : undefined}>
                  {summary ? formatRelative(summary.finishedAtMs) : "Not scanned yet"}
                </dd>
              </div>
              <div>
                <dt className="text-muted">Scans in history</dt>
                <dd className="mt-0.5 tabular text-foreground">
                  {history === null
                    ? "—"
                    : formatCount(summary ? rootHistory.length : history.length)}
                </dd>
              </div>
            </dl>
          </div>
        </Section>
      </div>
    </div>
  );
}
