import { useEffect, useMemo, useState } from "react";
import {
  ArrowRight,
  Bot,
  Cloud,
  FileDown,
  FolderLock,
  HardDrive,
  Info,
  ScanSearch,
  Sparkles,
  Trash2,
  TrendingUp,
} from "lucide-react";
import { useAppStore } from "@/lib/store";
import * as api from "@/lib/api";
import { buildSeries, formatAxisBytes } from "@/lib/chart";
import { categoryRows, sortLocations, totalRecoverable } from "@/lib/analysis";
import { categoryMeta } from "@/lib/categories";
import { displayPath, formatBytes, formatCount, formatDate, formatDuration, formatPercent, formatSignedBytes, formatSignedCount } from "@/lib/format";
import { buildReportHtml } from "@/lib/report";
import type {
  BiggestChanges,
  Category,
  CloudStatus,
  DirChange,
  DiskInfo,
  DriveInfo,
  RecycleBinItem,
  ScanComparison,
  ScanHistoryEntry,
  ScanSummary,
  SpaceForecast,
} from "@/lib/types";
import { Button } from "@/components/ui/button";
import { Section } from "@/components/ui/section";
import { Tooltip } from "@/components/ui/tooltip";
import { Chart, type ChartSeries } from "@/components/Chart";
import { CategoryBadge, CategoryDot } from "@/components/shared/CategoryBadge";
import { TierBadge } from "@/components/shared/TierBadge";
import { SizeBar } from "@/components/shared/SizeBar";
import { EmptyState } from "@/components/shared/EmptyState";
import { FreeUpSpaceDialog } from "@/components/shared/FreeUpSpaceDialog";
import { ResultsAge } from "@/components/shared/ResultsAge";
import { ScanControl } from "@/components/layout/ScanControl";

/** Root comparison the way Windows treats paths: trailing separators
    ignored, case-insensitive. */
export function sameRootPath(a: string, b: string): boolean {
  const normalise = (s: string) => s.replace(/[\\/]+$/, "").toLowerCase();
  return normalise(a) === normalise(b);
}

/* ---------------- StorageStrip ---------------- */

interface StripSegment {
  key: string;
  label: string;
  bytes: number;
  /** Width as a percent of the disk; null is the flexible free tail. */
  widthPct: number | null;
  colorVar: string | null;
  fillClass?: string;
  recoverable?: number;
}

