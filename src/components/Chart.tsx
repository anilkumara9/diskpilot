import {
  useEffect,
  useId,
  useMemo,
  useState,
} from "react";
import { niceTicks, smoothPath, type ChartPoint } from "@/lib/chart";

export interface ChartSeries {
  label: string;
  /** Any CSS colour — a token var ("var(--accent)") or a hex value. */
  color: string;
  points: ChartPoint[];
  formatY: (value: number) => string;
}

const dayFormat = new Intl.DateTimeFormat("en-US", {
  month: "short",
  day: "numeric",
  year: "numeric",
});

function defaultFormatX(x: number): string {
  return dayFormat.format(new Date(x));
}

const Y_GUTTER = 48;

/**
 * The app's one line/area chart: a smooth curve over a gradient
 * area, nice-tick gridlines, a crosshair with a floating tooltip,
 * and keyboard stepping. The first series is the primary — it
 * carries the area fill, the end-point dot, and the snapping the
 * crosshair follows; later series are lines on the same scale.
 *
 * `height` is the plot height; axis captions sit beneath it. In
 * `mini` configuration the y-axis labels and grid collapse to a
 * single baseline — the sparkline form. A single point renders as
 * a dot with a caption (never a fake line); no points render
 * nothing, leaving the caller's empty state in charge.
 *
 * Data is drawn in once on mount (line draw + area fade, ~600 ms);
 * the global reduced-motion rule collapses that to instant.
 */
