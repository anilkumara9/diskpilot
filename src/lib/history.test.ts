import { describe, expect, it } from "vitest";
import { historyDetail, operationTitle } from "./history";

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
