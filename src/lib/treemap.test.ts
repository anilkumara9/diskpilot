import { describe, expect, it } from "vitest";
import { partitionTreemap, squarifyTreemap, type TreemapRect } from "./treemap";

const RECT: TreemapRect = { x: 0, y: 0, width: 200, height: 100 };
const EPS = 1e-6;

function area(r: TreemapRect): number {
  return r.width * r.height;
}

describe("squarifyTreemap", () => {
  it("returns nothing for empty input, non-positive values, or a degenerate rect", () => {
    expect(squarifyTreemap([], RECT)).toEqual([]);
    expect(squarifyTreemap([{ id: "a", value: 0 }], RECT)).toEqual([]);
    expect(squarifyTreemap([{ id: "a", value: -5 }], RECT)).toEqual([]);
    expect(squarifyTreemap([{ id: "a", value: 5 }], { x: 0, y: 0, width: 0, height: 10 })).toEqual([]);
  });

  it("gives a single item the whole rect", () => {
    const nodes = squarifyTreemap([{ id: "only", value: 42 }], RECT);
    expect(nodes).toHaveLength(1);
    expect(nodes[0].rect).toEqual(RECT);
  });

  it("makes areas proportional to values", () => {
    const items = [
      { id: "a", value: 50 },
      { id: "b", value: 30 },
      { id: "c", value: 15 },
      { id: "d", value: 5 },
    ];
    const totalArea = area(RECT);
    const nodes = squarifyTreemap(items, RECT);
    expect(nodes).toHaveLength(4);
    for (const item of items) {
      const node = nodes.find((n) => n.id === item.id);
      expect(node).toBeDefined();
      const expected = (item.value / 100) * totalArea;
      expect(Math.abs(area(node!.rect) - expected) / expected).toBeLessThan(1e-9);
    }
    const covered = nodes.reduce((acc, n) => acc + area(n.rect), 0);
    expect(Math.abs(covered - totalArea) / totalArea).toBeLessThan(1e-9);
  });

  it("never overlaps and stays inside the rect", () => {
    const items = [
      { id: "a", value: 910 },
      { id: "b", value: 400 },
      { id: "c", value: 377 },
      { id: "d", value: 210 },
      { id: "e", value: 96 },
      { id: "f", value: 55 },
      { id: "g", value: 31 },
      { id: "h", value: 12 },
      { id: "i", value: 4 },
      { id: "j", value: 1 },
    ];
    const rect: TreemapRect = { x: 10, y: 20, width: 173, height: 91 };
    const nodes = squarifyTreemap(items, rect);
    expect(nodes).toHaveLength(items.length);
    for (const node of nodes) {
      expect(node.rect.x).toBeGreaterThanOrEqual(rect.x - EPS);
      expect(node.rect.y).toBeGreaterThanOrEqual(rect.y - EPS);
      expect(node.rect.x + node.rect.width).toBeLessThanOrEqual(rect.x + rect.width + EPS);
      expect(node.rect.y + node.rect.height).toBeLessThanOrEqual(rect.y + rect.height + EPS);
      expect(node.rect.width).toBeGreaterThan(0);
      expect(node.rect.height).toBeGreaterThan(0);
    }
    for (let i = 0; i < nodes.length; i++) {
      for (let j = i + 1; j < nodes.length; j++) {
        const a = nodes[i].rect;
        const b = nodes[j].rect;
        const overlapX = Math.max(0, Math.min(a.x + a.width, b.x + b.width) - Math.max(a.x, b.x));
        const overlapY = Math.max(0, Math.min(a.y + a.height, b.y + b.height) - Math.max(a.y, b.y));
        expect(overlapX * overlapY).toBeLessThanOrEqual(EPS);
      }
    }
  });

  it("keeps blocks reasonably square for equal values", () => {
    const items = Array.from({ length: 4 }, (_, i) => ({ id: `q${i}`, value: 25 }));
    const nodes = squarifyTreemap(items, { x: 0, y: 0, width: 100, height: 100 });
    for (const node of nodes) {
      const ratio = Math.max(node.rect.width / node.rect.height, node.rect.height / node.rect.width);
      expect(ratio).toBeLessThanOrEqual(2.01);
    }
  });
});

describe("partitionTreemap", () => {
  const entry = (bytes: number) => ({ bytes });

  it("shows everything when the level is small", () => {
    const items = [entry(300), entry(200), entry(100)];
    const { shown, rest, restBytes } = partitionTreemap(items);
    expect(shown.map((i) => i.bytes)).toEqual([300, 200, 100]);
    expect(rest).toEqual([]);
    expect(restBytes).toBe(0);
  });

  it("stops at the block cap and folds the tail", () => {
    const items = Array.from({ length: 200 }, () => entry(10));
    const { shown, rest, restBytes } = partitionTreemap(items);
    expect(shown).toHaveLength(140);
    expect(rest).toHaveLength(60);
    expect(restBytes).toBe(600);
  });

  it("stops once the shown blocks cover 99.5% of the level", () => {
    // One giant entry plus a long tail of crumbs: the giant alone
    // covers 99.9%, so the crumbs fold even though the cap is far away.
    const items = [entry(999_000), ...Array.from({ length: 50 }, () => entry(20))];
    const { shown, rest } = partitionTreemap(items);
    expect(shown).toHaveLength(1);
    expect(rest).toHaveLength(50);
  });

  it("always shows at least the largest entry, and drops non-positive entries", () => {
    const items = [entry(0), entry(-4), entry(7)];
    const { shown, rest } = partitionTreemap(items);
    expect(shown.map((i) => i.bytes)).toEqual([7]);
    expect(rest).toEqual([]);
  });

  it("honours a custom cap and coverage", () => {
    const items = Array.from({ length: 10 }, (_, i) => entry(100 - i * 10));
    const { shown, rest } = partitionTreemap(items, 3, 0.995);
    expect(shown).toHaveLength(3);
    expect(rest).toHaveLength(7);
  });
});
