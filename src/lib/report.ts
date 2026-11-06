import { categoryRows, sortLocations, totalRecoverable } from "./analysis";
import { categoryMeta } from "./categories";
import { formatBytes, formatCount, formatDate, formatPercent } from "./format";
import { tierMeta } from "./tiers";
import type {
  DiskInfo,
  Recommendation,
  ScanSummary,
  SpaceForecast,
} from "./types";

/* Standalone HTML storage report builder. Pure: it takes only data
   the Overview already loaded and returns one self-contained HTML
   string (inline CSS, no external assets) that the shell saves
   through `export_report`. Kept free of store/API imports so it can
   be unit-tested directly. */

export interface ReportData {
  summary: ScanSummary;
  disk: DiskInfo | null;
  forecast: SpaceForecast | null;
  recommendations: Recommendation[];
  generatedAtMs: number;
}

function escapeHtml(text: string): string {
  return text
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#39;");
}

const REPORT_CSS = `
  :root { color-scheme: light; }
  * { box-sizing: border-box; }
  body { margin: 0; padding: 40px 24px; background: #f5f5f7; color: #1d1d1f;
    font-family: "Segoe UI", -apple-system, "Helvetica Neue", Arial, sans-serif; font-size: 14px; line-height: 1.5; }
  main { max-width: 860px; margin: 0 auto; background: #ffffff; border: 1px solid #e5e5ea;
    border-radius: 12px; padding: 36px 40px 40px; }
  header { display: flex; align-items: baseline; justify-content: space-between; gap: 16px;
    border-bottom: 1px solid #e5e5ea; padding-bottom: 16px; }
  .wordmark { font-size: 22px; font-weight: 600; letter-spacing: -0.01em; }
  .wordmark span { color: #5856d6; }
  .generated { color: #6e6e73; font-size: 12px; white-space: nowrap; }
  h1 { font-size: 20px; font-weight: 600; margin: 24px 0 4px; }
  h2 { font-size: 15px; font-weight: 600; margin: 28px 0 8px; }
  p.lede { color: #6e6e73; margin: 0 0 8px; }
  .figures { display: flex; flex-wrap: wrap; gap: 12px 40px; margin: 16px 0 4px; }
  .figure .label { color: #6e6e73; font-size: 12px; }
  .figure .value { font-size: 20px; font-weight: 600; font-variant-numeric: tabular-nums; }
  .figure .value.safe { color: #248a3d; }
  table { width: 100%; border-collapse: collapse; font-variant-numeric: tabular-nums; }
  th, td { text-align: left; padding: 6px 8px; border-bottom: 1px solid #e5e5ea; }
  th { color: #6e6e73; font-weight: 500; font-size: 12px; }
  td.num, th.num { text-align: right; }
  .path { font-family: Consolas, "Cascadia Code", monospace; font-size: 12px; color: #424245; word-break: break-all; }
  .forecast { background: #f5f5f7; border: 1px solid #e5e5ea; border-radius: 10px; padding: 12px 16px; }
  footer { margin-top: 32px; border-top: 1px solid #e5e5ea; padding-top: 12px; color: #6e6e73; font-size: 12px; }
  @media print { body { background: #ffffff; padding: 0; } main { border: none; border-radius: 0; } }
`;

export function buildReportHtml(data: ReportData): string {
  const { summary, disk, forecast, recommendations } = data;
  const rows = categoryRows(summary);
  const folders = sortLocations(
    summary.heavyFolders.length > 0 ? summary.heavyFolders : summary.locations,
  ).slice(0, 10);
  const topRecommendations = [...recommendations]
    .filter((r) => r.tier !== "keep")
    .sort((a, b) => b.bytes - a.bytes)
    .slice(0, 10);

  const categoryTable = rows
    .map((row) => {
      const meta = categoryMeta(row.category);
      const share = summary.totalBytes > 0 ? formatPercent(row.bytes / summary.totalBytes) : "0%";
      return `<tr><td>${escapeHtml(meta.label)}</td><td class="num">${formatBytes(row.bytes)}</td><td class="num">${share}</td><td class="num">${formatBytes(row.recoverable)}</td></tr>`;
    })
    .join("\n");

  const folderRows = folders
    .map(
      (loc) =>
        `<tr><td>${escapeHtml(loc.name)}<br><span class="path">${escapeHtml(loc.path)}</span></td><td class="num">${formatBytes(loc.bytes)}</td></tr>`,
    )
    .join("\n");

  const recommendationRows = topRecommendations
    .map(
      (rec) =>
        `<tr><td>${escapeHtml(rec.title)}</td><td>${escapeHtml(tierMeta(rec.tier).label)}</td><td class="num">${formatBytes(rec.bytes)}</td></tr>`,
    )
    .join("\n");

  const forecastSection =
    forecast && forecast.pointsUsed >= 2
      ? forecast.daysUntilFull !== null && forecast.projectedFullAtMs !== null
        ? `<h2>Forecast</h2><p class="forecast">At the current rate of ${formatBytes(Math.max(0, forecast.dailyGrowthBytes))} per day, this drive fills in about ${Math.round(forecast.daysUntilFull)} days — around ${formatDate(forecast.projectedFullAtMs)}.</p>`
        : `<h2>Forecast</h2><p class="forecast">Your disk isn't growing — no fill-up date in sight.</p>`
      : "";

  return `<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>DiskPilot storage report — ${escapeHtml(formatDate(data.generatedAtMs))}</title>
<style>${REPORT_CSS}</style>
</head>
<body>
<main>
  <header>
    <div class="wordmark">Disk<span>Pilot</span></div>
    <div class="generated">Generated ${escapeHtml(formatDate(data.generatedAtMs))}</div>
  </header>

  <h1>Storage report</h1>
  <p class="lede">Scan of <span class="path">${escapeHtml(summary.root)}</span>, finished ${escapeHtml(formatDate(summary.finishedAtMs))} — ${formatCount(summary.fileCount)} files in ${formatCount(summary.dirCount)} folders.</p>

  <div class="figures">
    ${disk ? `<div class="figure"><div class="label">Available</div><div class="value">${formatBytes(disk.availableBytes)}</div></div>
    <div class="figure"><div class="label">Used</div><div class="value">${formatBytes(disk.usedBytes)}</div></div>` : ""}
    <div class="figure"><div class="label">Scanned</div><div class="value">${formatBytes(summary.totalBytes)}</div></div>
    <div class="figure"><div class="label">Likely recoverable</div><div class="value safe">${formatBytes(totalRecoverable(summary))}</div></div>
  </div>

  <h2>Where space goes</h2>
  <table>
    <thead><tr><th>Category</th><th class="num">Size</th><th class="num">Share</th><th class="num">Recoverable</th></tr></thead>
    <tbody>
${categoryTable}
    </tbody>
  </table>

  ${folders.length > 0 ? `<h2>Largest folders</h2>
  <table>
    <thead><tr><th>Folder</th><th class="num">Size</th></tr></thead>
    <tbody>
${folderRows}
    </tbody>
  </table>` : ""}

  ${forecastSection}

  ${topRecommendations.length > 0 ? `<h2>Top recommendations</h2>
  <table>
    <thead><tr><th>Recommendation</th><th>Tier</th><th class="num">Reclaimable</th></tr></thead>
    <tbody>
${recommendationRows}
    </tbody>
  </table>` : ""}

  <footer>Generated by DiskPilot from a local, read-only scan. Nothing in this report was uploaded anywhere.</footer>
</main>
</body>
</html>
`;
}
