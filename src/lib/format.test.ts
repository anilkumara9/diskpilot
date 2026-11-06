import { describe, expect, it } from "vitest";
import {
  basename,
  displayPath,
  formatBytes,
  formatCount,
  formatDuration,
  formatPercent,
  formatRelative,
  formatSignedBytes,
  formatSignedCount,
  formatTimeOfDay,
  parentPath,
} from "./format";

describe("formatBytes (decimal units)", () => {
  it("formats bytes, KB, MB, GB, TB", () => {
    expect(formatBytes(0)).toBe("0 B");
    expect(formatBytes(999)).toBe("999 B");
    expect(formatBytes(1_000)).toBe("1 KB");
    expect(formatBytes(1_500_000)).toBe("1.5 MB");
    expect(formatBytes(2_500_000_000)).toBe("2.5 GB");
    expect(formatBytes(1_000_000_000_000)).toBe("1 TB");
  });
  it("handles invalid input", () => {
    expect(formatBytes(-5)).toBe("0 B");
    expect(formatBytes(Number.NaN)).toBe("0 B");
  });
});

describe("formatCount / formatPercent / formatDuration", () => {
  it("groups counts en-US", () => {
    expect(formatCount(1234567)).toBe("1,234,567");
  });
  it("formats percents", () => {
    expect(formatPercent(0.5)).toBe("50%");
    expect(formatPercent(0.123)).toBe("12%");
    expect(formatPercent(0.0005)).toBe("<0.1%");
  });
  it("formats durations", () => {
    expect(formatDuration(5_000)).toBe("5s");
    expect(formatDuration(95_000)).toBe("1m 35s");
    expect(formatDuration(3_900_000)).toBe("1h 5m");
  });
});

describe("formatRelative", () => {
  const now = 1_700_000_000_000;
  it("covers recent ranges", () => {
    expect(formatRelative(now - 3_000, now)).toBe("just now");
    expect(formatRelative(now - 45_000, now)).toBe("45 seconds ago");
    expect(formatRelative(now - 60_000, now)).toBe("1 minute ago");
    expect(formatRelative(now - 3_600_000, now)).toBe("1 hour ago");
    expect(formatRelative(now - 86_400_000, now)).toBe("1 day ago");
  });
});

describe("Windows path helpers", () => {
  const home = "C:\\Users\\alex";
  it("replaces the home prefix with ~ handling backslashes", () => {
    expect(displayPath("C:\\Users\\alex", home)).toBe("~");
    expect(displayPath("C:\\Users\\alex\\Downloads\\file.zip", home)).toBe("~\\Downloads\\file.zip");
    expect(displayPath("C:/Users/alex/AppData/Local", home)).toBe("~\\AppData\\Local");
    expect(displayPath("D:\\Data\\x.bin", home)).toBe("D:\\Data\\x.bin");
    // A sibling folder that merely shares a prefix must not be rewritten.
    expect(displayPath("C:\\Users\\alexandra\\file.txt", home)).toBe("C:\\Users\\alexandra\\file.txt");
  });
  it("basename handles both separators", () => {
    expect(basename("C:\\Users\\alex\\file.txt")).toBe("file.txt");
    expect(basename("C:/Users/alex/file.txt")).toBe("file.txt");
    expect(basename("C:\\Users\\alex\\")).toBe("alex");
  });
  it("parentPath handles drive roots", () => {
    expect(parentPath("C:\\Users\\alex")).toBe("C:\\Users");
    expect(parentPath("C:\\Users")).toBe("C:\\");
  });
});

describe("signed deltas (scan-over-scan comparison)", () => {
  it("formats byte deltas with an explicit sign", () => {
    expect(formatSignedBytes(212_000_000)).toBe("+212 MB");
    expect(formatSignedBytes(-1_200_000_000)).toBe("−1.2 GB");
    expect(formatSignedBytes(-500)).toBe("−500 B");
    expect(formatSignedBytes(0)).toBe("0 B");
    expect(formatSignedBytes(Number.NaN)).toBe("0 B");
  });
  it("formats count deltas with an explicit sign", () => {
    expect(formatSignedCount(1234)).toBe("+1,234");
    expect(formatSignedCount(-56)).toBe("−56");
    expect(formatSignedCount(0)).toBe("0");
  });
});

describe("formatTimeOfDay", () => {
  it("formats local time as HH:MM in 24-hour form", () => {
    expect(formatTimeOfDay(new Date(2026, 0, 5, 9, 5).getTime())).toBe("09:05");
    expect(formatTimeOfDay(new Date(2026, 0, 5, 23, 59).getTime())).toBe("23:59");
    expect(formatTimeOfDay(new Date(2026, 0, 5, 0, 0).getTime())).toBe("00:00");
  });
});