export function StorageStrip({
  disk,
  summary,
}: {
  disk: DiskInfo | null;
  summary: ScanSummary | null;
}) {
  /* Segment widths grow in on mount; the hover layer below animates
     with the same widths so the two never drift apart. */
  const [mounted, setMounted] = useState(false);
  useEffect(() => {
    let inner = 0;
    const outer = requestAnimationFrame(() => {
      inner = requestAnimationFrame(() => setMounted(true));
    });
    return () => {
      cancelAnimationFrame(outer);
      cancelAnimationFrame(inner);
    };
  }, []);

  const total = disk?.totalBytes ?? summary?.totalBytes ?? 0;
  if (!disk && !summary) {
    return (
      <div className="flex h-7 items-center rounded-md bg-panel-raised px-3 text-[12px] text-muted">
        Disk usage appears here after a scan.
      </div>
    );
  }
  const rows = summary ? categoryRows(summary) : [];
  const scannedTotal = summary?.totalBytes ?? 0;
  const otherUsed = Math.max(0, (disk?.usedBytes ?? scannedTotal) - scannedTotal);
  const free = disk?.availableBytes ?? Math.max(0, total - (disk?.usedBytes ?? scannedTotal));

  const segments: StripSegment[] = [];
  for (const row of rows) {
    const widthPct = total > 0 ? (row.bytes / total) * 100 : 0;
    if (widthPct <= 0) continue;
    segments.push({
      key: row.category,
      label: categoryMeta(row.category).label,
      bytes: row.bytes,
      widthPct,
      colorVar: categoryMeta(row.category).colorVar,
      recoverable: row.recoverable,
    });
  }
  if (otherUsed > 0 && total > 0) {
    segments.push({
      key: "other-used",
      label: "Other used space",
      bytes: otherUsed,
      widthPct: (otherUsed / total) * 100,
      colorVar: null,
      fillClass: "bg-border-strong",
    });
  }
  if (total > 0) {
    segments.push({
      key: "free",
      label: "Available",
      bytes: free,
      widthPct: null,
      colorVar: null,
    });
  }

  const segmentWidth = (segment: StripSegment) =>
    segment.widthPct === null ? undefined : mounted ? `${segment.widthPct}%` : "0%";

  return (
    <div>
      <div className="relative">
        <div
          className="flex h-7 w-full overflow-hidden rounded-md bg-panel-raised"
          role="img"
          aria-label="Disk usage by category"
        >
          {segments.map((segment) =>
            segment.widthPct === null ? (
              <div key={segment.key} className="h-full min-w-0 flex-1 bg-transparent" />
            ) : (
              <div
                key={segment.key}
                className={`relative h-full shrink-0 transition-[width] duration-500 ease-out ${segment.fillClass ?? ""}`}
                style={{
                  width: segmentWidth(segment),
                  background: segment.colorVar ?? undefined,
                  boxShadow: "inset -1px 0 0 0 var(--panel)",
                }}
              >
                {segment.recoverable && segment.recoverable > 0 ? (
                  <div
                    className="hatched absolute inset-y-0 right-0"
                    style={{ width: `${(segment.recoverable / segment.bytes) * 100}%` }}
                  />
                ) : null}
              </div>
            ),
          )}
        </div>

        {/* Hover layer: the product's floating-card tooltip per
            segment (category, size, share of disk) — it lives
            outside the clipped bar so the card is never cut off. */}
        {total > 0 ? (
          <div className="absolute inset-0 flex" aria-hidden="true">
            {segments.map((segment) => (
              <div
                key={segment.key}
                className={
                  segment.widthPct === null
                    ? "relative h-full min-w-0 flex-1"
                    : "relative h-full shrink-0 transition-[width] duration-500 ease-out"
                }
                style={{ width: segmentWidth(segment) }}
              >
                <Tooltip
                  side="bottom"
                  className="h-full w-full"
                  content={
                    <>
                      <span className="block font-medium">{segment.label}</span>
                      <span className="tabular block">
                        {formatBytes(segment.bytes)} · {formatPercent(segment.bytes / total)} of disk
                      </span>
                      {segment.recoverable && segment.recoverable > 0 ? (
                        <span className="tabular block text-tier-safe">
                          {formatBytes(segment.recoverable)} likely recoverable
                        </span>
                      ) : null}
                    </>
                  }
                >
                  <span className="block h-full w-full" />
                </Tooltip>
              </div>
            ))}
          </div>
        ) : null}
      </div>

      <ul className="mt-2.5 flex flex-wrap gap-x-4 gap-y-1.5">
        {rows.map((row) => (
          <li key={row.category} className="flex items-center gap-1.5 whitespace-nowrap text-[12px] text-muted">
            <CategoryDot category={row.category} />
            <span className="text-foreground">{categoryMeta(row.category).label}</span>
            <span className="tabular">{formatBytes(row.bytes)}</span>
            {row.recoverable > 0 ? (
              <span className="tabular text-tier-safe">({formatBytes(row.recoverable)} recoverable)</span>
            ) : null}
          </li>
        ))}
        {otherUsed > 0 ? (
          <li className="flex items-center gap-1.5 whitespace-nowrap text-[12px] text-muted">
            <span className="inline-block size-2 rounded-full bg-border-strong" aria-hidden="true" />
            <span className="text-foreground">Other used space</span>
            <span className="tabular">{formatBytes(otherUsed)}</span>
          </li>
        ) : null}
        {disk ? (
          <li className="flex items-center gap-1.5 whitespace-nowrap text-[12px] text-muted">
            <span className="inline-block size-2 rounded-full border border-border-strong" aria-hidden="true" />
            <span className="text-foreground">Available</span>
            <span className="tabular">{formatBytes(disk.availableBytes)}</span>
          </li>
        ) : null}
        {summary ? (
          <li className="flex items-center gap-1.5 whitespace-nowrap text-[12px] text-muted">
            <span className="hatched inline-block size-2.5 rounded-[3px] bg-muted" aria-hidden="true" />
            <span className="text-foreground">Likely recoverable</span>
            <span className="tabular text-tier-safe">{formatBytes(totalRecoverable(summary))}</span>
          </li>
        ) : null}
      </ul>
    </div>
  );
}

/* ---------------- Figures row ---------------- */

function Figure({ label, tip, value, accent }: { label: string; tip: string; value: string; accent?: boolean }) {
  return (
    <div className="min-w-[120px]">
      <p className="flex items-center gap-1 text-[12px] text-muted">
        {label}
        <Tooltip content={tip}>
          <button type="button" aria-label={`About ${label}`} className="text-subtle hover:text-foreground">
            <Info className="size-3.5" aria-hidden="true" />
          </button>
        </Tooltip>
      </p>
      <p className={`mt-0.5 text-[20px] font-semibold leading-6 tabular ${accent ? "text-tier-safe" : "text-foreground"}`}>
        {value}
      </p>
    </div>
  );
}

