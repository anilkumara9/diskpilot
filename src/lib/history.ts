/**
 * Presentation rules for History entries. The shell records terse
 * machine operations ("empty-recycle-bin") and stores raw plan ids
 * and group hashes in the detail field; the page shows human titles
 * and demotes raw ids to quiet reference captions — an id is not a
 * path and must never be styled like one.
 */

const OPERATION_TITLES: Record<string, string> = {
  cleanup: "Cleanup",
  restore: "Restore",
  duplicates: "Duplicate removal",
  "empty-recycle-bin": "Recycle Bin emptied",
  "docker-action": "Docker action",
  scan: "Scan",
};

/** Human title for a recorded operation. Unknown operations are
    humanized (dashes to spaces, first letter capitalized) rather
    than shown raw. */
export function operationTitle(operation: string): string {
  const known = OPERATION_TITLES[operation];
  if (known) return known;
  const words = operation.replace(/[-_]+/g, " ").trim();
  if (!words) return "Action";
  return words.charAt(0).toUpperCase() + words.slice(1);
}

export type HistoryDetail =
  | { kind: "text"; text: string }
  | { kind: "reference"; label: string };

/**
 * How an entry's detail renders: prose (it contains spaces — a
 * sentence the shell wrote for humans) stays text; a single token
 * is a machine reference (a plan id, a duplicate-group hash) and
 * becomes a demoted caption naming what it references, shortened
 * so a full id never dominates the entry.
 */
export function historyDetail(detail: string | null | undefined): HistoryDetail | null {
  const trimmed = (detail ?? "").trim();
  if (!trimmed) return null;
  if (/\s/.test(trimmed)) return { kind: "text", text: trimmed };
  if (trimmed.startsWith("plan-")) {
    return { kind: "reference", label: `Plan ${shorten(trimmed.slice(5))}` };
  }
  if (/^[0-9a-f]{16,}$/i.test(trimmed)) {
    return { kind: "reference", label: `Group ${shorten(trimmed)}` };
  }
  return { kind: "reference", label: `Ref ${shorten(trimmed)}` };
}

function shorten(id: string): string {
  return id.length > 12 ? `${id.slice(0, 12)}…` : id;
}
