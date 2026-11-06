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