/* ---------------- Lists ---------------- */

export function CategoryList({ summary }: { summary: ScanSummary }) {
  const comparison = useAppStore((s) => s.comparison);
  const rows = categoryRows(summary);
  const max = rows[0]?.bytes ?? 1;
  const deltas = new Map<Category, number>(
    (comparison?.categories ?? []).map((c) => [c.category, c.deltaBytes]),
  );
  return (
    <ul className="divide-y divide-border">
      {rows.map((row) => {
        const delta = deltas.get(row.category) ?? 0;
        return (
          <li key={row.category} className="flex items-center gap-3 py-2">
            <CategoryBadge category={row.category} className="w-[168px] shrink-0" />
            <div className="min-w-0 flex-1">
              <SizeBar fraction={row.bytes / max} colorVar={categoryMeta(row.category).colorVar} height={4} />
            </div>
            <span className="w-[92px] shrink-0 text-right text-[13px] tabular text-foreground">
              {formatBytes(row.bytes)}
            </span>
            <span className="w-[64px] shrink-0 text-right text-[12px] tabular text-muted">
              {formatPercent(summary.totalBytes ? row.bytes / summary.totalBytes : 0)}
            </span>
            {comparison ? (
              <span
                className={`w-[76px] shrink-0 text-right text-[12px] tabular ${
                  delta < 0 ? "text-tier-safe" : "text-muted"
                }`}
                title={delta !== 0 ? "Change since the previous scan" : undefined}
              >
                {delta !== 0 ? formatSignedBytes(delta) : null}
              </span>
            ) : null}
          </li>
        );
      })}
    </ul>
  );
}

/* ---------------- Since last scan ---------------- */

/**
 * Scan-over-scan delta strip. Renders only when the shell has a
 * previous scan of the same root to compare against — every number
 * comes from `get_scan_comparison`, never from local guesswork.
 * Negative (shrinking) totals are the good outcome and take the
 * safe-tier green, matching the "Likely recoverable" figure.
 */
export function SinceLastScan({ comparison }: { comparison: ScanComparison }) {
  const total = comparison.totalBytesDelta;
  const headline =
    total < 0
      ? `${formatBytes(-total)} freed since the last scan`
      : total > 0
        ? `${formatBytes(total)} more than the last scan`
        : "No change in total size since the last scan";
  return (
    <div className="grid items-center gap-x-10 gap-y-3 rounded-[10px] border border-border bg-panel-raised px-4 py-3 sm:grid-cols-[minmax(120px,auto)_minmax(120px,auto)_minmax(0,1fr)]">
      <div>
        <p className="text-[12px] text-muted">Total measured</p>
        <p
          className={`mt-0.5 text-[20px] font-semibold leading-6 tabular ${
            total < 0 ? "text-tier-safe" : "text-foreground"
          }`}
        >
          {formatSignedBytes(total)}
        </p>
      </div>
      <div>
        <p className="text-[12px] text-muted">Files</p>
        <p className="mt-0.5 text-[20px] font-semibold leading-6 tabular text-foreground">
          {formatSignedCount(comparison.fileCountDelta)}
        </p>
      </div>
      <p className="max-w-[440px] text-[12px] leading-4 text-muted">
        {headline}. Per-category changes are shown in “Where space goes” below.
      </p>
    </div>
  );
}

/* ---------------- Drives ---------------- */

/**
 * Whether the drives list earns a section of its own: hidden when
 * there is only the scan root's drive — the strip and figures above
 * already tell that drive's story.
 */
export function drivesVisible(drives: DriveInfo[], scanRoot: string): boolean {
  if (drives.length === 0) return false;
  return !(drives.length === 1 && sameRootPath(drives[0].rootPath, scanRoot));
}

/**
 * Every drive on this PC as one quiet hairline row each: label, a
 * thin used-space bar, the free caption, and — for a drive that is
 * not the scan root — a Scan action. A footnote to the Overview,
 * not a second headline.
 */
