/* Settings helpers that are pure enough to unit-test: the scan
   exclusions are edited as one pattern per line in Settings and
   stored as a clean list. */

/**
 * Parse the exclusions textarea into the stored list: one pattern per
 * line, trimmed, blank lines dropped, duplicates removed keeping the
 * first occurrence (order is the user's, and the engine matches in
 * order). Comparison for duplicates is exact — patterns are
 * case-sensitive text the engine interprets.
 */
export function parseExclusions(text: string): string[] {
  const seen = new Set<string>();
  const patterns: string[] = [];
  for (const line of text.split(/\r?\n/)) {
    const pattern = line.trim();
    if (!pattern || seen.has(pattern)) continue;
    seen.add(pattern);
    patterns.push(pattern);
  }
  return patterns;
}

/** Render the stored exclusions list back into textarea text. */
export function exclusionsToText(patterns: string[]): string {
  return patterns.join("\n");
}

/** True when two exclusion lists hold the same patterns in order. */
export function sameExclusions(a: string[], b: string[]): boolean {
  return a.length === b.length && a.every((pattern, i) => pattern === b[i]);
}

/**
 * One-time migration for the collapsed-sidebar preference. Early
 * builds mirrored `sidebarCollapsed` to localStorage because the
 * shell's settings struct predated the field and dropped it on
 * save; the shell now round-trips the field natively, so settings
 * are the single source of truth. At startup a surviving mirror
 * value is applied through the normal settings update path (which
 * persists it to settings.json) and the key is then removed —
 * see `init` in the store.
 */
export const SIDEBAR_COLLAPSED_MIRROR_KEY = "diskpilot.sidebarCollapsed.v1";

interface MirrorStorage {
  getItem(key: string): string | null;
  removeItem(key: string): void;
}

function defaultMirrorStorage(): MirrorStorage | null {
  try {
    return window.localStorage;
  } catch {
    return null;
  }
}

/** Read the legacy mirror: true/false when present, null when
    there is nothing to migrate. */
export function readSidebarCollapsedMirror(
  storage: MirrorStorage | null = defaultMirrorStorage(),
): boolean | null {
  if (!storage) return null;
  try {
    const value = storage.getItem(SIDEBAR_COLLAPSED_MIRROR_KEY);
    return value === null ? null : value === "1";
  } catch {
    return null;
  }
}

/** Remove the legacy mirror key once it has been migrated. */
export function removeSidebarCollapsedMirror(
  storage: MirrorStorage | null = defaultMirrorStorage(),
): void {
  if (!storage) return;
  try {
    storage.removeItem(SIDEBAR_COLLAPSED_MIRROR_KEY);
  } catch {
    /* a surviving key is harmless: migration simply retries */
  }
}
