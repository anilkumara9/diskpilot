import { describe, expect, it } from "vitest";
import { INTENT_DEDUPE_WINDOW_MS, shouldConsumeIntent } from "./launchIntent";

describe("shouldConsumeIntent", () => {
  const last = { path: "D:\\Games", startedAt: 10_000 };

  it("consumes when nothing has been delivered yet", () => {
    expect(shouldConsumeIntent(null, "D:\\Games", 10_000, false)).toBe(true);
    expect(shouldConsumeIntent(null, "D:\\Games", 10_000, true)).toBe(true);
  });

  it("consumes a different path even inside the window", () => {
    expect(shouldConsumeIntent(last, "E:\\Media", 10_500, true)).toBe(true);
  });

  it("ignores the same path while its scan just started and is still running", () => {
    expect(shouldConsumeIntent(last, "D:\\Games", 10_001, true)).toBe(false);
    expect(
      shouldConsumeIntent(last, "D:\\Games", 10_000 + INTENT_DEDUPE_WINDOW_MS - 1, true),
    ).toBe(false);
  });

  it("consumes the same path again once its scan has finished", () => {
    expect(shouldConsumeIntent(last, "D:\\Games", 10_500, false)).toBe(true);
  });

  it("consumes the same path again after the window, even if a scan is running", () => {
    expect(
      shouldConsumeIntent(last, "D:\\Games", 10_000 + INTENT_DEDUPE_WINDOW_MS, true),
    ).toBe(true);
    expect(shouldConsumeIntent(last, "D:\\Games", 99_000, true)).toBe(true);
  });
});