export function DrivesList({
  drives,
  scanRoot,
  onScanDrive,
}: {
  drives: DriveInfo[];
  scanRoot: string;
  onScanDrive: (rootPath: string) => void;
}) {
  if (!drivesVisible(drives, scanRoot)) return null;
  return (
    <ul className="divide-y divide-border">
      {drives.map((drive) => {
        const isScanRoot = sameRootPath(drive.rootPath, scanRoot);
        const usedFraction =
          drive.totalBytes > 0 ? Math.min(1, Math.max(0, 1 - drive.freeBytes / drive.totalBytes)) : 0;
        const label = drive.rootPath.replace(/[\\/]+$/, "");
        return (
          <li key={drive.rootPath} className="flex items-center gap-3 py-2">
            <HardDrive className="size-4 shrink-0 text-muted" aria-hidden="true" />
            <span className="w-[132px] shrink-0 truncate text-[13px] font-medium text-foreground" title={drive.rootPath}>
              {label}
            </span>
            <span className="min-w-0 flex-1">
              <SizeBar fraction={usedFraction} colorVar="var(--accent)" height={4} />
            </span>
            <span className="w-[168px] shrink-0 text-right text-[12px] tabular text-muted">
              {formatBytes(drive.freeBytes)} free of {formatBytes(drive.totalBytes)}
            </span>
            <span className="flex w-[76px] shrink-0 items-center justify-end">
              {isScanRoot ? (
                <span className="text-[11px] text-subtle">Scan root</span>
              ) : (
                <Button variant="subtle" size="sm" onClick={() => onScanDrive(drive.rootPath)}>
                  Scan
                </Button>
              )}
            </span>
          </li>
        );
      })}
    </ul>
  );
}

/* ---------------- Forecast ---------------- */

/**
 * Where the current growth rate leads, from the scan-history time
 * series. Renders only when the forecast fit used at least two
 * points — a single scan has no rate to project.
 */
export function ForecastCard({
  forecast,
  history,
  root,
}: {
  forecast: SpaceForecast;
  history: ScanHistoryEntry[];
  root: string;
}) {
  const series = useMemo<ChartSeries[]>(
    () => [
      {
        label: "Scanned size",
        color: "var(--accent)",
        points: buildSeries(
          history.filter((entry) => sameRootPath(entry.rootPath, root)),
          (entry) => ({ x: entry.scannedAtMs, y: entry.totalBytes }),
        ),
        formatY: formatAxisBytes,
      },
    ],
    [history, root],
  );
  const filling = forecast.daysUntilFull !== null && forecast.projectedFullAtMs !== null;
  return (
    <div className="rounded-[10px] border border-border bg-panel-raised px-4 py-3">
      <p className="flex items-center gap-1.5 text-[12px] text-muted">
        <TrendingUp className="size-3.5" aria-hidden="true" />
        {filling ? "Fill-up forecast" : "Growth forecast"}
      </p>
      <p className="mt-0.5 text-[15px] font-semibold leading-5 text-foreground">
        {filling
          ? `At your current rate, this drive fills in about ${Math.round(forecast.daysUntilFull as number)} days`
          : "Your disk isn't growing — no fill-up date in sight."}
      </p>
      <p className="mt-0.5 text-[12px] leading-4 text-muted">
        {filling
          ? `${formatBytes(Math.max(0, forecast.dailyGrowthBytes))} more per day on average · full around ${formatDate(forecast.projectedFullAtMs as number)}.`
          : forecast.dailyGrowthBytes < 0
            ? `Usage is shrinking by ${formatBytes(-forecast.dailyGrowthBytes)} per day on average across the last ${forecast.pointsUsed} scans.`
            : `Usage has been flat across the last ${forecast.pointsUsed} scans.`}
      </p>
      <div className="mt-3">
        <Chart
          series={series}
          height={34}
          mini
          ariaLabel="Total scanned size across recent scans"
        />
      </div>
    </div>
  );
}

/* ---------------- Biggest changes ---------------- */

function ChangeList({ title, changes, tone }: { title: string; changes: DirChange[]; tone: "grew" | "freed" }) {
  const homeDir = useAppStore((s) => s.env?.homeDir);
  if (changes.length === 0) return null;
  return (
    <div className="min-w-[240px] flex-1">
      <p className="text-[12px] font-medium text-muted">{title}</p>
      <ul className="mt-1 divide-y divide-border">
        {changes.map((change) => (
          <li key={change.path} className="flex items-baseline gap-3 py-1.5">
            <span className="min-w-0 flex-1">
              <span className="block truncate text-[13px] text-foreground" title={change.path}>
                {change.name}
              </span>
              <span className="mono-path block truncate text-subtle" title={change.path}>
                {displayPath(change.path, homeDir)}
              </span>
            </span>
            <span
              className={`shrink-0 text-[12px] tabular ${tone === "grew" ? "text-tier-review" : "text-tier-safe"}`}
            >
              {formatSignedBytes(change.deltaBytes)}
            </span>
          </li>
        ))}
      </ul>
    </div>
  );
}

