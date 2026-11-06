/* Chart geometry helpers — the pure half of the Chart component.
   Everything here works in plain numbers: callers scale data points
   into plot pixels, then hand the pixel points to smoothPath. */

import { formatBytes } from "./format";

export interface ChartPoint {
  x: number;
  y: number;
}

/** The vertical span of the plot box, in the same units as the
    points passed to smoothPath. Control points are clamped into it
    so a smoothed curve never overshoots the data's own range. */
export interface SmoothClamp {
  top: number;
  bottom: number;
}

/**
 * Map raw entries to chart points through an accessor. The accessor
 * returns the point's x and y; a null y drops the entry (a series
 * with gaps, e.g. free space missing from an old scan). The result
 * is sorted by x — time series read left to right.
 */
export function buildSeries<T>(
  entries: readonly T[],
  accessor: (entry: T) => { x: number; y: number | null },
): ChartPoint[] {
  const points: ChartPoint[] = [];
  for (const entry of entries) {
    const { x, y } = accessor(entry);
    if (y === null || !Number.isFinite(x) || !Number.isFinite(y)) continue;
    points.push({ x, y });
  }
  return points.sort((a, b) => a.x - b.x);
}

/**
 * Human-friendly axis ticks between min and max: the step is always
 * 1, 2 or 5 times a power of ten, the first tick is at or below min
 * and the last at or above max, so the ticks double as the scale's
 * domain. A flat range (min === max) yields that single value.
 */
export function niceTicks(min: number, max: number, targetCount = 4): number[] {
  if (!Number.isFinite(min) || !Number.isFinite(max)) return [];
  let lo = min;
  let hi = max;
  if (lo > hi) [lo, hi] = [hi, lo];
  if (lo === hi) return [lo];

  /* The smallest nice step whose interval count stays near the
     target — a range starting mid-step needs a coarser step than
     the raw span suggests, or the axis drowns in gridlines. */
  const magnitude = 10 ** Math.floor(Math.log10((hi - lo) / Math.max(1, targetCount)));
  let step = magnitude;
  for (const multiplier of [1, 2, 5, 10]) {
    step = multiplier * magnitude;
    const intervals = Math.ceil(hi / step) - Math.floor(lo / step);
    if (intervals <= targetCount + 1) break;
  }
  let start = Math.floor(lo / step) * step;
  let end = Math.ceil(hi / step) * step;
  /* A pathological range can still ask for too many ticks; coarsen
     the step until the count is presentable. */
  while (Math.round((end - start) / step) > 12) {
    step *= 2;
    start = Math.floor(lo / step) * step;
    end = Math.ceil(hi / step) * step;
  }
  const count = Math.round((end - start) / step);
  const ticks: number[] = [];
  for (let i = 0; i <= count; i += 1) {
    /* toPrecision trims binary dust (0.30000000000000004) off ticks. */
    ticks.push(Number((start + i * step).toPrecision(12)));
  }
  return ticks;
}

const clamp = (value: number, lo: number, hi: number) =>
  Math.min(hi, Math.max(lo, value));

const fmt = (value: number) => Number(value.toFixed(1));

/**
 * A smooth path through pixel points: Catmull-Rom splines converted
 * to cubic Bézier segments. Control points are clamped into the
 * plot box (when given), so the curve can approach the data's
 * extremes but never swing past them. Two points are a straight
 * line; fewer than two points have no line at all (empty string) —
 * callers render a lone point as a dot instead.
 */
export function smoothPath(
  points: readonly ChartPoint[],
  box?: SmoothClamp,
): string {
  if (points.length < 2) return "";
  const [first, second] = points;
  if (points.length === 2) {
    return `M ${fmt(first.x)},${fmt(first.y)} L ${fmt(second.x)},${fmt(second.y)}`;
  }
  const top = box ? Math.min(box.top, box.bottom) : -Infinity;
  const bottom = box ? Math.max(box.top, box.bottom) : Infinity;
  const cy = (value: number) => (box ? clamp(value, top, bottom) : value);

  let d = `M ${fmt(first.x)},${fmt(first.y)}`;
  for (let i = 0; i < points.length - 1; i += 1) {
    const p0 = points[Math.max(0, i - 1)];
    const p1 = points[i];
    const p2 = points[i + 1];
    const p3 = points[Math.min(points.length - 1, i + 2)];
    const c1x = p1.x + (p2.x - p0.x) / 6;
    const c1y = cy(p1.y + (p2.y - p0.y) / 6);
    const c2x = p2.x - (p3.x - p1.x) / 6;
    const c2y = cy(p2.y - (p3.y - p1.y) / 6);
    d += ` C ${fmt(c1x)},${fmt(c1y)} ${fmt(c2x)},${fmt(c2y)} ${fmt(p2.x)},${fmt(p2.y)}`;
  }
  return d;
}

/** Axis labels are byte sizes throughout the app — the formatter
    axes use, named for the call sites that read better with it. */
export function formatAxisBytes(value: number): string {
  return formatBytes(value);
}
