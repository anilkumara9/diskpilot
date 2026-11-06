import { useEffect, useState } from "react";
import {
  ArrowRight,
  Bot,
  FolderLock,
  HardDrive,
  Info,
  ScanSearch,
  Sparkles,
  Trash2,
} from "lucide-react";
import { useAppStore } from "@/lib/store";
import * as api from "@/lib/api";
import { categoryRows, sortLocations, totalRecoverable } from "@/lib/analysis";
import { categoryMeta } from "@/lib/categories";
import { displayPath, formatBytes, formatCount, formatDate, formatDuration, formatPercent, formatSignedBytes, formatSignedCount } from "@/lib/format";
import type { Category, DiskInfo, RecycleBinItem, ScanComparison, ScanSummary } from "@/lib/types";
import { Button } from "@/components/ui/button";
import { Section } from "@/components/ui/section";
import { Tooltip } from "@/components/ui/tooltip";
import { CategoryBadge, CategoryDot } from "@/components/shared/CategoryBadge";
import { TierBadge } from "@/components/shared/TierBadge";
import { SizeBar } from "@/components/shared/SizeBar";
import { EmptyState } from "@/components/shared/EmptyState";
import { ResultsAge } from "@/components/shared/ResultsAge";
import { ScanControl } from "@/components/layout/ScanControl";

/* ---------------- StorageStrip ---------------- */

export function StorageStrip({
  disk,
  summary,
}: {
  disk: DiskInfo | null;
  summary: ScanSummary | null;
}) {
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

  return (
    <div>
      <div
        className="flex h-7 w-full overflow-hidden rounded-md bg-panel-raised"
        role="img"
        aria-label="Disk usage by category"
      >
        {rows.map((row) => {
          const width = total > 0 ? (row.bytes / total) * 100 : 0;
          if (width <= 0) return null;
          const recoverableWidth = row.bytes > 0 ? (row.recoverable / row.bytes) * 100 : 0;
          return (
            <div
              key={row.category}
              className="relative h-full shrink-0"
              style={{ width: `${width}%`, background: categoryMeta(row.category).colorVar }}
              title={`${categoryMeta(row.category).label}: ${formatBytes(row.bytes)}`}
            >
              {row.recoverable > 0 ? (
                <div
                  className="hatched absolute inset-y-0 right-0"
                  style={{ width: `${recoverableWidth}%` }}
                  title={`Likely recoverable: ${formatBytes(row.recoverable)}`}
                />
              ) : null}
            </div>
          );
        })}
        {otherUsed > 0 && total > 0 ? (
          <div
            className="h-full shrink-0 bg-border-strong"
            style={{ width: `${(otherUsed / total) * 100}%` }}
            title={`Other used space: ${formatBytes(otherUsed)}`}
          />
        ) : null}
        {free > 0 && total > 0 ? (
          <div className="h-full flex-1 bg-transparent" title={`Available: ${formatBytes(free)}`} />
        ) : null}
      </div>

      <ul className="mt-2.5 flex flex-wrap gap-x-4 gap-y-1.5">
        {rows.map((row) => (
          <li key={row.category} className="flex items-center gap-1.5 text-[12px] text-muted">
            <CategoryDot category={row.category} />
            <span className="text-foreground">{categoryMeta(row.category).label}</span>
            <span className="tabular">{formatBytes(row.bytes)}</span>
            {row.recoverable > 0 ? (
              <span className="tabular text-tier-safe">({formatBytes(row.recoverable)} recoverable)</span>
            ) : null}
          </li>
        ))}
        {otherUsed > 0 ? (
          <li className="flex items-center gap-1.5 text-[12px] text-muted">
            <span className="inline-block size-2 rounded-full bg-border-strong" aria-hidden="true" />
            <span className="text-foreground">Other used space</span>
            <span className="tabular">{formatBytes(otherUsed)}</span>
          </li>
        ) : null}
        {disk ? (
          <li className="flex items-center gap-1.5 text-[12px] text-muted">
            <span className="inline-block size-2 rounded-full border border-border-strong" aria-hidden="true" />
            <span className="text-foreground">Available</span>
            <span className="tabular">{formatBytes(disk.availableBytes)}</span>
          </li>
        ) : null}
        {summary ? (
          <li className="flex items-center gap-1.5 text-[12px] text-muted">
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
    <div className="flex flex-wrap items-center gap-x-10 gap-y-3">
      <div className="min-w-[120px]">
        <p className="text-[12px] text-muted">Total measured</p>
        <p
          className={`mt-0.5 text-[20px] font-semibold leading-6 tabular ${
            total < 0 ? "text-tier-safe" : "text-foreground"
          }`}
        >
          {formatSignedBytes(total)}
        </p>
      </div>
      <div className="min-w-[120px]">
        <p className="text-[12px] text-muted">Files</p>
        <p className="mt-0.5 text-[20px] font-semibold leading-6 tabular text-foreground">
          {formatSignedCount(comparison.fileCountDelta)}
        </p>
      </div>
      <p className="min-w-0 flex-1 text-[12px] leading-4 text-muted">
        {headline}. Per-category changes are shown in “Where space goes” below.
      </p>
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
    <div className="flex items-center gap-3 rounded-[10px] border border-border px-4 py-3">
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

  const shown = summary;

  return (
    <div className="mx-auto max-w-[880px] px-6 py-6">
      <div className="flex items-start justify-between gap-4">
        <div>
          <h1 className="text-[28px] font-semibold leading-8 tracking-[-0.01em] text-foreground">
            {disk ? `${formatBytes(disk.availableBytes)} available` : "Storage overview"}
          </h1>
          <p className="mt-1 flex items-center gap-2 text-[12px] text-muted">
            <HardDrive className="size-3.5" aria-hidden="true" />
            {disk ? `${disk.volumeName || "System drive"} · ${formatBytes(disk.totalBytes)} total` : "Run a scan to understand your storage."}
            {summary ? <ResultsAge finishedAtMs={summary.finishedAtMs} /> : null}
          </p>
        </div>
        <ScanControl />
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
            <TrashWaitingNotice />
            <AssistantPrompt />
          </div>

          <Section title="Scan details">
            <ScanDetails summary={summary} homeDir={homeDir} />
          </Section>
        </>
      ) : null}
    </div>
  );
}
