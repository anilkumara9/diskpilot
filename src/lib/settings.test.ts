import { describe, expect, it } from "vitest";
import { exclusionsToText, parseExclusions, sameExclusions } from "./settings";

describe("parseExclusions", () => {
  it("splits one pattern per line and trims whitespace", () => {
    expect(parseExclusions("*.iso\n  build-*  \nMyBackups")).toEqual([
      "*.iso",
      "build-*",
      "MyBackups",
    ]);
  });

  it("drops blank lines and CRLF endings", () => {
    expect(parseExclusions("\n*.tmp\r\n\r\ncache\n")).toEqual(["*.tmp", "cache"]);
  });

  it("dedupes preserving first-occurrence order", () => {
    expect(parseExclusions("b\na\nb\nc\na")).toEqual(["b", "a", "c"]);
  });

  it("returns an empty list for empty or whitespace-only text", () => {
    expect(parseExclusions("")).toEqual([]);
    expect(parseExclusions("  \n \n")).toEqual([]);
  });
});

describe("exclusionsToText / sameExclusions", () => {
  it("round-trips through parse", () => {
    const patterns = ["*.iso", "build-*", "MyBackups"];
    expect(parseExclusions(exclusionsToText(patterns))).toEqual(patterns);
    expect(exclusionsToText([])).toBe("");
  });

  it("compares lists by content and order", () => {
    expect(sameExclusions(["a", "b"], ["a", "b"])).toBe(true);
    expect(sameExclusions(["a", "b"], ["b", "a"])).toBe(false);
    expect(sameExclusions(["a"], ["a", "b"])).toBe(false);
    expect(sameExclusions([], [])).toBe(true);
  });
});
