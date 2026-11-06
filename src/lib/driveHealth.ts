import { formatCount } from "./format";
import type { DriveHealth, DriveHealthStatus } from "./types";

/**
 * Presentation derivations for a drive's SMART report, kept pure
 * so they are testable: the fact rows (with an honest "Not
 * reported" for every value the drive declined to give — zero is
 * a real measurement and renders as "0"), the status label, and
 * the colour of the life-left bar.
 */

export interface DriveFact {
  label: string;
  value: string;
}

function fact(value: number | null, render: (n: number) => string): string {
  return value === null ? "Not reported" : render(value);
}

export function driveHealthFacts(health: DriveHealth): DriveFact[] {
  return [
    { label: "Temperature", value: fact(health.temperatureC, (n) => `${n} °C`) },
    { label: "Power-on hours", value: fact(health.powerOnHours, (n) => formatCount(n)) },
    { label: "Power cycles", value: fact(health.powerCycles, (n) => formatCount(n)) },
    { label: "Reallocated sectors", value: fact(health.reallocatedSectors, (n) => formatCount(n)) },
  ];
}

export function driveHealthStatusLabel(status: DriveHealthStatus): string {
  switch (status) {
    case "healthy":
      return "Healthy";
    case "caution":
      return "Caution";
    case "critical":
      return "Critical";
    case "unknown":
      return "Unknown";
  }
}

/** The life-left bar follows the product's status colours: accent
    while life is comfortable, the review tier at 20% or below,
    critical at 10% or below. */
export function lifeBarColorVar(lifeLeftPercent: number): string {
  if (lifeLeftPercent <= 10) return "var(--critical)";
  if (lifeLeftPercent <= 20) return "var(--tier-review)";
  return "var(--accent)";
}

const STATUS_RANK: Record<DriveHealthStatus, number> = {
  unknown: 0,
  healthy: 1,
  caution: 2,
  critical: 3,
};

/**
 * The drive the sidebar health dot speaks for: the worst status
 * across all drives (critical beats caution beats healthy; ties
 * keep the first drive). Returns null when there is nothing to
 * say — no drives, or every drive Unknown — so the dot simply
 * does not render rather than guessing at a state.
 */
export function worstDriveHealth(drives: DriveHealth[]): DriveHealth | null {
  let worst: DriveHealth | null = null;
  for (const drive of drives) {
    if (worst === null || STATUS_RANK[drive.status] > STATUS_RANK[worst.status]) {
      worst = drive;
    }
  }
  return worst !== null && worst.status !== "unknown" ? worst : null;
}

/**
 * The dot's one-line summary: "<model> · SSD life 87% · Healthy".
 * Parts a drive did not report are omitted, never invented.
 */
export function healthDotSummary(drive: DriveHealth): string {
  const parts = [drive.model];
  if (drive.lifeLeftPercent !== null) parts.push(`SSD life ${drive.lifeLeftPercent}%`);
  parts.push(driveHealthStatusLabel(drive.status));
  return parts.join(" · ");
}
