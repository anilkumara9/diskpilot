import type { CleanupBanner } from "./cleanup";
import { formatBytes } from "./format";
import type { CleanAppJunkResult } from "./types";

export type { CleanupBanner };

/**
 * The banner a finished app-cache clean earns — the same 3-state
 * derivation as `cleanupBanner` (full success / partial /
 * nothing), in junk wording: what was cleaned went to the Recycle
 * Bin, so the message says "moved", never "reclaimed", and the
 * full per-path record is in History, as everywhere.
 */
export function appJunkBanner(result: CleanAppJunkResult): CleanupBanner {
  const failedCount = result.failed.length;
  /* Nothing attempted at all — the caches vanished between
     measuring and cleaning (emptied by the app, or already
     cleaned). That is neither a success nor a failure: a
     neutral note, not a green check. */
  if (result.succeeded.length === 0 && failedCount === 0) {
    return {
      kind: "info",
      message: "Nothing was cleaned — those caches were already empty or gone.",
    };
  }
  if (failedCount === 0) {
    return {
      kind: "success",
      message: `App caches cleaned. ${formatBytes(result.reclaimedBytes)} moved to the Recycle Bin — see History for the full record.`,
    };
  }
  const reason = result.failed[0]?.error;
  if (result.succeeded.length === 0) {
    return {
      kind: "error",
      message: `Nothing was cleaned.${reason ? ` ${reason}` : ""} See History for the full record.`,
    };
  }
  return {
    kind: "warning",
    message:
      `${failedCount} location${failedCount === 1 ? "" : "s"} couldn't be cleaned.` +
      `${reason ? ` ${reason}` : ""} ${formatBytes(result.reclaimedBytes)} moved to the Recycle Bin — see History for the full record.`,
  };
}