/**
 * The folders behind the scan-over-scan delta: what grew most
 * (including folders that only exist now) and what freed space
 * (shrunk plus removed). One callout panel belonging to the
 * Since-last-scan section — a titled sub-section of the delta,
 * not a separate floating block. Renders nothing when the diff
 * is empty.
 */
export function BiggestChanges({ changes }: { changes: BiggestChanges }) {
  const grew = useMemo(
    () => [...changes.grown, ...changes.added].sort((a, b) => b.deltaBytes - a.deltaBytes).slice(0, 5),
    [changes],
  );
  const freed = useMemo(
    () => [...changes.shrunk, ...changes.removed].sort((a, b) => a.deltaBytes - b.deltaBytes).slice(0, 5),
    [changes],
  );
  if (grew.length === 0 && freed.length === 0) return null;
  return (
    <div className="mt-3 rounded-[10px] border border-border bg-panel-raised px-4 py-3">
      <p className="text-[13px] font-semibold leading-[18px] text-foreground">Biggest changes</p>
      <div className="mt-2 flex flex-wrap gap-x-10 gap-y-3">
        <ChangeList title="Grew most" changes={grew} tone="grew" />
        <ChangeList title="Freed" changes={freed} tone="freed" />
      </div>
    </div>
  );
}

/* ---------------- OneDrive / cloud ---------------- */

/**
 * The cloud sync folder's local footprint, with the online-only
 * escape hatch. Renders only when the shell found a cloud folder.
 */
export function OneDriveCard({
  cloud,
  onChanged,
}: {
  cloud: CloudStatus;
  onChanged: () => void;
}) {
  const homeDir = useAppStore((s) => s.env?.homeDir);
  const setBanner = useAppStore((s) => s.setBanner);
  const [confirmOpen, setConfirmOpen] = useState(false);
  return (
    <div className="flex items-center gap-3 rounded-[10px] border border-border bg-panel-raised px-4 py-3">
      <Cloud className="size-4 shrink-0 text-accent" aria-hidden="true" />
      <p className="min-w-0 flex-1 text-[12px] leading-4 text-muted">
        <span className="font-medium text-foreground">
          {cloud.provider} is keeping {formatBytes(cloud.localBytes)} on this PC
        </span>{" "}
        — {formatCount(cloud.fileCount)} file{cloud.fileCount === 1 ? "" : "s"} stored locally in{" "}
        <span className="mono-path">{displayPath(cloud.rootPath, homeDir)}</span>. Making them
        online-only frees the space; they download again when opened.
      </p>
      <Button variant="secondary" size="sm" onClick={() => setConfirmOpen(true)}>
        Free up space
      </Button>
      <FreeUpSpaceDialog
        open={confirmOpen}
        provider={cloud.provider}
        paths={[cloud.rootPath]}
        bytes={cloud.localBytes}
        onClose={() => setConfirmOpen(false)}
        onDone={(result) => {
          if (result.failed.length > 0) {
            setBanner({
              kind: "error",
              message: `Could not free up space in ${cloud.provider}: ${result.failed[0].error}`,
            });
          } else {
            setBanner({
              kind: "success",
              message: `Freed up space for ${result.succeeded.length} item${result.succeeded.length === 1 ? "" : "s"} in ${cloud.provider}. The files stay in ${cloud.provider} and download again when opened.`,
            });
          }
          onChanged();
        }}
      />
    </div>
  );
}

export function LocationList({ summary, homeDir }: { summary: ScanSummary; homeDir?: string }) {
  const locations = sortLocations(summary.locations).slice(0, 8);
  const max = locations[0]?.bytes ?? 1;
  if (locations.length === 0) {
    return <p className="text-[12px] text-muted">No locations were measured in this scan.</p>;
  }
  return (
    <ul className="divide-y divide-border">
      {locations.map((loc) => (
        <li key={loc.path} className="flex items-center gap-3 py-2">
          <div className="min-w-0 flex-1">
            <p className="truncate text-[13px] text-foreground" title={loc.path}>{loc.name}</p>
            <p className="mono-path truncate text-subtle" title={loc.path}>{displayPath(loc.path, homeDir)}</p>
          </div>
          <div className="hidden w-[140px] shrink-0 md:block">
            <SizeBar fraction={loc.bytes / max} colorVar={categoryMeta(loc.category).colorVar} height={4} />
          </div>
          <CategoryBadge category={loc.category} className="hidden w-[140px] shrink-0 lg:flex" />
          <span className="w-[92px] shrink-0 text-right text-[13px] tabular text-foreground">
            {formatBytes(loc.bytes)}
          </span>
        </li>
      ))}
    </ul>
  );
}

