import { describe, expect, it } from "vitest";
import { cleanupBanner } from "./cleanup";
import type { CleanupResult } from "./types";

function result(partial: Partial<CleanupResult>): CleanupResult {
  return {
    planId: "plan-1",
    succeeded: [],
    failed: [],
    reclaimedBytes: 0,
    finishedAtMs: 1700000000000,
    ...partial,
  };
}

describe("cleanupBanner", () => {
  it("all succeeded reads as the classic success banner", () => {
    const banner = cleanupBanner(
      result({ succeeded: ["a", "b"], reclaimedBytes: 2048 }),
    );
    expect(banner.kind).toBe("success");
    expect(banner.message).toContain("Cleanup complete.");
    expect(banner.message).toContain("see History for the full record");
  });

  it("partial success is a warning naming the count and first reason", () => {
    const banner = cleanupBanner(
      result({
        succeeded: ["a"],
        failed: [
          { path: "b", message: "The file is in use." },
          { path: "c", message: "Access denied." },
        ],
        reclaimedBytes: 1024,
      }),
    );
    expect(banner.kind).toBe("warning");
    expect(banner.message).toContain("2 items couldn't be removed.");
    expect(banner.message).toContain("The file is in use.");
    expect(banner.message).not.toContain("Cleanup complete");
  });

  it("a single partial failure uses the singular", () => {
    const banner = cleanupBanner(
      result({
        succeeded: ["a"],
        failed: [{ path: "b", message: "Gone already." }],
        reclaimedBytes: 512,
      }),
    );
    expect(banner.kind).toBe("warning");
    expect(banner.message).toContain("1 item couldn't be removed.");
  });

  it("all failed is an error that says nothing was removed", () => {
    const banner = cleanupBanner(
      result({
        succeeded: [],
        failed: [{ path: "a", message: "The plan has expired." }],
      }),
    );
    expect(banner.kind).toBe("error");
    expect(banner.message).toContain("Nothing was removed.");
    expect(banner.message).toContain("The plan has expired.");
    expect(banner.message).not.toContain("Cleanup complete");
  });
});
