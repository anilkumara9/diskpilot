import { describe, expect, it } from "vitest";
import { driveStatus, healthHeadline } from "./health";
import type { DriveInfo } from "./types";

function drive(freeBytes: number, totalBytes = 1_000_000_000): DriveInfo {
  return { rootPath: "C:\\", totalBytes, freeBytes };
}

describe("driveStatus", () => {
  it("is healthy at a fifth free and above", () => {
    expect(driveStatus(drive(250_000_000))).toBe("healthy");
    expect(driveStatus(drive(200_000_000))).toBe("healthy");
    expect(driveStatus(drive(1_000_000_000))).toBe("healthy");
  });

  it("is getting low between a tenth and a fifth free", () => {
    expect(driveStatus(drive(199_999_999))).toBe("gettingLow");
    expect(driveStatus(drive(150_000_000))).toBe("gettingLow");
    expect(driveStatus(drive(100_000_000))).toBe("gettingLow");
  });

  it("is low space below a tenth free", () => {
    expect(driveStatus(drive(99_999_999))).toBe("lowSpace");
    expect(driveStatus(drive(0))).toBe("lowSpace");
  });

  it("treats a drive with no measurable size as low space", () => {
    expect(driveStatus({ rootPath: "D:\\", totalBytes: 0, freeBytes: 0 })).toBe("lowSpace");
  });
});

describe("healthHeadline", () => {
  it("is null when there are no drives", () => {
    expect(healthHeadline([])).toBeNull();
  });

  it("reports all healthy when every drive has room", () => {
    expect(healthHeadline([drive(500_000_000), drive(300_000_000)])).toBe("All drives look healthy");
  });

  it("names a single getting-low drive", () => {
    expect(healthHeadline([drive(500_000_000), drive(150_000_000)])).toBe("One drive is getting low");
  });

  it("pluralizes several getting-low drives", () => {
    expect(healthHeadline([drive(150_000_000), drive(120_000_000)])).toBe("2 drives are getting low");
  });

  it("prioritizes low space over getting low, with counts", () => {
    expect(healthHeadline([drive(150_000_000), drive(50_000_000)])).toBe("One drive needs attention");
    expect(healthHeadline([drive(50_000_000), drive(20_000_000), drive(150_000_000)])).toBe(
      "2 drives need attention",
    );
  });
});
