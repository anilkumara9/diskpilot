import { describe, expect, it } from "vitest";
import { appJunkBanner } from "./junk";
import { destinationDrives, driveRootOf, relocationTarget, sameLocation } from "./moves";
import { actionableIds, anyRunning, markRunning, mergeOutcomes, PENDING } from "./updates";
import { clampMinAgeDays } from "./maintenance";
import { driveHealthFacts, driveHealthStatusLabel, lifeBarColorVar } from "./driveHealth";
import type { DriveHealth, DriveInfo } from "./types";

describe("appJunkBanner", () => {
  it("full success names the bytes and the Recycle Bin", () => {
    const banner = appJunkBanner({ succeeded: ["a"], failed: [], reclaimedBytes: 2048 });
    expect(banner.kind).toBe("success");
    expect(banner.message).toContain("moved to the Recycle Bin");
    expect(banner.message).toContain("History");
  });

  it("total failure is an error that says nothing was cleaned", () => {
    const banner = appJunkBanner({
      succeeded: [],
      failed: [{ path: "a", error: "The folder is in use." }],
      reclaimedBytes: 0,
    });
    expect(banner.kind).toBe("error");
    expect(banner.message).toContain("Nothing was cleaned.");
    expect(banner.message).toContain("The folder is in use.");
  });

  it("partial success is a warning counting the failed locations", () => {
    const banner = appJunkBanner({
      succeeded: ["a"],
      failed: [
        { path: "b", error: "Locked." },
        { path: "c", error: "Locked." },
      ],
      reclaimedBytes: 1024,
    });
    expect(banner.kind).toBe("warning");
    expect(banner.message).toContain("2 locations couldn't be cleaned.");
  });
});

describe("moves", () => {
  const drives: DriveInfo[] = [
    { rootPath: "C:\\", totalBytes: 100, freeBytes: 10 },
    { rootPath: "D:\\", totalBytes: 200, freeBytes: 90 },
    { rootPath: "E:\\", totalBytes: 300, freeBytes: 290 },
  ];

  it("driveRootOf reads the drive letter either slash style", () => {
    expect(driveRootOf("C:\\Users\\me")).toBe("C:\\");
    expect(driveRootOf("d:/Games")).toBe("D:\\");
    expect(driveRootOf("relative\\path")).toBeNull();
  });

  it("sameLocation ignores case, slashes and trailing separators", () => {
    expect(sameLocation("c:/", "C:\\")).toBe(true);
    expect(sameLocation("D:\\Data\\", "d:\\data")).toBe(true);
    expect(sameLocation("C:\\", "D:\\")).toBe(false);
  });

  it("destinationDrives excludes the source's own drive", () => {
    expect(destinationDrives(drives, "C:\\Games\\Big").map((d) => d.rootPath)).toEqual([
      "D:\\",
      "E:\\",
    ]);
    expect(destinationDrives(drives, "e:\\lib").map((d) => d.rootPath)).toEqual([
      "C:\\",
      "D:\\",
    ]);
  });

  it("destinationDrives offers everything when the source drive is unknown", () => {
    expect(destinationDrives(drives, "folder")).toHaveLength(3);
  });

  it("relocationTarget appends the folder name to the destination", () => {
    expect(relocationTarget("C:\\Games\\Big Game", "D:\\")).toBe("D:\\Big Game");
    expect(relocationTarget("C:\\Games\\Big Game", "D:\\Moved")).toBe("D:\\Moved\\Big Game");
    expect(relocationTarget("C:\\Games\\Big Game", "D:/")).toBe("D:\\Big Game");
  });
});

describe("updates row states", () => {
  it("markRunning then mergeOutcomes settles updated and failed rows", () => {
    let states = markRunning({}, ["a", "b"]);
    expect(anyRunning(states)).toBe(true);
    states = mergeOutcomes(
      states,
      [
        { id: "a", success: true, message: "Done" },
        { id: "b", success: false, message: "winget exited 1" },
      ],
      ["a", "b"],
    );
    expect(states.a).toEqual({ phase: "updated", message: "Done" });
    expect(states.b).toEqual({ phase: "failed", message: "winget exited 1" });
    expect(anyRunning(states)).toBe(false);
  });

  it("an attempted row with no outcome returns to pending, never stuck running", () => {
    const states = mergeOutcomes(markRunning({}, ["a"]), [], ["a"]);
    expect(states.a).toEqual(PENDING);
  });

  it("outcomes for unattempted ids are ignored", () => {
    const states = mergeOutcomes({}, [{ id: "zzz", success: true, message: "" }], ["a"]);
    expect(states.zzz).toBeUndefined();
  });

  it("actionableIds covers pending and failed rows only", () => {
    const states = {
      a: PENDING,
      b: { phase: "failed", message: "x" } as const,
      c: { phase: "updated", message: "y" } as const,
    };
    expect(actionableIds(states, ["a", "b", "c", "d"])).toEqual(["a", "b", "d"]);
  });
});

describe("clampMinAgeDays", () => {
  it("clamps to 7..365 and rounds", () => {
    expect(clampMinAgeDays(3)).toBe(7);
    expect(clampMinAgeDays(400)).toBe(365);
    expect(clampMinAgeDays(30.6)).toBe(31);
    expect(clampMinAgeDays(90)).toBe(90);
  });

  it("non-numeric input falls back to the default", () => {
    expect(clampMinAgeDays(Number.NaN)).toBe(30);
  });
});

describe("driveHealth", () => {
  const base: DriveHealth = {
    model: "Samsung SSD 990 PRO",
    interface: "NVMe",
    capacityBytes: 1_000_000,
    temperatureC: 42,
    lifeLeftPercent: 93,
    powerOnHours: 1234,
    powerCycles: 567,
    reallocatedSectors: 0,
    predictedFailure: false,
    status: "healthy",
  };

  it("facts render measured values, with 0 shown as 0", () => {
    const facts = driveHealthFacts(base);
    expect(facts).toEqual([
      { label: "Temperature", value: "42 °C" },
      { label: "Power-on hours", value: "1,234" },
      { label: "Power cycles", value: "567" },
      { label: "Reallocated sectors", value: "0" },
    ]);
  });

  it("facts say Not reported for values the drive declined to give", () => {
    const facts = driveHealthFacts({ ...base, temperatureC: null, powerOnHours: null });
    expect(facts[0].value).toBe("Not reported");
    expect(facts[1].value).toBe("Not reported");
    expect(facts[2].value).toBe("567");
  });

  it("status labels are human words", () => {
    expect(driveHealthStatusLabel("healthy")).toBe("Healthy");
    expect(driveHealthStatusLabel("caution")).toBe("Caution");
    expect(driveHealthStatusLabel("critical")).toBe("Critical");
    expect(driveHealthStatusLabel("unknown")).toBe("Unknown");
  });

  it("life bar colour follows the status thresholds", () => {
    expect(lifeBarColorVar(93)).toBe("var(--accent)");
    expect(lifeBarColorVar(20)).toBe("var(--tier-review)");
    expect(lifeBarColorVar(10)).toBe("var(--critical)");
  });
});
