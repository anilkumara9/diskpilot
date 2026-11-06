import type { DuplicateGroup } from "./types";

/**
 * One-click keeper rules for the exact-duplicates view. Applying
 * a rule picks a keeper in EVERY group and selects every other
 * copy for cleanup — the review dialog still shows the exact
 * copies and re-verifies them before anything moves, so a rule
 * is a shortcut to a selection, never a deletion by itself.
 *
 * Keeper choice is fully deterministic: the rule's key decides,
 * and ties fall back to the lexicographically smallest path, so
 * the same groups always produce the same keepers.
 */
export type SmartSelectRule = "newest" | "oldest" | "shortestPath";

export interface SmartSelection {
  /** group hash → keeper path */
  keepers: Record<string, string>;
  /** every non-keeper copy across all groups */
  selected: Set<string>;
}

function comparePaths(a: string, b: string): number {
  return a < b ? -1 : a > b ? 1 : 0;
}

/** A copy with no recorded modification time counts as older
    than any timed copy for "newest", and as the natural keeper
    for "oldest" only when nothing timed is older — both follow
    from treating the missing time as the epoch. */
function modifiedMs(file: { modifiedAtMs?: number | null }): number {
  return file.modifiedAtMs ?? 0;
}

/** The keeper one rule picks for one group; null for an empty group. */
export function smartKeeper(group: DuplicateGroup, rule: SmartSelectRule): string | null {
  const files = group.files;
  if (files.length === 0) return null;
  const sorted = [...files].sort((a, b) => {
    const primary =
      rule === "newest"
        ? modifiedMs(b) - modifiedMs(a)
        : rule === "oldest"
          ? modifiedMs(a) - modifiedMs(b)
          : a.path.length - b.path.length;
    return primary !== 0 ? primary : comparePaths(a.path, b.path);
  });
  return sorted[0]?.path ?? null;
}

/** Apply a rule across all groups: keepers for every group and
    the selection of every other copy (hard-link copies select
    like any other; the inode-aware totals keep the reclaim
    figure honest). */
export function smartSelect(groups: DuplicateGroup[], rule: SmartSelectRule): SmartSelection {
  const keepers: Record<string, string> = {};
  const selected = new Set<string>();
  for (const group of groups) {
    const keeper = smartKeeper(group, rule);
    if (keeper === null) continue;
    keepers[group.hash] = keeper;
    for (const file of group.files) {
      if (file.path !== keeper) selected.add(file.path);
    }
  }
  return { keepers, selected };
}
