import { describe, expect, it } from "vitest";
import { buildReportHtml, type ReportData } from "./report";
import type { Recommendation, ScanSummary } from "./types";

function makeSummary(overrides: Partial<ScanSummary> = {}): ScanSummary {
  return {
    root: "C:\\Users\\me",
    startedAtMs: 1_700_000_000_000,
    finishedAtMs: 1_700_000_060_000,
    durationMs: 60_000,
    totalBytes: 10_000_000_000,
    fileCount: 12_345,
    dirCount: 678,
    categoryTotals: {
      cache: 2_000_000_000,
      buildArtifacts: 0,
      developerTools: 0,
      dockerStorage: 0,
      applicationData: 0,
      logs: 0,
      downloads: 3_000_000_000,
      userData: 5_000_000_000,
      binaryData: 0,
      systemFiles: 0,
      unknown: 0,
    },
    recoverableByCategory: {
      cache: 1_500_000_000,
      buildArtifacts: 0,
      developerTools: 0,
      dockerStorage: 0,
      applicationData: 0,
      logs: 0,
      downloads: 0,
      userData: 0,
      binaryData: 0,
      systemFiles: 0,
      unknown: 0,
    },
    topFiles: [],
    locations: [
      { path: "C:\\Users\\me\\Downloads", name: "Downloads", bytes: 3_000_000_000, category: "downloads" },
    ],
    heavyFolders: [],
    errors: [],
    skipped: [],
    ...overrides,
  };
}

function makeRecommendation(overrides: Partial<Recommendation> = {}): Recommendation {
  return {
    id: "rec-1",
    title: "Clear the npm cache",
    paths: ["C:\\Users\\me\\AppData\\Local\\npm-cache"],
    bytes: 1_500_000_000,
    tier: "likelySafe",
    methods: [],
    whyDetected: "Cache folder",
    whySafe: "Regenerable",
    risks: "None",
    confidence: "high",
    source: "Cache rule",
    ...overrides,
  };
}

function makeData(overrides: Partial<ReportData> = {}): ReportData {
  return {
    summary: makeSummary(),
    disk: {
      totalBytes: 500_000_000_000,
      usedBytes: 320_000_000_000,
      availableBytes: 180_000_000_000,
      volumeName: "Windows",
    },
    forecast: null,
    recommendations: [makeRecommendation()],
    generatedAtMs: 1_700_000_120_000,
    ...overrides,
  };
}

describe("buildReportHtml", () => {
  it("is a standalone document with the DiskPilot wordmark and generated date", () => {
    const html = buildReportHtml(makeData());
    expect(html.startsWith("<!DOCTYPE html>")).toBe(true);
    expect(html).toContain("Disk<span>Pilot</span>");
    expect(html).toContain("Generated");
    expect(html).toContain("<style>");
    expect(html).not.toContain("<script");
    expect(html).not.toContain("http://");
    expect(html).not.toContain("https://");
  });

  it("includes totals, the category breakdown, largest folders and recommendations", () => {
    const html = buildReportHtml(makeData());
    expect(html).toContain("Likely recoverable");
    // formatBytes of the recoverable total (1.5 GB)
    expect(html).toContain("1.5 GB");
    expect(html).toContain("Cache");
    expect(html).toContain("Downloads");
    expect(html).toContain("C:\\Users\\me\\Downloads");
    expect(html).toContain("Clear the npm cache");
    expect(html).toContain("Likely safe");
  });

  it("omits the forecast section without enough history points", () => {
    const html = buildReportHtml(
      makeData({ forecast: { dailyGrowthBytes: 5, daysUntilFull: 12, projectedFullAtMs: 1, pointsUsed: 1 } }),
    );
    expect(html).not.toContain("Forecast");
  });

  it("includes the forecast line when the drive is filling", () => {
    const html = buildReportHtml(
      makeData({
        forecast: {
          dailyGrowthBytes: 1_000_000_000,
          daysUntilFull: 42.4,
          projectedFullAtMs: 1_700_100_000_000,
          pointsUsed: 5,
        },
      }),
    );
    expect(html).toContain("Forecast");
    expect(html).toContain("fills in about 42 days");
  });

  it("uses the not-growing forecast copy when there is no fill-up date", () => {
    const html = buildReportHtml(
      makeData({
        forecast: { dailyGrowthBytes: -10, daysUntilFull: null, projectedFullAtMs: null, pointsUsed: 3 },
      }),
    );
    expect(html).toContain("no fill-up date in sight");
  });

  it("escapes markup in titles and paths", () => {
    const html = buildReportHtml(
      makeData({
        recommendations: [makeRecommendation({ title: "<script>alert(1)</script>" })],
        summary: makeSummary({ root: "C:\\Users\\<me>" }),
      }),
    );
    expect(html).not.toContain("<script>alert(1)</script>");
    expect(html).toContain("&lt;script&gt;alert(1)&lt;/script&gt;");
    expect(html).toContain("C:\\Users\\&lt;me&gt;");
  });

  it("excludes Keep-tier recommendations from the top list", () => {
    const html = buildReportHtml(
      makeData({ recommendations: [makeRecommendation({ tier: "keep", title: "Keep this one" })] }),
    );
    expect(html).not.toContain("Keep this one");
  });
});
