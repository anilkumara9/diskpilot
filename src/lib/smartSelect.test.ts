import { describe, expect, it } from "vitest";
import { smartKeeper, smartSelect } from "./smartSelect";
import type { DuplicateFile, DuplicateGroup } from "./types";

function file(path: string, modifiedAtMs?: number | null): DuplicateFile {
  return { path, bytes: 100, isClone: false, isHardlink: false, modifiedAtMs };
}

function group(hash: string, files: DuplicateFile[]): DuplicateGroup {
  return { size: 100, hash, files };
}

describe("smartKeeper", () => {
  it("newest keeps the most recently modified copy", () => {
    const g = group("h", [file("C:\\a.bin", 100), file("C:\\b.bin", 300), file("C:\\c.bin", 200)]);
    expect(smartKeeper(g, "newest")).toBe("C:\\b.bin");
  });

  it("oldest keeps the least recently modified copy", () => {
    const g = group("h", [file("C:\\a.bin", 100), file("C:\\b.bin", 300), file("C:\\c.bin", 200)]);
    expect(smartKeeper(g, "oldest")).toBe("C:\\a.bin");
  });

  it("shortestPath keeps the shortest path", () => {
    const g = group("h", [
      file("C:\\Users\\me\\Documents\\deep\\a.bin", 100),
      file("C:\\a.bin", 50),
      file("C:\\Users\\me\\a.bin", 999),
    ]);
    expect(smartKeeper(g, "shortestPath")).toBe("C:\\a.bin");
  });

  it("breaks equal modification times by the smallest path", () => {
    const g = group("h", [file("C:\\z.bin", 100), file("C:\\a.bin", 100), file("C:\\m.bin", 100)]);
    expect(smartKeeper(g, "newest")).toBe("C:\\a.bin");
    expect(smartKeeper(g, "oldest")).toBe("C:\\a.bin");
  });

  it("breaks equal path lengths by the smallest path", () => {
    const g = group("h", [file("C:\\zz.bin", 1), file("C:\\aa.bin", 2)]);
    expect(smartKeeper(g, "shortestPath")).toBe("C:\\aa.bin");
  });

  it("treats a missing modification time as the epoch", () => {
    const untimed = group("h", [file("C:\\a.bin", null), file("C:\\b.bin", 50)]);
    expect(smartKeeper(untimed, "newest")).toBe("C:\\b.bin");
    expect(smartKeeper(untimed, "oldest")).toBe("C:\\a.bin");
  });

  it("returns null for an empty group", () => {
    expect(smartKeeper(group("h", []), "newest")).toBeNull();
  });
});

describe("smartSelect", () => {
  it("selects every non-keeper in every group", () => {
    const groups = [
      group("h1", [file("C:\\a.bin", 100), file("C:\\b.bin", 300)]),
      group("h2", [file("D:\\x.bin", 10), file("D:\\y.bin", 20), file("D:\\z.bin", 30)]),
    ];
    const result = smartSelect(groups, "newest");
    expect(result.keepers).toEqual({ h1: "C:\\b.bin", h2: "D:\\z.bin" });
    expect([...result.selected].sort()).toEqual(["C:\\a.bin", "D:\\x.bin", "D:\\y.bin"]);
  });

  it("keeps a single-copy group untouched", () => {
    const result = smartSelect([group("h", [file("C:\\only.bin", 1)])], "oldest");
    expect(result.keepers).toEqual({ h: "C:\\only.bin" });
    expect(result.selected.size).toBe(0);
  });

  it("selects hard-link copies like any other copy", () => {
    const g = group("h", [
      { path: "C:\\a.bin", bytes: 100, isClone: false, isHardlink: false, modifiedAtMs: 300 },
      { path: "C:\\b.bin", bytes: 100, isClone: false, isHardlink: true, modifiedAtMs: 100 },
    ]);
    const result = smartSelect([g], "newest");
    expect(result.keepers.h).toBe("C:\\a.bin");
    expect([...result.selected]).toEqual(["C:\\b.bin"]);
  });

  it("handles no groups", () => {
    const result = smartSelect([], "shortestPath");
    expect(result.keepers).toEqual({});
    expect(result.selected.size).toBe(0);
  });
});