/* ---------------- Notices & details ---------------- */

export function ScanDetails({ summary, homeDir }: { summary: ScanSummary; homeDir?: string }) {
  return (
    <dl className="grid grid-cols-2 gap-x-8 gap-y-1.5 text-[12px] leading-4 md:grid-cols-3">
      <div><dt className="text-muted">Scan root</dt><dd className="mono-path mt-0.5 text-foreground">{displayPath(summary.root, homeDir)}</dd></div>
      <div><dt className="text-muted">Files</dt><dd className="tabular text-foreground">{formatCount(summary.fileCount)}</dd></div>
      <div><dt className="text-muted">Folders</dt><dd className="tabular text-foreground">{formatCount(summary.dirCount)}</dd></div>
      <div><dt className="text-muted">Duration</dt><dd className="tabular text-foreground">{formatDuration(summary.durationMs)}</dd></div>
      <div><dt className="text-muted">Finished</dt><dd className="text-foreground">{formatDate(summary.finishedAtMs)}</dd></div>
      <div><dt className="text-muted">Measured</dt><dd className="tabular text-foreground">{formatBytes(summary.totalBytes)}</dd></div>
      {summary.skipped.length > 0 ? (
        <div className="col-span-full">
          <dt className="text-muted">Skipped or unreadable folders ({summary.skipped.length})</dt>
          <dd>
            <ul className="mt-1 max-h-[120px] space-y-0.5 overflow-y-auto">
              {summary.skipped.map((path) => (
                <li key={path} className="mono-path text-subtle">{displayPath(path, homeDir)}</li>
              ))}
            </ul>
          </dd>
        </div>
      ) : null}
      {summary.errors.length > 0 ? (
        <div className="col-span-full">
          <dt className="text-muted">Errors</dt>
          <dd>
            <ul className="mt-1 space-y-0.5">
              {summary.errors.map((err, i) => (
                <li key={i} className="text-[12px] text-critical">
                  <span className="mono-path">{displayPath(err.path, homeDir)}</span>
                  {err.message ? <> — {err.message}</> : null}
                </li>
              ))}
            </ul>
          </dd>
        </div>
      ) : null}
    </dl>
  );
}

export function PrivateFoldersNotice({ summary }: { summary: ScanSummary }) {
  if (summary.skipped.length === 0) return null;
  return (
    <div className="flex gap-3 rounded-[10px] border border-border bg-panel-raised px-4 py-3">
      <FolderLock className="mt-0.5 size-4 shrink-0 text-muted" aria-hidden="true" />
      <div>
        <p className="text-[13px] font-medium text-foreground">Some folders were not measured</p>
        <p className="mt-0.5 text-[12px] leading-4 text-muted">
          {summary.skipped.length} folder{summary.skipped.length === 1 ? " was" : "s were"} skipped because
          Windows or the app that owns them restricts access. Their contents are not counted in any total
          or recommendation.
        </p>
      </div>
    </div>
  );
}

export function TrashWaitingNotice() {
  const setRecycleBinOpen = useAppStore((s) => s.setRecycleBinOpen);
  const [items, setItems] = useState<RecycleBinItem[]>([]);
  useEffect(() => {
    let live = true;
    void api.getRecycleBinItems().then((rows) => { if (live) setItems(rows); });
    return () => { live = false; };
  }, []);
  const bytes = items.reduce((acc, i) => acc + i.bytes, 0);
  if (items.length === 0) return null;
  return (
    <div className="flex items-center gap-3 rounded-[10px] border border-border bg-panel-raised px-4 py-3">
      <Trash2 className="size-4 shrink-0 text-muted" aria-hidden="true" />
      <p className="flex-1 text-[12px] leading-4 text-muted">
        <span className="font-medium text-foreground">{formatBytes(bytes)}</span> is sitting in the Recycle
        Bin. That space is not freed until the Recycle Bin is emptied.
      </p>
      <Button variant="secondary" size="sm" onClick={() => setRecycleBinOpen(true)}>
        Review Recycle Bin
      </Button>
    </div>
  );
}

