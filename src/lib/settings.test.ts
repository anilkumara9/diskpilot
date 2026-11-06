import { describe, expect, it } from "vitest";
import {
  exclusionsToText,
  parseExclusions,
  readSidebarCollapsedMirror,
  removeSidebarCollapsedMirror,
  sameExclusions,
  SIDEBAR_COLLAPSED_MIRROR_KEY,
} from "./settings";

function memoryStorage(initial: Record<string, string> = {}) {
  const map = new Map(Object.entries(initial));
  return {
    getItem: (k: string) => map.get(k) ?? null,
    removeItem: (k: string) => void map.delete(k),
    has: (k: string) => map.has(k),
  };
}

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

describe("sidebarCollapsed mirror migration", () => {
  it("returns null when no mirror exists — nothing to migrate", () => {
    expect(readSidebarCollapsedMirror(memoryStorage())).toBeNull();
  });

  it("reads the mirrored preference in both directions", () => {
    expect(
      readSidebarCollapsedMirror(
        memoryStorage({ [SIDEBAR_COLLAPSED_MIRROR_KEY]: "1" }),
      ),
    ).toBe(true);
    expect(
      readSidebarCollapsedMirror(
        memoryStorage({ [SIDEBAR_COLLAPSED_MIRROR_KEY]: "0" }),
      ),
    ).toBe(false);
  });

  it("treats any non-\"1\" value as expanded, matching the old reader", () => {
    expect(
      readSidebarCollapsedMirror(
        memoryStorage({ [SIDEBAR_COLLAPSED_MIRROR_KEY]: "yes" }),
      ),
    ).toBe(false);
  });

  it("removes the key so the migration happens exactly once", () => {
    const storage = memoryStorage({ [SIDEBAR_COLLAPSED_MIRROR_KEY]: "1" });
    expect(readSidebarCollapsedMirror(storage)).toBe(true);
    removeSidebarCollapsedMirror(storage);
    expect(storage.has(SIDEBAR_COLLAPSED_MIRROR_KEY)).toBe(false);
    expect(readSidebarCollapsedMirror(storage)).toBeNull();
  });

  it("survives unavailable storage without throwing", () => {
    expect(readSidebarCollapsedMirror(null)).toBeNull();
    expect(() => removeSidebarCollapsedMirror(null)).not.toThrow();
  });
});
