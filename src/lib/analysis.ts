import type {
  Category,
  FileEntry,
  LocationEntry,
  Recommendation,
  ScanSummary,
  Tier,
} from "./types";
import { CATEGORY_ORDER } from "./categories";
import { TIER_ORDER } from "./tiers";

/**
 * Nesting-safe totals: locations and heavy folders can contain each other
 * (for example a project folder inside a home folder). Summing them naively
 * double-counts. These helpers always prefer the authoritative category
 * totals from the scan summary and only sum leaf-level collections.
 */

export function categoryRows(
  summary: ScanSummary,
): { category: Category; bytes: number; recoverable: number }[] {
  return CATEGORY_ORDER.map((category) => ({
    category,
    bytes: summary.categoryTotals[category] ?? 0,
    recoverable: summary.recoverableByCategory[category] ?? 0,
  }))
    .filter((row) => row.bytes > 0)
    .sort((a, b) => b.bytes - a.bytes);
}

export function totalRecoverable(summary: ScanSummary): number {
  return Object.values(summary.recoverableByCategory).reduce((acc, v) => acc + (v ?? 0), 0);
}

/** Sum a flat list of entries whose paths are guaranteed disjoint. */
export function sumDisjoint(entries: { bytes: number }[]): number {
  return entries.reduce((acc, e) => acc + e.bytes, 0);
}

function isAncestorPath(ancestor: string, path: string): boolean {
  const a = ancestor.replace(/\//g, "\\").replace(/\\+$/, "").toLowerCase();
  const p = path.replace(/\//g, "\\").toLowerCase();
  return p === a || p.startsWith(a + "\\");
}

/**
 * Drop any entry that is contained inside another entry in the same list,
 * so nested locations never double-count toward a displayed total.
 */
export function dedupeNested<T extends { path: string }>(entries: T[]): T[] {
  return entries.filter(
    (entry) =>
      !entries.some((other) => other !== entry && isAncestorPath(other.path, entry.path)),
  );
}

export function sortLocations(locations: LocationEntry[]): LocationEntry[] {
  return [...locations].sort((a, b) => b.bytes - a.bytes);
}

export function sortFilesBySize(files: FileEntry[]): FileEntry[] {
  return [...files].sort((a, b) => b.bytes - a.bytes);
}

export type FileSortKey = "name" | "size" | "category";
export type SortDirection = "asc" | "desc";

export function sortExplorerRows<T extends { name: string; bytes: number; category: Category }>(
  rows: T[],
  key: FileSortKey,
  direction: SortDirection,
): T[] {
  const factor = direction === "asc" ? 1 : -1;
  return [...rows].sort((a, b) => {
    if (key === "size") return (a.bytes - b.bytes) * factor;
    if (key === "category") {
      const cmp = a.category.localeCompare(b.category);
      return (cmp !== 0 ? cmp : a.name.localeCompare(b.name)) * factor;
    }
    return a.name.localeCompare(b.name) * factor;
  });
}

export function groupRecommendationsByTier(
  recommendations: Recommendation[],
): { tier: Tier; items: Recommendation[] }[] {
  return TIER_ORDER.map((tier) => ({
    tier,
    items: recommendations
      .filter((r) => r.tier === tier)
      .sort((a, b) => b.bytes - a.bytes),
  })).filter((group) => group.items.length > 0);
}

export function actionableCount(recommendations: Recommendation[]): number {
  return recommendations.filter((r) => r.tier !== "keep").length;
}

export function actionableBytes(recommendations: Recommendation[]): number {
  return recommendations
    .filter((r) => r.tier !== "keep")
    .reduce((acc, r) => acc + r.bytes, 0);
}

export function filterByQuery<T extends { name: string; path: string }>(
  items: T[],
  query: string,
): T[] {
  const q = query.trim().toLowerCase();
  if (!q) return items;
  return items.filter(
    (item) => item.name.toLowerCase().includes(q) || item.path.toLowerCase().includes(q),
  );
}
