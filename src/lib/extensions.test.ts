import { describe, expect, it } from "vitest";
import { extensionLabel, topTypesCoverage } from "./extensions";

describe("extensionLabel", () => {
  it("dots a bare extension", () => {
    expect(extensionLabel("zip")).toBe(".zip");
    expect(extensionLabel("tar.gz")).toBe(".tar.gz");
  });

  it("keeps an already-dotted extension as is", () => {
    expect(extensionLabel(".zip")).toBe(".zip");
  });

  it("spells out the empty extension", () => {
    expect(extensionLabel("")).toBe("(no extension)");
  });
});

describe("topTypesCoverage", () => {
  it("sums the largest types against the scanned total", () => {
    const stats = [{ bytes: 300 }, { bytes: 500 }, { bytes: 200 }];
    expect(topTypesCoverage(stats, 2000)).toBeCloseTo(0.5);
    expect(topTypesCoverage(stats, 2000, 2)).toBeCloseTo(0.4);
  });

  it("is zero when there is nothing to cover", () => {
    expect(topTypesCoverage([], 2000)).toBe(0);
    expect(topTypesCoverage([{ bytes: 10 }], 0)).toBe(0);
  });

  it("never exceeds the whole", () => {
    expect(topTypesCoverage([{ bytes: 3000 }], 2000)).toBe(1);
  });
});
