import { describe, expect, it } from "vitest";
import { historyDetail, operationTitle, undoneRelocationIds } from "./history";
import type { HistoryEntry } from "./types";

describe("operationTitle", () => {
  it("maps the shell's recorded operations to human titles", () => {
    expect(operationTitle("cleanup")).toBe("Cleanup");
    expect(operationTitle("restore")).toBe("Restore");
    expect(operationTitle("duplicates")).toBe("Duplicate removal");
    expect(operationTitle("empty-recycle-bin")).toBe("Recycle Bin emptied");
  });

  it("humanizes unknown operations instead of showing them raw", () => {
    expect(operationTitle("free-up-space")).toBe("Free up space");
    expect(operationTitle("")).toBe("Action");
  });
});

describe("historyDetail", () => {
  it("returns null for empty details", () => {
    expect(historyDetail(null)).toBeNull();
    expect(historyDetail(undefined)).toBeNull();
    expect(historyDetail("   ")).toBeNull();
  });

  it("keeps prose details as text", () => {
    expect(historyDetail("Restored 3 items from a cleanup")).toEqual({
      kind: "text",
      text: "Restored 3 items from a cleanup",
    });
  });

  it("demotes a plan id to a reference caption, never a path", () => {
    const detail = historyDetail("plan-3f8a1c2b9d4e5f60a1b2");
    expect(detail).toEqual({ kind: "reference", label: "Plan 3f8a1c2b9d4e…" });
  });

  it("demotes a bare group hash to a reference caption", () => {
    const detail = historyDetail("9f86d081884c7d659a2feaa0c55ad015");
    expect(detail?.kind).toBe("reference");
    expect(detail && detail.kind === "reference" ? detail.label : "").toContain("Group 9f86d081884c");
  });

  it("shortens other single-token references", () => {
    const detail = historyDetail("recycle-bin");
    expect(detail).toEqual({ kind: "reference", label: "Ref recycle-bin" });
  });
});

describe("undoneRelocationIds", () => {
  const entry = (partial: Partial<HistoryEntry>): HistoryEntry => ({
    id: 1,
    timestampMs: 1_000,
    operation: "cleanup",
    paths: [],
    reclaimedBytes: 0,
    errors: [],
    ...partial,
  });
  const relocate = (id: number, timestampMs: number, from: string, to: string) =>
    entry({ id, timestampMs, operation: "relocate", paths: [from, to] });
  const undo = (id: number, timestampMs: number, wasAt: string, backTo: string) =>
    entry({ id, timestampMs, operation: "relocate-undo", paths: [wasAt, backTo] });

  it("marks a relocate whose reversed pair a later undo recorded", () => {
    const entries = [
      relocate(1, 1_000, "C:\\Games\\Big", "D:\\Big"),
      undo(2, 2_000, "D:\\Big", "C:\\Games\\Big"),
    ];
    expect(undoneRelocationIds(entries)).toEqual(new Set([1]));
  });

  it("matches paths with Windows semantics (case and slashes)", () => {
    const entries = [
      relocate(1, 1_000, "C:\\Games\\Big", "D:\\Big"),
      undo(2, 2_000, "d:/big", "c:\\games\\big"),
    ];
    expect(undoneRelocationIds(entries)).toEqual(new Set([1]));
  });

  it("an undo recorded BEFORE the move does not mark it", () => {
    const entries = [
      undo(1, 500, "D:\\Big", "C:\\Games\\Big"),
      relocate(2, 1_000, "C:\\Games\\Big", "D:\\Big"),
    ];
    expect(undoneRelocationIds(entries).size).toBe(0);
  });

  it("a different pair does not mark it, nor do other operations", () => {
    const entries = [
      relocate(1, 1_000, "C:\\Games\\Big", "D:\\Big"),
      undo(2, 2_000, "D:\\Other", "C:\\Games\\Other"),
      entry({ id: 3, timestampMs: 3_000, operation: "relocate-undo", paths: [] }),
      relocate(4, 4_000, "C:\\Games\\Big", "E:\\Big"),
    ];
    expect(undoneRelocationIds(entries).size).toBe(0);
  });

  it("only the undone move of several is marked", () => {
    const entries = [
      relocate(1, 1_000, "C:\\A", "D:\\A"),
      relocate(2, 1_500, "C:\\B", "D:\\B"),
      undo(3, 2_000, "D:\\B", "C:\\B"),
    ];
    expect(undoneRelocationIds(entries)).toEqual(new Set([2]));
  });
});
