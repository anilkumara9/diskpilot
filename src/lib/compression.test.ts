import { describe, expect, it } from "vitest";
import { untouchedLabel, wholeMonthsBetween } from "./compression";

const ms = (iso: string) => new Date(iso).getTime();

describe("wholeMonthsBetween", () => {
  it("counts full calendar months", () => {
    expect(wholeMonthsBetween(ms("2026-01-15T00:00:00"), ms("2026-04-15T00:00:00"))).toBe(3);
    expect(wholeMonthsBetween(ms("2025-11-06T00:00:00"), ms("2026-11-06T00:00:00"))).toBe(12);
  });

  it("does not count a month the day-of-month has not reached", () => {
    expect(wholeMonthsBetween(ms("2026-01-15T00:00:00"), ms("2026-02-14T00:00:00"))).toBe(0);
    expect(wholeMonthsBetween(ms("2026-01-31T00:00:00"), ms("2026-02-28T00:00:00"))).toBe(0);
  });

  it("is zero for reversed or invalid input", () => {
    expect(wholeMonthsBetween(ms("2026-04-15T00:00:00"), ms("2026-01-15T00:00:00"))).toBe(0);
    expect(wholeMonthsBetween(Number.NaN, ms("2026-01-15T00:00:00"))).toBe(0);
  });
});

describe("untouchedLabel", () => {
  it("speaks in months for cold folders", () => {
    expect(untouchedLabel(ms("2026-08-06T00:00:00"), ms("2026-11-06T00:00:00"))).toBe(
      "Untouched for 3 months",
    );
    expect(untouchedLabel(ms("2026-10-06T00:00:00"), ms("2026-11-06T00:00:00"))).toBe(
      "Untouched for 1 month",
    );
  });

  it("speaks in years once a folder is a year cold", () => {
    expect(untouchedLabel(ms("2025-11-06T00:00:00"), ms("2026-11-06T00:00:00"))).toBe(
      "Untouched for 1 year",
    );
    expect(untouchedLabel(ms("2024-09-06T00:00:00"), ms("2026-11-06T00:00:00"))).toBe(
      "Untouched for 2 years 2 months",
    );
  });

  it("falls back to weeks and days under a month", () => {
    expect(untouchedLabel(ms("2026-10-16T00:00:00"), ms("2026-11-06T00:00:00"))).toBe(
      "Untouched for 3 weeks",
    );
    expect(untouchedLabel(ms("2026-11-03T00:00:00"), ms("2026-11-06T00:00:00"))).toBe(
      "Untouched for 3 days",
    );
    expect(untouchedLabel(ms("2026-11-06T00:00:00"), ms("2026-11-06T12:00:00"))).toBe(
      "Touched within the last day",
    );
  });
});