export function StartHere() {
  const recommendations = useAppStore((s) => s.recommendations);
  const navigate = useAppStore((s) => s.navigate);
  const top = [...recommendations].filter((r) => r.tier !== "keep").sort((a, b) => b.bytes - a.bytes).slice(0, 3);
  if (top.length === 0) {
    return <p className="text-[12px] text-muted">Nothing is flagged for cleanup right now. A fresh scan keeps this list current.</p>;
  }
  return (
    <ul className="divide-y divide-border">
      {top.map((rec) => (
        <li key={rec.id}>
          <button
            type="button"
            onClick={() => navigate("cleanup")}
            className="flex w-full items-center gap-3 py-2.5 text-left transition-colors duration-120 hover:bg-hover"
          >
            <div className="min-w-0 flex-1">
              <p className="truncate text-[13px] font-medium text-foreground">{rec.title}</p>
              <p className="mt-0.5 line-clamp-1 text-[12px] text-muted">{rec.whyDetected}</p>
            </div>
            <TierBadge tier={rec.tier} />
            <span className="w-[92px] shrink-0 text-right text-[13px] tabular text-foreground">{formatBytes(rec.bytes)}</span>
            <ArrowRight className="size-4 shrink-0 text-subtle" aria-hidden="true" />
          </button>
        </li>
      ))}
    </ul>
  );
}

export function AssistantPrompt() {
  const assistantEnabled = useAppStore((s) => s.settings.assistantEnabled);
  const navigate = useAppStore((s) => s.navigate);
  if (!assistantEnabled) return null;
  return (
    <div className="flex items-center gap-3 rounded-[10px] border border-border bg-panel-raised px-4 py-3">
      <Bot className="size-4 shrink-0 text-accent" aria-hidden="true" />
      <p className="flex-1 text-[12px] leading-4 text-muted">
        Ask the assistant about this scan — what is using space, and what is safest to clean first.
      </p>
      <Button variant="subtle" size="sm" onClick={() => navigate("assistant")}>
        Open Assistant
        <ArrowRight className="size-3.5" aria-hidden="true" />
      </Button>
    </div>
  );
}

/* ---------------- Page ---------------- */

