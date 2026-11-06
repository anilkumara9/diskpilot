import { describe, expect, it } from "vitest";
import { healthDotSummary, worstDriveHealth } from "./driveHealth";
import type { DriveHealth } from "./types";

function drive(partial: Partial<DriveHealth>): DriveHealth {
  return {
    model: "Test SSD 1TB",
    interface: "NVMe",
    capacityBytes: 1_000_000_000_000,
    temperatureC: 40,
    lifeLeftPercent: 90,
    powerOnHours: 100,
    powerCycles: 10,
    reallocatedSectors: 0,
    predictedFailure: false,
    status: "healthy",
    ...partial,
  };
}

describe("worstDriveHealth", () => {
  it("returns null for no drives", () => {
    expect(worstDriveHealth([])).toBeNull();
  });

  it("returns null when every drive is unknown", () => {
    expect(worstDriveHealth([drive({ status: "unknown" }), drive({ status: "unknown" })])).toBeNull();
  });

  it("picks the critical drive over healthy and caution", () => {
    const critical = drive({ model: "C", status: "critical" });
    expect(
      worstDriveHealth([drive({ status: "healthy" }), drive({ status: "caution" }), critical]),
    ).toBe(critical);
  });

  it("picks caution over healthy and unknown", () => {
    const caution = drive({ model: "B", status: "caution" });
    expect(worstDriveHealth([drive({ status: "unknown" }), caution, drive({})])).toBe(caution);
  });

  it("keeps the first drive on a tie", () => {
    const first = drive({ model: "First", status: "caution" });
    const second = drive({ model: "Second", status: "caution" });
    expect(worstDriveHealth([first, second])).toBe(first);
  });

  it("returns the only drive when it is healthy", () => {
    const only = drive({});
    expect(worstDriveHealth([only])).toBe(only);
  });
});

describe("healthDotSummary", () => {
  it("composes model, life and status", () => {
    expect(healthDotSummary(drive({ model: "WD Blue SN570 500GB", lifeLeftPercent: 6, status: "critical" }))).toBe(
      "WD Blue SN570 500GB · SSD life 6% · Critical",
    );
  });

  it("omits the life part when the drive did not report it", () => {
    expect(healthDotSummary(drive({ model: "Old HDD", lifeLeftPercent: null, status: "caution" }))).toBe(
      "Old HDD · Caution",
    );
  });
});