export function Chart({
  series,
  height = 140,
  ariaLabel,
  mini = false,
  formatX = defaultFormatX,
}: {
  series: ChartSeries[];
  height?: number;
  ariaLabel: string;
  mini?: boolean;
  formatX?: (x: number) => string;
}) {
  const [container, setContainer] = useState<HTMLDivElement | null>(null);
  const [width, setWidth] = useState(0);
  const [activeIndex, setActiveIndex] = useState<number | null>(null);
  const [drawn, setDrawn] = useState(false);
  const gradientBase = useId().replace(/[^a-zA-Z0-9]/g, "");

  /* The rendered element swaps once (placeholder → measured chart),
     so the observer follows the container through state. */
  useEffect(() => {
    if (!container) return;
    const observer = new ResizeObserver((entries) => {
      const rect = entries[0]?.contentRect;
      if (rect && rect.width > 0) setWidth(rect.width);
    });
    observer.observe(container);
    return () => observer.disconnect();
  }, [container]);

  /* Draw-in runs once, after the first measured paint. */
  useEffect(() => {
    if (width === 0 || drawn) return;
    let inner = 0;
    const outer = requestAnimationFrame(() => {
      inner = requestAnimationFrame(() => setDrawn(true));
    });
    return () => {
      cancelAnimationFrame(outer);
      cancelAnimationFrame(inner);
    };
  }, [width, drawn]);

  const primaryIndex = series.findIndex((s) => s.points.length > 0);
  const primary = primaryIndex >= 0 ? series[primaryIndex] : null;

  const model = useMemo(() => {
    if (!primary || width === 0) return null;
    const gutter = mini ? 0 : Y_GUTTER;
    const plotWidth = Math.max(1, width - gutter);
    const plotHeight = height;

    const allPoints = series.flatMap((s) => s.points);
    const xs = allPoints.map((p) => p.x);
    const ys = allPoints.map((p) => p.y);
    const xMin = Math.min(...xs);
    const xMax = Math.max(...xs);
    const ticks = niceTicks(Math.min(...ys), Math.max(...ys), 4);
    let yLo = ticks[0];
    let yHi = ticks[ticks.length - 1];
    if (yLo === yHi) {
      const pad = Math.max(Math.abs(yLo) * 0.05, 1);
      yLo -= pad;
      yHi += pad;
    }
    const x = (value: number) =>
      xMax === xMin
        ? gutter + plotWidth / 2
        : gutter + ((value - xMin) / (xMax - xMin)) * plotWidth;
    const y = (value: number) =>
      plotHeight - ((value - yLo) / (yHi - yLo)) * plotHeight;

    const scaled = series.map((s) =>
      s.points.map((p) => ({ x: x(p.x), y: y(p.y), data: p })),
    );
    const primaryPoints = scaled[primaryIndex];
    const linePaths = scaled.map((points) =>
      smoothPath(points, { top: 0, bottom: plotHeight }),
    );
    const first = primaryPoints[0];
    const last = primaryPoints[primaryPoints.length - 1];
    const areaPath =
      primaryPoints.length >= 2
        ? `${linePaths[primaryIndex]} L ${last.x.toFixed(1)},${plotHeight} L ${first.x.toFixed(1)},${plotHeight} Z`
        : "";
    return {
      gutter,
      ticks,
      x,
      y,
      scaled,
      linePaths,
      areaPath,
      first,
      last,
      xMin,
      xMax,
    };
  }, [series, primary, primaryIndex, width, height, mini]);

  if (!primary) return null;

  /* A lone point is a fact, not a trend: dot + caption. */
  if (primary.points.length === 1) {
    const point = primary.points[0];
    return (
      <div>
        <div
          className="relative flex items-center justify-center"
          style={{ height }}
          role="img"
          aria-label={`${ariaLabel}: ${formatX(point.x)}, ${primary.label} ${primary.formatY(point.y)}`}
        >
          <span
            className="absolute inline-block size-[15px] rounded-full"
            style={{ background: primary.color, opacity: 0.18 }}
            aria-hidden="true"
          />
          <span
            className="relative inline-block size-[7px] rounded-full border-2"
            style={{ background: primary.color, borderColor: "var(--panel-raised)" }}
            aria-hidden="true"
          />
        </div>
        <p className="pt-1.5 text-center text-[11px] text-subtle">
          {formatX(point.x)} · {primary.label} {primary.formatY(point.y)}
        </p>
      </div>
    );
  }

  if (!model) {
    /* Space is reserved until the first measurement lands. */
    return <div ref={setContainer} style={{ height }} aria-hidden="true" />;
  }

  const active =
    activeIndex !== null && model.scaled[primaryIndex][activeIndex]
      ? model.scaled[primaryIndex][activeIndex]
      : null;
  const activeX = active ? active.data.x : null;

  const nearestInSeries = (points: ChartPoint[], targetX: number) => {
    let best = points[0];
    for (const point of points) {
      if (Math.abs(point.x - targetX) < Math.abs(best.x - targetX)) best = point;
    }
    return best;
  };

  const handlePointerMove = (event: React.PointerEvent<HTMLDivElement>) => {
    const rect = event.currentTarget.getBoundingClientRect();
    const px = event.clientX - rect.left;
    const points = model.scaled[primaryIndex];
    let best = 0;
    for (let i = 1; i < points.length; i += 1) {
      if (Math.abs(points[i].x - px) < Math.abs(points[best].x - px)) best = i;
    }
    setActiveIndex(best);
  };

  const handleKeyDown = (event: React.KeyboardEvent) => {
    const lastIndex = primary.points.length - 1;
    if (event.key === "ArrowRight") {
      event.preventDefault();
      setActiveIndex((i) => Math.min(lastIndex, (i ?? lastIndex) + 1));
    } else if (event.key === "ArrowLeft") {
      event.preventDefault();
      setActiveIndex((i) => Math.max(0, (i ?? lastIndex) - 1));
    }
  };

  const describedPoint =
    active && activeX !== null
      ? `${formatX(activeX)} — ${series
          .filter((s) => s.points.length > 0)
          .map((s) => {
            const point = nearestInSeries(s.points, activeX);
            return `${s.label} ${s.formatY(point.y)}`;
          })
          .join(", ")}`
      : null;

  const gridTicks = mini ? [model.ticks[0]] : model.ticks;
  const flipTooltip = active !== null && active.x > width * 0.6;
  const tooltipAbove = active !== null && active.y >= 76;

  return (
    <div>
      <div
        ref={setContainer}
        className="relative outline-none"
        style={{ height }}
        role="img"
        aria-label={describedPoint ? `${ariaLabel}: ${describedPoint}` : ariaLabel}
        tabIndex={0}
        onPointerMove={handlePointerMove}
        onPointerLeave={() => setActiveIndex(null)}
        onFocus={() => {
          if (activeIndex === null) setActiveIndex(primary.points.length - 1);
        }}
        onBlur={() => setActiveIndex(null)}
        onKeyDown={handleKeyDown}
      >
        {/* Y-axis labels, in the reserved gutter */}
        {!mini
          ? gridTicks.map((tick) => (
              <span
                key={tick}
                className="tabular pointer-events-none absolute left-0 pr-2 text-right text-[11px] leading-[14px] text-muted"
                style={{ top: model.y(tick) - 7, width: model.gutter }}
                aria-hidden="true"
              >
                {primary.formatY(tick)}
              </span>
            ))
          : null}

        <svg
          width={width}
          height={height}
          viewBox={`0 0 ${width} ${height}`}
          className="block"
          aria-hidden="true"
        >
          <defs>
            {series.map((s, i) => (
              <linearGradient
                key={s.label}
                id={`${gradientBase}-${i}`}
                x1="0"
                y1="0"
                x2="0"
                y2="1"
              >
                <stop offset="0%" style={{ stopColor: s.color }} stopOpacity={0.24} />
                <stop offset="100%" style={{ stopColor: s.color }} stopOpacity={0} />
              </linearGradient>
            ))}
          </defs>

          {/* Grid */}
          {gridTicks.map((tick) => (
            <line
              key={tick}
              x1={model.gutter}
              x2={width}
              y1={model.y(tick)}
              y2={model.y(tick)}
              stroke="var(--border)"
              strokeWidth={1}
              strokeDasharray="2 4"
              opacity={0.6}
              vectorEffect="non-scaling-stroke"
            />
          ))}

          {/* Area + line per series (area under the primary only) */}
          {series.map((s, i) => (
            <g key={s.label}>
              {i === primaryIndex && model.areaPath ? (
                <path
                  d={model.areaPath}
                  fill={`url(#${gradientBase}-${i})`}
                  opacity={drawn ? 1 : 0}
                  style={{ transition: "opacity 600ms ease-out" }}
                />
              ) : null}
              {model.linePaths[i] ? (
                <path
                  d={model.linePaths[i]}
                  fill="none"
                  stroke={s.color}
                  strokeWidth={1.5}
                  strokeLinejoin="round"
                  strokeLinecap="round"
                  vectorEffect="non-scaling-stroke"
                  pathLength={1}
                  strokeDasharray={1}
                  strokeDashoffset={drawn ? 0 : 1}
                  style={{ transition: "stroke-dashoffset 600ms ease-out" }}
                />
              ) : null}
            </g>
          ))}

          {/* End point of the primary series */}
          <g opacity={drawn ? 1 : 0} style={{ transition: "opacity 600ms ease-out" }}>
            <circle cx={model.last.x} cy={model.last.y} r={8} fill={primary.color} opacity={0.18} />
            <circle
              cx={model.last.x}
              cy={model.last.y}
              r={3.5}
              fill={primary.color}
              stroke="var(--panel-raised)"
              strokeWidth={2}
            />
          </g>

          {/* Crosshair */}
          {active ? (
            <g>
              <line
                x1={active.x}
                x2={active.x}
                y1={0}
                y2={height}
                stroke="var(--border-strong)"
                strokeWidth={1}
                vectorEffect="non-scaling-stroke"
              />
              {series.map((s, i) => {
                const point = model.scaled[i].length
                  ? nearestInSeries(model.scaled[i], active.x)
                  : null;
                return point ? (
                  <circle
                    key={s.label}
                    cx={point.x}
                    cy={point.y}
                    r={3}
                    fill={s.color}
                    stroke="var(--panel-raised)"
                    strokeWidth={1.5}
                  />
                ) : null;
              })}
            </g>
          ) : null}
        </svg>

        {/* Tooltip */}
        {active && activeX !== null ? (
          <div
            className="pointer-events-none absolute z-50 w-max max-w-[240px] rounded-md border border-border-strong bg-panel-raised px-2.5 py-1.5 shadow-[0_10px_28px_rgba(0,0,0,0.3)] animate-fade-in"
            style={{
              left: active.x,
              top: active.y,
              transform: `translate(${flipTooltip ? "calc(-100% - 10px)" : "10px"}, ${
                tooltipAbove ? "calc(-100% - 10px)" : "14px"
              })`,
            }}
            aria-hidden="true"
          >
            <p className="text-[11px] text-muted">{formatX(activeX)}</p>
            <ul className="mt-0.5 space-y-0.5">
              {series
                .filter((s) => s.points.length > 0)
                .map((s) => {
                  const point = nearestInSeries(s.points, activeX);
                  return (
                    <li key={s.label} className="flex items-center gap-2 text-[12px] leading-4">
                      <span
                        className="inline-block size-2 shrink-0 rounded-full"
                        style={{ background: s.color }}
                      />
                      <span className="text-muted">{s.label}</span>
                      <span className="tabular ml-auto pl-3 text-foreground">
                        {s.formatY(point.y)}
                      </span>
                    </li>
                  );
                })}
            </ul>
          </div>
        ) : null}
      </div>

      {/* X-axis end captions */}
      <div
        className="flex items-baseline justify-between gap-4 pt-1.5 text-[11px] text-subtle"
        style={{ paddingLeft: model.gutter }}
        aria-hidden="true"
      >
        <span>{formatX(model.xMin)}</span>
        <span>{formatX(model.xMax)}</span>
      </div>
    </div>
  );
}
