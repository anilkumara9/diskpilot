import type { AvailableUpdate, SoftwareUpdateOutcome } from "./types";

/**
 * Per-row state for the Applications updates list, derived from
 * the outcomes `update_software` returns. Kept pure so the mapping
 * is testable: a row is pending until an update runs for it, then
 * it wears its outcome — "updated" with the shell's message kept
 * for reference, or "failed" with the reason shown under the row.
 */
export type UpdateRowState =
  | { phase: "pending" }
  | { phase: "running" }
  | { phase: "updated"; message: string }
  | { phase: "failed"; message: string };

export const PENDING: UpdateRowState = { phase: "pending" };
export const RUNNING: UpdateRowState = { phase: "running" };

/** Mark rows as running (an update was just started for them). */
export function markRunning(
  states: Record<string, UpdateRowState>,
  ids: string[],
): Record<string, UpdateRowState> {
  const next = { ...states };
  for (const id of ids) next[id] = RUNNING;
  return next;
}

/**
 * Fold one update run's outcomes into the row states. Outcomes
 * for ids that were not attempted are ignored — the list only
 * ever shows states for its own rows.
 */
export function mergeOutcomes(
  states: Record<string, UpdateRowState>,
  outcomes: SoftwareUpdateOutcome[],
  attemptedIds: string[],
): Record<string, UpdateRowState> {
  const attempted = new Set(attemptedIds);
  const next = { ...states };
  for (const outcome of outcomes) {
    if (!attempted.has(outcome.id)) continue;
    next[outcome.id] =
      outcome.success
        ? { phase: "updated", message: outcome.message }
        : { phase: "failed", message: outcome.message };
  }
  /* An attempted row with no outcome came back unreported: return
     it to pending rather than leaving it "running" forever. */
  for (const id of attempted) {
    if (next[id]?.phase === "running") next[id] = PENDING;
  }
  return next;
}

/** True while any row's update is in flight (the whole section's
    actions disable — winget runs one queue). */
export function anyRunning(states: Record<string, UpdateRowState>): boolean {
  return Object.values(states).some((s) => s.phase === "running");
}

/** Ids whose rows can still be updated (pending or failed). */
export function actionableIds(
  states: Record<string, UpdateRowState>,
  allIds: string[],
): string[] {
  return allIds.filter((id) => {
    const phase = states[id]?.phase ?? "pending";
    return phase === "pending" || phase === "failed";
  });
}

/**
 * How many of the listed updates are still outstanding given the
 * row states: rows never attempted (pending) and rows whose last
 * attempt failed. Rows that updated successfully no longer count.
 */
export function outstandingUpdateCount(
  updates: AvailableUpdate[],
  states: Record<string, UpdateRowState>,
): number {
  return actionableIds(
    states,
    updates.map((u) => u.id),
  ).length;
}

/**
 * The Applications nav badge's memory: the result of the last
 * COMPLETED update check (count + when), persisted in
 * localStorage so it survives restarts. It is a memory of a check
 * the user ran — nothing here ever triggers a check by itself;
 * winget only runs when the user asks on the Applications page.
 */
export interface SoftwareUpdatesBadge {
  count: number;
  checkedAtMs: number;
}

export const SOFTWARE_UPDATES_BADGE_KEY = "diskpilot.softwareUpdatesBadge.v1";

interface BadgeStorage {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
  removeItem(key: string): void;
}

function defaultStorage(): BadgeStorage | null {
  try {
    return window.localStorage;
  } catch {
    return null;
  }
}

/** Read the persisted badge; anything malformed reads as absent. */
export function readSoftwareUpdatesBadge(
  storage: BadgeStorage | null = defaultStorage(),
): SoftwareUpdatesBadge | null {
  if (!storage) return null;
  try {
    const raw = storage.getItem(SOFTWARE_UPDATES_BADGE_KEY);
    if (!raw) return null;
    const parsed = JSON.parse(raw) as Partial<SoftwareUpdatesBadge>;
    if (
      typeof parsed.count !== "number" ||
      !Number.isFinite(parsed.count) ||
      parsed.count < 0 ||
      typeof parsed.checkedAtMs !== "number" ||
      !Number.isFinite(parsed.checkedAtMs)
    ) {
      return null;
    }
    return { count: Math.floor(parsed.count), checkedAtMs: parsed.checkedAtMs };
  } catch {
    return null;
  }
}

/** Persist the badge (or clear it with null). Storage failures
    are swallowed — the badge is ambient, never worth an error. */
export function writeSoftwareUpdatesBadge(
  badge: SoftwareUpdatesBadge | null,
  storage: BadgeStorage | null = defaultStorage(),
): void {
  if (!storage) return;
  try {
    if (badge === null) storage.removeItem(SOFTWARE_UPDATES_BADGE_KEY);
    else storage.setItem(SOFTWARE_UPDATES_BADGE_KEY, JSON.stringify(badge));
  } catch {
    /* ambient state only */
  }
}
