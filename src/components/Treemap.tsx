import { useEffect, useMemo, useRef, useState } from "react";
import { formatBytes, formatCount } from "@/lib/format";
import { squarifyTreemap } from "@/lib/treemap";
import { cn } from "@/lib/utils";

/**
 * The Storage map: the current folder level as proportional blocks.
 * Each block is filled with its category colour mixed into the
 * panel colour (the `--map-tint` token sets the strength per
 * theme); neighbouring blocks of the same category alternate a
 * ±4-point tint step so individual files still read as distinct
 * tiles. Blocks sit on a 2 px gap that shows the surface beneath,
 * with a 4 px radius. The aggregate block is a waffle of small
 * cells — one per folded item, up to 400 — so a long tail reads
 * as "many small files", never as an empty slab. Layout is the
 * pure squarified treemap in lib/treemap; this component only
 * measures its container and paints.
 */

export interface TreemapBlock {
  id: string;
  name: string;
  bytes: number;
  /** A design-token colour value, e.g. a category `colorVar`. */
  colorVar: string;
  isDir: boolean;
  path: string;
  /** Aggregate blocks ("other files") are not interactive. */
  aggregate?: boolean;
  /** Aggregate only: how many items the block stands for. */
  count?: number;
}

const GAP = 2;
const WAFFLE_MAX_CELLS = 400;

/** Tint offset per same-colour occurrence: the first block sits at
    the base tint, later neighbours alternate +4 / −4 points. */
function tintOffset(occurrence: number): number {
  if (occurrence <= 0) return 0;
  return occurrence % 2 === 1 ? 4 : -4;
}

function WaffleCells({ count, width, height }: { count: number; width: number; height: number }) {
  const cells = Math.min(count, WAFFLE_MAX_CELLS);
  const cols = Math.max(1, Math.min(64, Math.round(Math.sqrt(cells * (width / Math.max(height, 1))))));
  return (
    <div
      aria-hidden="true"
      className="absolute inset-0 grid gap-[2px] p-[3px]"
      style={{ gridTemplateColumns: `repeat(${cols}, minmax(0, 1fr))` }}
    >
      {Array.from({ length: cells }, (_, i) => (
        <span
          key={i}
          className="rounded-[2px]"
          style={{ background: "color-mix(in srgb, var(--foreground) 15%, transparent)" }}
        />
      ))}
    </div>
  );
}

export function Treemap({
  blocks,
  selectedId,
  onActivate,
}: {
  blocks: TreemapBlock[];
  selectedId?: string | null;
  onActivate?: (block: TreemapBlock) => void;
}) {
  const containerRef = useRef<HTMLDivElement>(null);
  const [size, setSize] = useState({ width: 800, height: 480 });

  useEffect(() => {
    const element = containerRef.current;
    if (!element) return;
    const observer = new ResizeObserver((entries) => {
      const rect = entries[0]?.contentRect;
      if (rect && rect.width > 0 && rect.height > 0) {
        setSize({ width: rect.width, height: rect.height });
      }
    });
    observer.observe(element);
    return () => observer.disconnect();
  }, []);

  const nodes = useMemo(
    () =>
      squarifyTreemap(
        blocks.map((b) => ({ id: b.id, value: b.bytes })),
        { x: 0, y: 0, width: size.width, height: size.height },
      ),
    [blocks, size],
  );
  const byId = useMemo(() => new Map(blocks.map((b) => [b.id, b])), [blocks]);
  const tintOffsets = useMemo(() => {
    const seen = new Map<string, number>();
    const offsets = new Map<string, number>();
    for (const block of blocks) {
      const occurrence = seen.get(block.colorVar) ?? 0;
      seen.set(block.colorVar, occurrence + 1);
      offsets.set(block.id, tintOffset(occurrence));
    }
    return offsets;
  }, [blocks]);

  return (
    <div
      ref={containerRef}
      className="relative h-full w-full overflow-hidden rounded-[10px] border border-border bg-background"
      role="group"
      aria-label="Folder size map"
    >
      {nodes.map((node) => {
        const block = byId.get(node.id);
        if (!block) return null;
        const width = Math.max(0, node.rect.width - GAP);
        const height = Math.max(0, node.rect.height - GAP);
        const showName = width >= 64 && height >= 30;
        const showSize = width >= 64 && height >= 46;
        const isAggregate = Boolean(block.aggregate);
        const style: React.CSSProperties = {
          left: node.rect.x + GAP / 2,
          top: node.rect.y + GAP / 2,
          width,
          height,
          background: isAggregate
            ? "var(--panel-raised)"
            : `color-mix(in srgb, ${block.colorVar} calc(var(--map-tint) + ${tintOffsets.get(block.id) ?? 0}%), var(--panel))`,
        };
        const label = (
          <>
            {showName ? (
              <span className="block truncate text-[12px] font-medium leading-4 text-foreground">
                {block.name}
              </span>
            ) : null}
            {showSize ? (
              <span className="block text-[11px] tabular leading-4 text-muted">
                {formatBytes(block.bytes)}
              </span>
            ) : null}
          </>
        );
        const title = isAggregate && block.count
          ? `${formatCount(block.count)} more items — ${formatBytes(block.bytes)} in total`
          : `${block.name} — ${formatBytes(block.bytes)}`;
        if (isAggregate) {
          const showWaffle = width >= 40 && height >= 24 && (block.count ?? 0) > 0;
          return (
            <div
              key={block.id}
              title={title}
              style={style}
              className="absolute overflow-hidden rounded-[4px]"
            >
              {showWaffle ? <WaffleCells count={block.count as number} width={width} height={height} /> : null}
              {showName ? (
                <span
                  className="absolute left-1.5 top-1.5 rounded px-1.5 py-0.5"
                  style={{ background: "color-mix(in srgb, var(--panel) 88%, transparent)" }}
                >
                  {label}
                </span>
              ) : null}
            </div>
          );
        }
        const commonProps = {
          key: block.id,
          title,
          style,
          className: "absolute overflow-hidden rounded-[4px] p-1.5",
        };
        if (!onActivate) {
          return (
            <div {...commonProps}>
              {label}
            </div>
          );
        }
        return (
          <button
            {...commonProps}
            type="button"
            aria-label={`${block.name}, ${formatBytes(block.bytes)}`}
            aria-pressed={selectedId === block.id}
            onClick={() => onActivate(block)}
            className={cn(
              commonProps.className,
              "text-left transition-[box-shadow] duration-120 hover:shadow-[inset_0_0_0_1px_var(--border-strong)]",
              selectedId === block.id && "shadow-[inset_0_0_0_2px_var(--accent)]",
            )}
          >
            {label}
          </button>
        );
      })}
      {blocks.length === 0 ? (
        <p className="absolute inset-0 flex items-center justify-center text-[12px] text-muted">
          Nothing here takes measurable space.
        </p>
      ) : null}
    </div>
  );
}
