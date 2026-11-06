import type { DriveInfo } from "./types";

/**
 * Drive health from free space alone — the one signal every drive
 * reports the same way. Thresholds are fractions of the drive's
 * total size: at a fifth free the drive is comfortable; below a
 * tenth, Windows updates and temp files start failing.
 */
export type DriveStatus = "healthy" | "gettingLow" | "lowSpace";

export function driveStatus(drive: DriveInfo): DriveStatus {
  if (drive.totalBytes <= 0) return "lowSpace";
  const freeFraction = drive.freeBytes / drive.totalBytes;
  if (freeFraction >= 0.2) return "healthy";
  if (freeFraction >= 0.1) return "gettingLow";
  return "lowSpace";
}

/**
 * The Health page's one-line verdict, from the worst drive present:
 * any low-space drive is a drive that "needs attention"; otherwise
 * any getting-low drive is named; otherwise all drives are healthy.
 * Null when there are no drives to judge.
 */
export function healthHeadline(drives: DriveInfo[]): string | null {
  if (drives.length === 0) return null;
  const low = drives.filter((d) => driveStatus(d) === "lowSpace").length;
  if (low > 0) {
    return low === 1 ? "One drive needs attention" : `${low} drives need attention`;
  }
  const gettingLow = drives.filter((d) => driveStatus(d) === "gettingLow").length;
  if (gettingLow > 0) {
    return gettingLow === 1 ? "One drive is getting low" : `${gettingLow} drives are getting low`;
  }
  return "All drives look healthy";
}
