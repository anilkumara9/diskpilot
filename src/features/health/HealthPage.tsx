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
import { buildSeries, formatAxisBytes } from "@/lib/chart";
import { driveStatus, healthHeadline, type DriveStatus } from "@/lib/health";
import { formatBytes, formatCount, formatDate, formatRelative } from "@/lib/format";
import type {
  DriveInfo,
  ScanHistoryEntry,
  SpaceForecast,
} from "@/lib/types";
import { sameRootPath } from "@/features/overview/OverviewPage";
import { PageHeader } from "@/components/layout/PageHeader";
import { Chart, type ChartSeries } from "@/components/Chart";
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

/**
 * The scan-history time series at page scale, drawn by the shared
 * Chart: scanned size as the accent area, free space as a muted
 * line over the same scale so the two can be compared. The Chart
 * carries the axis (ticks, end dates, crosshair tooltip); the
 * caption beneath keeps the min–max ranges it has always stated.
 */
function GrowthChart({ entries }: { entries: ScanHistoryEntry[] }) {
  const freeValues = useMemo(
    () =>
      entries
        .map((entry) => entry.driveFreeBytes)
        .filter((v): v is number => v !== null),
    [entries],
  );
  const hasFree = freeValues.length > 0;

  const series = useMemo<ChartSeries[]>(
    () => [
      {
        label: "Scanned size",
        color: "var(--accent)",
        points: buildSeries(entries, (entry) => ({
          x: entry.scannedAtMs,
          y: entry.totalBytes,
        })),
        formatY: formatAxisBytes,
      },
      ...(hasFree
        ? [
            {
              label: "Free space",
              color: "var(--muted)",
              points: buildSeries(entries, (entry) => ({
                x: entry.scannedAtMs,
                y: entry.driveFreeBytes,
              })),
              formatY: formatAxisBytes,
            },
          ]
        : []),
    ],
    [entries, hasFree],
  );

  if (entries.length === 0) return null;
  const totals = entries.map((entry) => entry.totalBytes);

  return (
    <div>
      <div className="flex items-center justify-end gap-4 pb-2 text-[11px] text-muted" aria-hidden="true">
        <span className="flex items-center gap-1.5">
          <span className="inline-block h-[3px] w-4 rounded-full bg-accent" />
          Scanned size
        </span>
        {hasFree ? (
          <span className="flex items-center gap-1.5">
            <span className="inline-block h-[3px] w-4 rounded-full bg-muted" />
            Free space
          </span>
        ) : null}
      </div>
      <Chart
        series={series}
        height={140}
        ariaLabel="Scanned size and free space across recent scans"
      />
      <p className="tabular pt-1.5 text-center text-[11px] text-subtle">
        Scanned size {formatBytes(Math.min(...totals))} to {formatBytes(Math.max(...totals))}
        {hasFree
          ? ` · Free space ${formatBytes(Math.min(...freeValues))} to ${formatBytes(Math.max(...freeValues))}`
          : ""}
      </p>
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
