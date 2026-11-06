import { describe, expect, it } from "vitest";
import { buildSeries, formatAxisBytes, niceTicks, smoothPath } from "./chart";

/** Every "x,y" pair in a path string, as [x, y] numbers. */
function pathCoords(path: string): Array<[number, number]> {
  const numbers = (path.match(/-?\d+(?:\.\d+)?/g) ?? []).map(Number);
  const coords: Array<[number, number]> = [];
  for (let i = 0; i + 1 < numbers.length; i += 2) {
    coords.push([numbers[i], numbers[i + 1]]);
  }
  return coords;
}

describe("smoothPath", () => {
  it("has no line for fewer than two points", () => {
    expect(smoothPath([])).toBe("");
    expect(smoothPath([{ x: 10, y: 10 }])).toBe("");
  });

  it("draws two points as a straight line, never NaN", () => {
    const path = smoothPath([
      { x: 0, y: 20 },
      { x: 100, y: 60 },
    ]);
    expect(path).toBe("M 0,20 L 100,60");
    expect(path).not.toContain("NaN");
  });

  it("keeps a flat series flat", () => {
    const path = smoothPath([
      { x: 0, y: 42 },
      { x: 40, y: 42 },
      { x: 80, y: 42 },
      { x: 120, y: 42 },
    ]);
    expect(path).toContain("C");
    for (const [, y] of pathCoords(path)) {
      expect(y).toBe(42);
    }
  });

  it("curves through three or more points", () => {
    const path = smoothPath([
      { x: 0, y: 90 },
      { x: 50, y: 10 },
      { x: 100, y: 90 },
    ]);
    expect(path.startsWith("M 0,90")).toBe(true);
    expect(path).toContain("C");
    expect(path).toContain("100,90");
  });

  it("clamps control points into the plot box", () => {
    /* A spike between calm neighbours would overshoot both ends of
       the box without clamping. */
    const path = smoothPath(
      [
        { x: 0, y: 50 },
        { x: 50, y: 50 },
        { x: 100, y: 0 },
        { x: 150, y: 50 },
        { x: 200, y: 50 },
      ],
      { top: 0, bottom: 100 },
    );
    for (const [, y] of pathCoords(path)) {
      expect(y).toBeGreaterThanOrEqual(0);
      expect(y).toBeLessThanOrEqual(100);
    }
  });
});

describe("niceTicks", () => {
  it("uses 1/2/5 × 10^n steps and brackets the range", () => {
    const ticks = niceTicks(0, 100, 4);
    expect(ticks[0]).toBeLessThanOrEqual(0);
    expect(ticks[ticks.length - 1]).toBeGreaterThanOrEqual(100);
    const step = ticks[1] - ticks[0];
    const magnitude = 10 ** Math.floor(Math.log10(step));
    expect([1, 2, 5]).toContain(step / magnitude);
    for (let i = 1; i < ticks.length; i += 1) {
      expect(ticks[i] - ticks[i - 1]).toBeCloseTo(step, 9);
    }
  });

  it("produces presentable ticks for byte-scale ranges", () => {
    const ticks = niceTicks(2.91e9, 14.4e9, 4);
    expect(ticks.length).toBeGreaterThanOrEqual(3);
    expect(ticks.length).toBeLessThanOrEqual(7);
    expect(ticks[0]).toBeLessThanOrEqual(2.91e9);
    expect(ticks[ticks.length - 1]).toBeGreaterThanOrEqual(14.4e9);
  });

  it("returns the single value for a flat range", () => {
    expect(niceTicks(7, 7)).toEqual([7]);
    expect(niceTicks(0, 0)).toEqual([0]);
  });

  it("returns nothing for non-finite input", () => {
    expect(niceTicks(NaN, 10)).toEqual([]);
    expect(niceTicks(0, Infinity)).toEqual([]);
  });
});

describe("buildSeries", () => {
  it("maps entries to points, sorted by x, dropping null y", () => {
    const entries = [
      { at: 300, size: 3 },
      { at: 100, size: null },
      { at: 200, size: 2 },
    ];
    expect(buildSeries(entries, (e) => ({ x: e.at, y: e.size }))).toEqual([
      { x: 200, y: 2 },
      { x: 300, y: 3 },
    ]);
  });

  it("is empty for no entries", () => {
    expect(buildSeries([], (e: number) => ({ x: e, y: e }))).toEqual([]);
  });
});

describe("formatAxisBytes", () => {
  it("formats like the app's byte formatter", () => {
    expect(formatAxisBytes(0)).toBe("0 B");
    expect(formatAxisBytes(1500)).toBe("1.5 KB");
    expect(formatAxisBytes(14.4e9)).toBe("14.4 GB");
  });
});
