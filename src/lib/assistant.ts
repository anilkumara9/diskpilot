import type { Recommendation } from "./types";

/** Resolve the recommendation ids an assistant reply highlighted against
    the recommendations loaded in the store. Ids are returned in the
    order the assistant gave them; ids that no longer resolve (stale
    scan, already cleaned) are skipped rather than rendered blank. */
export function resolveHighlights(
  ids: readonly string[] | undefined,
  recommendations: readonly Recommendation[],
): Recommendation[] {
  if (!ids || ids.length === 0) return [];
  const byId = new Map(recommendations.map((r) => [r.id, r]));
  const resolved: Recommendation[] = [];
  for (const id of ids) {
    const rec = byId.get(id);
    if (rec) resolved.push(rec);
  }
  return resolved;
}
