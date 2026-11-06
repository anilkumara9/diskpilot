import { describe, expect, it } from "vitest";
import { liveChipState } from "./liveIndex";

describe("liveChipState", () => {
  it("shows nothing without a summary", () => {
    expect(liveChipState(null)).toBeNull();
    expect(liveChipState(undefined)).toBeNull();
  });

  it("shows nothing when neither flag is set", () => {
    expect(liveChipState({})).toBeNull();
    expect(liveChipState({ liveIndexActive: false, indexStale: false })).toBeNull();
  });

  it("is live when the index is actively followed and not stale", () => {
    expect(liveChipState({ liveIndexActive: true })).toBe("live");
    expect(liveChipState({ liveIndexActive: true, indexStale: false })).toBe("live");
  });

  it("is stale whenever the shell marks the index stale", () => {
    expect(liveChipState({ indexStale: true })).toBe("stale");
    expect(liveChipState({ liveIndexActive: false, indexStale: true })).toBe("stale");
  });

  it("stale wins over live when both flags are set", () => {
    expect(liveChipState({ liveIndexActive: true, indexStale: true })).toBe("stale");
  });
});
