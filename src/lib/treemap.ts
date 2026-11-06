/* Squarified treemap layout (Bruls, Huijsen & van Wijk). Pure: items
   with values go in, rectangles come out, areas proportional to
   values. Kept free of DOM/React imports so it can be unit-tested
   directly. */

export interface TreemapItem {
  id: string;
  value: number;
}

export interface TreemapRect {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface TreemapNode {
  id: string;
  value: number;
  rect: TreemapRect;
}

interface ScaledItem {
  id: string;
  value: number;
  area: number;
}

/** Worst (largest) aspect ratio in a row of areas laid along `side`. */
function worstRatio(areas: number[], side: number): number {
  const sum = areas.reduce((a, b) => a + b, 0);
  if (sum <= 0 || side <= 0) return Number.POSITIVE_INFINITY;
  const max = Math.max(...areas);
  const min = Math.min(...areas);
  if (min <= 0) return Number.POSITIVE_INFINITY;
  return Math.max(
    (side * side * max) / (sum * sum),
    (sum * sum) / (side * side * min),
  );
}

function layoutInto(items: ScaledItem[], rect: TreemapRect, out: TreemapNode[]): void {
  if (items.length === 0) return;
  if (rect.width <= 0 || rect.height <= 0) {
    for (const item of items) {
      out.push({ id: item.id, value: item.value, rect: { x: rect.x, y: rect.y, width: 0, height: 0 } });
    }
    return;
  }
  if (items.length === 1) {
    out.push({ id: items[0].id, value: items[0].value, rect: { ...rect } });
    return;
  }

  /* Grow the current row while adding the next item keeps the worst
     aspect ratio from getting worse; then fix the row as one strip. */
  const side = Math.min(rect.width, rect.height);
  const row: ScaledItem[] = [items[0]];
  let rest = items.slice(1);
  while (rest.length > 0) {
    const current = worstRatio(row.map((r) => r.area), side);
    const candidate = worstRatio([...row.map((r) => r.area), rest[0].area], side);
    if (candidate > current) break;
    row.push(rest[0]);
    rest = rest.slice(1);
  }

  const rowArea = row.reduce((a, r) => a + r.area, 0);
  let remaining: TreemapRect;
  if (rect.width >= rect.height) {
    /* Vertical strip down the left, spanning the full (shorter)
       height, items stacked. The row runs along the short side —
       that is what keeps the blocks square. */
    const stripWidth = rowArea / rect.height;
    let y = rect.y;
    for (const item of row) {
      const h = stripWidth > 0 ? item.area / stripWidth : 0;
      out.push({
        id: item.id,
        value: item.value,
        rect: { x: rect.x, y, width: stripWidth, height: h },
      });
      y += h;
    }
    remaining = {
      x: rect.x + stripWidth,
      y: rect.y,
      width: rect.width - stripWidth,
      height: rect.height,
    };
  } else {
    /* Horizontal strip across the top, spanning the full (shorter)
       width, items side by side. */
    const stripHeight = rowArea / rect.width;
    let x = rect.x;
    for (const item of row) {
      const w = stripHeight > 0 ? item.area / stripHeight : 0;
      out.push({
        id: item.id,
        value: item.value,
        rect: { x, y: rect.y, width: w, height: stripHeight },
      });
      x += w;
    }
    remaining = {
      x: rect.x,
      y: rect.y + stripHeight,
      width: rect.width,
      height: rect.height - stripHeight,
    };
  }
  layoutInto(rest, remaining, out);
}

export interface TreemapPartition<T> {
  /** Items worth their own block, largest first. */
  shown: T[];
  /** The long tail, largest first — rendered as one aggregate. */
  rest: T[];
  /** Sum of the tail's bytes (the aggregate block's value). */
  restBytes: number;
}

/**
 * Decide which level entries earn their own block on the map.
 * Largest first: keep showing entries until either `maxBlocks`
 * blocks are shown or the shown entries cover `coverage` of the
 * level's total bytes — whichever comes first. Everything after
 * that folds into the aggregate. Non-positive entries never show
 * (they have no area) and do not count toward the total.
 */
export function partitionTreemap<T extends { bytes: number }>(
  items: T[],
  maxBlocks = 140,
  coverage = 0.995,
): TreemapPartition<T> {
  const sorted = items
    .filter((i) => Number.isFinite(i.bytes) && i.bytes > 0)
    .sort((a, b) => b.bytes - a.bytes);
  const total = sorted.reduce((acc, i) => acc + i.bytes, 0);
  const shown: T[] = [];
  let covered = 0;
  let index = 0;
  while (index < sorted.length) {
    if (shown.length >= maxBlocks) break;
    if (shown.length > 0 && total > 0 && covered / total >= coverage) break;
    shown.push(sorted[index]);
    covered += sorted[index].bytes;
    index += 1;
  }
  const rest = sorted.slice(index);
  return { shown, rest, restBytes: rest.reduce((acc, i) => acc + i.bytes, 0) };
}

/**
 * Lay `items` out inside `rect`, largest first. Non-positive or
 * non-finite values are dropped (a zero-byte item has no area to
 * show). The returned nodes cover the rect exactly, in layout order.
 */
export function squarifyTreemap(items: TreemapItem[], rect: TreemapRect): TreemapNode[] {
  const usable = items.filter((i) => Number.isFinite(i.value) && i.value > 0);
  if (usable.length === 0 || rect.width <= 0 || rect.height <= 0) return [];
  const totalValue = usable.reduce((a, i) => a + i.value, 0);
  const totalArea = rect.width * rect.height;
  const scaled: ScaledItem[] = [...usable]
    .sort((a, b) => b.value - a.value)
    .map((i) => ({ id: i.id, value: i.value, area: (i.value / totalValue) * totalArea }));
  const out: TreemapNode[] = [];
  layoutInto(scaled, rect, out);
  return out;
}
