import { formatBytes } from "./format";
import type { CleanupResult } from "./types";

/**
 * The banner a finished cleanup earns, derived from the FULL
 * result — never from the reclaimed byte count alone. A run where
 * some (or all) items failed must not read as "Cleanup complete":
 * partial success is a warning naming the first failure, and a run
 * that removed nothing is an error that says so plainly. The full
 * per-item record always lives in History, and every wording
 * points there.
 */
export interface CleanupBanner {
  kind: "success" | "warning" | "error" | "info";
  message: string;
}

export function cleanupBanner(result: CleanupResult): CleanupBanner {
  const failedCount = result.failed.length;
  if (failedCount === 0) {
    return {
      kind: "success",
      message: `Cleanup complete. ${formatBytes(result.reclaimedBytes)} reclaimed — see History for the full record.`,
    };
  }
  const reason = result.failed[0]?.message;
  if (result.succeeded.length === 0) {
    return {
      kind: "error",
      message: `Nothing was removed.${reason ? ` ${reason}` : ""} See History for the full record.`,
    };
  }
  return {
    kind: "warning",
    message:
      `${failedCount} item${failedCount === 1 ? "" : "s"} couldn't be removed.` +
      `${reason ? ` ${reason}` : ""} ${formatBytes(result.reclaimedBytes)} reclaimed — see History for the full record.`,
  };
}
