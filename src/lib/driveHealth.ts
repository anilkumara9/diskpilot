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
