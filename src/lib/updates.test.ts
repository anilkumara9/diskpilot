import { describe, expect, it } from "vitest";
import {
  outstandingUpdateCount,
  PENDING,
  readSoftwareUpdatesBadge,
  SOFTWARE_UPDATES_BADGE_KEY,
  writeSoftwareUpdatesBadge,
  type UpdateRowState,
} from "./updates";
import type { AvailableUpdate } from "./types";

function update(id: string): AvailableUpdate {
  return { name: id, id, currentVersion: "1.0", availableVersion: "2.0" };
}

function memoryStorage() {
  const map = new Map<string, string>();
  return {
    getItem: (k: string) => map.get(k) ?? null,
    setItem: (k: string, v: string) => void map.set(k, v),
    removeItem: (k: string) => void map.delete(k),
  };
}

describe("outstandingUpdateCount", () => {
  const updates = [update("a"), update("b"), update("c")];

  it("counts every listed update before any run", () => {
    expect(outstandingUpdateCount(updates, {})).toBe(3);
  });

  it("drops rows that updated, keeps pending and failed", () => {
    const states: Record<string, UpdateRowState> = {
      a: { phase: "updated", message: "done" },
      b: { phase: "failed", message: "nope" },
      c: PENDING,
    };
    expect(outstandingUpdateCount(updates, states)).toBe(2);
  });

  it("is zero when every row updated", () => {
    const states: Record<string, UpdateRowState> = {
      a: { phase: "updated", message: "" },
      b: { phase: "updated", message: "" },
      c: { phase: "updated", message: "" },
    };
    expect(outstandingUpdateCount(updates, states)).toBe(0);
  });
});

describe("software updates badge persistence", () => {
  it("round-trips a badge through storage", () => {
    const storage = memoryStorage();
    writeSoftwareUpdatesBadge({ count: 4, checkedAtMs: 1_700_000_000_000 }, storage);
    expect(readSoftwareUpdatesBadge(storage)).toEqual({ count: 4, checkedAtMs: 1_700_000_000_000 });
    expect(storage.getItem(SOFTWARE_UPDATES_BADGE_KEY)).toContain("\"count\":4");
  });

  it("clears the badge with null", () => {
    const storage = memoryStorage();
    writeSoftwareUpdatesBadge({ count: 2, checkedAtMs: 5 }, storage);
    writeSoftwareUpdatesBadge(null, storage);
    expect(readSoftwareUpdatesBadge(storage)).toBeNull();
  });

  it("reads malformed values as absent", () => {
    const storage = memoryStorage();
    storage.setItem(SOFTWARE_UPDATES_BADGE_KEY, "not json");
    expect(readSoftwareUpdatesBadge(storage)).toBeNull();
    storage.setItem(SOFTWARE_UPDATES_BADGE_KEY, JSON.stringify({ count: "four" }));
    expect(readSoftwareUpdatesBadge(storage)).toBeNull();
    storage.setItem(SOFTWARE_UPDATES_BADGE_KEY, JSON.stringify({ count: -1, checkedAtMs: 5 }));
    expect(readSoftwareUpdatesBadge(storage)).toBeNull();
  });

  it("tolerates missing storage", () => {
    expect(readSoftwareUpdatesBadge(null)).toBeNull();
    expect(() => writeSoftwareUpdatesBadge({ count: 1, checkedAtMs: 1 }, null)).not.toThrow();
  });
});