export function OverviewPage() {
  const summary = useAppStore((s) => s.summary);
  const comparison = useAppStore((s) => s.comparison);
  const disk = useAppStore((s) => s.disk);
  const scanning = useAppStore((s) => s.scanning);
  const homeDir = useAppStore((s) => s.env?.homeDir);
  const navigate = useAppStore((s) => s.navigate);
  const recommendations = useAppStore((s) => s.recommendations);
  const settings = useAppStore((s) => s.settings);
  const updateSettings = useAppStore((s) => s.updateSettings);
  const startScan = useAppStore((s) => s.startScan);
  const refreshDisk = useAppStore((s) => s.refreshDisk);
  const setBanner = useAppStore((s) => s.setBanner);

  const [drives, setDrives] = useState<DriveInfo[]>([]);
  const [forecast, setForecast] = useState<SpaceForecast | null>(null);
  const [history, setHistory] = useState<ScanHistoryEntry[]>([]);
  const [changes, setChanges] = useState<BiggestChanges | null>(null);
  const [cloud, setCloud] = useState<CloudStatus | null>(null);
  const [exporting, setExporting] = useState(false);

  /* Watch data (drives, forecast, history, biggest changes, cloud)
     refreshes whenever a scan finishes — the finished timestamp is
     the trigger, the loaders themselves are best-effort. */
  const finishedAtMs = summary?.finishedAtMs;
  useEffect(() => {
    let live = true;
    void (async () => {
      const [d, f, h, c, cl] = await Promise.all([
        api.getDrives(),
        api.getSpaceForecast(),
        api.getScanHistory(),
        api.getBiggestChanges(5),
        api.getCloudStatus(),
      ]);
      if (!live) return;
      setDrives(d);
      setForecast(f);
      setHistory(h);
      setChanges(c);
      setCloud(cl);
    })();
    return () => {
      live = false;
    };
  }, [finishedAtMs]);

  const shown = summary;

  const scanDrive = async (rootPath: string) => {
    await updateSettings({ scanRoot: rootPath });
    await startScan(rootPath);
  };

  const exportReport = async () => {
    if (!summary || exporting) return;
    setExporting(true);
    try {
      const html = buildReportHtml({
        summary,
        disk,
        forecast,
        recommendations,
        generatedAtMs: Date.now(),
      });
      const path = await api.exportReport(html);
      if (path) {
        setBanner({ kind: "success", message: `Report saved to ${path}` });
      }
    } finally {
      setExporting(false);
    }
  };

  return (
    <div className="h-full overflow-y-auto px-6 py-6">
      <div className="max-w-[880px]">
      <div>
        <div className="flex items-start justify-between gap-4">
          <h1 className="text-[28px] font-semibold leading-8 tracking-[-0.01em] text-foreground">
            {disk ? `${formatBytes(disk.availableBytes)} available` : "Storage overview"}
          </h1>
          <div className="flex shrink-0 items-center gap-2">
            {summary ? (
              <Button variant="secondary" onClick={() => void exportReport()} disabled={exporting}>
                <FileDown className="size-4" aria-hidden="true" />
                {exporting ? "Exporting…" : "Export report"}
              </Button>
            ) : null}
            <ScanControl />
          </div>
        </div>
        <p className="mt-1.5 flex items-center gap-2 text-[12px] text-muted">
          <HardDrive className="size-3.5 shrink-0" aria-hidden="true" />
          {disk ? (
            <>
              <span className="min-w-0 truncate">
                {`${disk.volumeName || "System drive"} · ${formatBytes(disk.totalBytes)} total`}
              </span>
              {summary ? (
                <span className="shrink-0">
                  <ResultsAge finishedAtMs={summary.finishedAtMs} />
                </span>
              ) : null}
            </>
          ) : (
            "Run a scan to understand your storage."
          )}
        </p>
      </div>

      <div className="mt-5">
        <StorageStrip disk={disk} summary={shown} />
      </div>

      {disk ? (
        <div className="mt-5 flex flex-wrap gap-x-10 gap-y-4">
          <Figure label="Used" tip="Space currently occupied on this drive, including files outside the scanned folder." value={formatBytes(disk.usedBytes)} />
          <Figure label="Available" tip="Free space Windows can use right now for new files and updates." value={formatBytes(disk.availableBytes)} />
          <Figure label="Likely recoverable" tip="Space DiskPilot believes can be reclaimed safely from the scanned folder — caches, logs, and regenerable build output." value={summary ? formatBytes(totalRecoverable(summary)) : "—"} accent />
          <Figure label="Scanned" tip="Total size measured inside the scan root during the most recent scan." value={summary ? formatBytes(summary.totalBytes) : "—"} />
        </div>
      ) : null}

      {!summary && !scanning ? (
        <div className="mt-6">
          <EmptyState
            icon={ScanSearch}
            title="No scan yet"
            description="A scan measures every folder under your scan root, classifies what it finds, and explains what is safe to clean. Scanning is read-only — nothing is moved or deleted."
            action={<ScanControl compact />}
          />
        </div>
      ) : null}

      {summary ? (
        <>
          {comparison ? (
            <Section
              title="Since last scan"
              description={`Compared with the previous scan, finished ${formatDate(comparison.previousScannedAtMs)}.`}
            >
              <SinceLastScan comparison={comparison} />
              {changes ? <BiggestChanges changes={changes} /> : null}
            </Section>
          ) : null}

          {forecast && forecast.pointsUsed >= 2 ? (
            <Section title="Forecast" description="Projected from your scan history — the trend, not a promise.">
              <ForecastCard forecast={forecast} history={history} root={summary.root} />
            </Section>
          ) : null}

          {drivesVisible(drives, settings.scanRoot) ? (
            <Section title="Drives" description="Every drive on this PC, and how full each one is.">
              <DrivesList drives={drives} scanRoot={settings.scanRoot} onScanDrive={(root) => void scanDrive(root)} />
            </Section>
          ) : null}

          <Section
            title="Start here"
            description="The items most worth reviewing first."
            action={
              <Button variant="subtle" size="sm" onClick={() => navigate("cleanup")}>
                <Sparkles className="size-3.5" aria-hidden="true" />
                Open Cleanup
              </Button>
            }
          >
            <StartHere />
          </Section>

          <Section title="Where space goes" description="Measured size by category. Hatching in the strip above marks the likely recoverable part.">
            <CategoryList summary={summary} />
          </Section>

          <Section title="Largest consumers">
            <LocationList summary={summary} homeDir={homeDir} />
          </Section>

          <div className="mt-6 space-y-3">
            <PrivateFoldersNotice summary={summary} />
            {cloud ? (
              <OneDriveCard
                cloud={cloud}
                onChanged={() => {
                  void api.getCloudStatus().then(setCloud);
                  void refreshDisk();
                }}
              />
            ) : null}
            <TrashWaitingNotice />
            <AssistantPrompt />
          </div>

          <Section title="Scan details">
            <ScanDetails summary={summary} homeDir={homeDir} />
          </Section>
        </>
      ) : null}
      </div>
    </div>
  );
}
