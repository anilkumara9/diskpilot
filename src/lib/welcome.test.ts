import { describe, expect, it } from "vitest";
import { shouldShowWelcome } from "./welcome";

describe("shouldShowWelcome", () => {
  it("shows for a first-run user: not onboarded, never scanned", () => {
    expect(shouldShowWelcome({ onboarded: false }, false)).toBe(true);
  });

  it("hides once setup was completed or skipped", () => {
    expect(shouldShowWelcome({ onboarded: true }, false)).toBe(false);
  });

  it("hides when a scan already exists, even if never onboarded", () => {
    expect(shouldShowWelcome({ onboarded: false }, true)).toBe(false);
  });

  it("hides for an onboarded user with a scan", () => {
    expect(shouldShowWelcome({ onboarded: true }, true)).toBe(false);
  });
});
