import { describe, expect, it } from "vitest";
import {
  actionableBytes,
  actionableCount,
  categoryRows,
  dedupeNested,
  filterByQuery,
  groupRecommendationsByTier,
  sortExplorerRows,
  totalRecoverable,
} from "./analysis";
import { breadcrumbsBetween, entriesToRows } from "@/features/explorer/ExplorerPage";
import type { DirectoryEntry, Recommendation, ScanSummary } from "./types";

function emptyTotals(): ScanSummary["categoryTotals"] {
  return {
    cache: 0, buildArtifacts: 0, developerTools: 0, dockerStorage: 0,
    applicationData: 0, logs: 0, downloads: 0, userData: 0,
    binaryData: 0, systemFiles: 0, unknown: 0,
  };
}

function makeSummary(partial: Partial<ScanSummary>): ScanSummary {
  return {
    root: "C:\\Users\\alex",
    startedAtMs: 1, finishedAtMs: 2, durationMs: 1,
    totalBytes: 0, fileCount: 0, dirCount: 0,
    categoryTotals: emptyTotals(),
    recoverableByCategory: emptyTotals(),
    topFiles: [], locations: [], heavyFolders: [],
    errors: [], skipped: [],
    ...partial,
  };
}

describe("categoryRows / totalRecoverable", () => {
  it("returns only non-empty categories sorted by size", () => {
    const summary = makeSummary({
      categoryTotals: { ...emptyTotals(), cache: 500, userData: 2_000 },
      recoverableByCategory: { ...emptyTotals(), cache: 400 },
    });
    const rows = categoryRows(summary);
    expect(rows.map((r) => r.category)).toEqual(["userData", "cache"]);
    expect(totalRecoverable(summary)).toBe(400);
  });
});

describe("dedupeNested", () => {
  it("drops locations contained inside other locations (nesting-safe)", () => {
    const rows = dedupeNested([
      { path: "C:\\Users\\alex", bytes: 100 },
      { path: "C:\\Users\\alex\\Projects", bytes: 60 },
      { path: "D:\\Data", bytes: 10 },
    ]);
    expect(rows.map((r) => r.path)).toEqual(["C:\\Users\\alex", "D:\\Data"]);
  });
});

describe("recommendation grouping", () => {
  const rec = (id: string, tier: Recommendation["tier"], bytes: number): Recommendation => ({
    id, title: id, paths: [], bytes, tier, methods: [],
    whyDetected: "", whySafe: "", risks: "", confidence: "high",
    alternativeCommand: null, source: "test",
  });
  it("groups in tier order and counts actionable items", () => {
    const groups = groupRecommendationsByTier([
      rec("a", "keep", 10), rec("b", "likelySafe", 50), rec("c", "reviewFirst", 30),
    ]);
    expect(groups.map((g) => g.tier)).toEqual(["likelySafe", "reviewFirst", "keep"]);
    expect(actionableCount([rec("a", "keep", 10), rec("b", "likelySafe", 50)])).toBe(1);
    expect(actionableBytes([rec("a", "keep", 10), rec("b", "likelySafe", 50)])).toBe(50);
  });
});

describe("explorer sorting", () => {
  const rows = [
    { name: "beta", bytes: 10, category: "cache" as const },
    { name: "alpha", bytes: 90, category: "userData" as const },
    { name: "gamma", bytes: 40, category: "cache" as const },
  ];
  it("sorts by size desc and name asc", () => {
    expect(sortExplorerRows(rows, "size", "desc").map((r) => r.name)).toEqual(["alpha", "gamma", "beta"]);
    expect(sortExplorerRows(rows, "name", "asc").map((r) => r.name)).toEqual(["alpha", "beta", "gamma"]);
    expect(sortExplorerRows(rows, "size", "asc").map((r) => r.name)).toEqual(["beta", "gamma", "alpha"]);
  });
  it("entriesToRows maps directory entries to folder/file rows", () => {
    const entries: DirectoryEntry[] = [
      { name: "sub", path: "C:\\root\\sub", bytes: 60, isDir: true, category: "cache", classification: null },
      { name: "a.txt", path: "C:\\root\\a.txt", bytes: 40, isDir: false, category: "userData" },
    ];
    const out = entriesToRows(entries);
    expect(out.map((r) => r.kind)).toEqual(["folder", "file"]);
    expect(out.map((r) => r.path)).toEqual(["C:\\root\\sub", "C:\\root\\a.txt"]);
  });
  it("breadcrumbsBetween walks from the root down to the current folder", () => {
    const crumbs = breadcrumbsBetween("C:\\Users\\alex", "C:\\Users\\alex\\Projects\\app");
    expect(crumbs.map((c) => c.name)).toEqual(["alex", "Projects", "app"]);
    expect(crumbs.map((c) => c.path)).toEqual([
      "C:\\Users\\alex",
      "C:\\Users\\alex\\Projects",
      "C:\\Users\\alex\\Projects\\app",
    ]);
    expect(breadcrumbsBetween("C:\\Users\\alex", "C:\\Users\\alex")).toHaveLength(1);
  });
});

describe("filterByQuery (explorer name filter)", () => {
  const items = [
    { name: "node_modules", path: "C:\\root\\node_modules" },
    { name: "Notes.txt", path: "C:\\root\\Notes.txt" },
    { name: "photo.png", path: "C:\\root\\photo.png" },
  ];
  it("matches name substrings case-insensitively", () => {
    expect(filterByQuery(items, "NOTE").map((i) => i.name)).toEqual(["Notes.txt"]);
    expect(filterByQuery(items, "module").map((i) => i.name)).toEqual(["node_modules"]);
    expect(filterByQuery(items, "zzz")).toEqual([]);
  });
  it("returns everything for an empty or blank query", () => {
    expect(filterByQuery(items, "")).toHaveLength(3);
    expect(filterByQuery(items, "   ")).toHaveLength(3);
  });
});
