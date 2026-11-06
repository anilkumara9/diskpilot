import { describe, expect, it } from "vitest";
import {
  isUpdateNoticeable,
  updateProgressPercent,
  updateStatusLine,
  type UpdateState,
} from "./updater";

describe("updateProgressPercent", () => {
  it("is null when the total is unknown", () => {
    expect(updateProgressPercent(512, null)).toBeNull();
    expect(updateProgressPercent(512, 0)).toBeNull();
  });

  it("computes a floored percent", () => {
    expect(updateProgressPercent(0, 1000)).toBe(0);
    expect(updateProgressPercent(250, 1000)).toBe(25);
    expect(updateProgressPercent(999, 1000)).toBe(99);
    expect(updateProgressPercent(1000, 1000)).toBe(100);
  });

  it("clamps over- and under-counts", () => {
    expect(updateProgressPercent(1500, 1000)).toBe(100);
  });
});

describe("isUpdateNoticeable", () => {
  it("flags only a waiting or installed update", () => {
    const quiet: UpdateState[] = [
      { status: "idle" },
      { status: "checking" },
      { status: "upToDate" },
      { status: "downloading", version: "0.7.0", downloaded: 1, total: 2 },
      { status: "error" },
    ];
    for (const state of quiet) expect(isUpdateNoticeable(state)).toBe(false);
    expect(isUpdateNoticeable({ status: "available", version: "0.7.0", notes: null })).toBe(true);
    expect(isUpdateNoticeable({ status: "readyToRestart", version: "0.7.0" })).toBe(true);
  });
});

describe("updateStatusLine", () => {
  it("names the current version when up to date", () => {
    expect(updateStatusLine({ status: "upToDate" }, "0.6.0")).toBe(
      "DiskPilot 0.6.0 is the latest version.",
    );
    expect(updateStatusLine({ status: "upToDate" }, null)).toBe(
      "DiskPilot is the latest version.",
    );
  });

  it("reports progress with and without a known total", () => {
    expect(
      updateStatusLine({ status: "downloading", version: "0.7.0", downloaded: 1, total: 4 }, "0.6.0"),
    ).toBe("Downloading version 0.7.0… 25%");
    expect(
      updateStatusLine({ status: "downloading", version: "0.7.0", downloaded: 1, total: null }, "0.6.0"),
    ).toBe("Downloading version 0.7.0…");
  });

  it("keeps the failure line honest and soft", () => {
    expect(updateStatusLine({ status: "error" }, "0.6.0")).toContain("Couldn't check for updates");
  });
});
