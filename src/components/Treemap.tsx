import { useEffect, useMemo, useRef, useState } from "react";
import { formatBytes } from "@/lib/format";
import { squarifyTreemap } from "@/lib/treemap";
import { cn } from "@/lib/utils";

/**
 * The Storage map: the current folder level as proportional blocks.
 * Block colours come from the design tokens (the entry's category
 * colour, mixed down to a tint so labels stay readable in both
 * themes). Layout is the pure squarified treemap in lib/treemap;
 * this component only measures its container and paints.
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
}

const GAP = 2;

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

  return (
    <div
      ref={containerRef}
      className="relative h-full w-full overflow-hidden rounded-[10px] border border-border bg-panel"
      role="group"
      aria-label="Folder size map"
    >
      {nodes.map((node) => {
        const block = byId.get(node.id);
        if (!block) return null;
        const width = Math.max(0, node.rect.width - GAP);
        const height = Math.max(0, node.rect.height - GAP);
        const showName = width >= 72 && height >= 32;
        const showSize = width >= 72 && height >= 48;
        const style: React.CSSProperties = {
          left: node.rect.x + GAP / 2,
          top: node.rect.y + GAP / 2,
          width,
          height,
          background: `color-mix(in srgb, ${block.colorVar} 26%, transparent)`,
          borderColor: `color-mix(in srgb, ${block.colorVar} 55%, transparent)`,
        };
        const content = (
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
        const title = `${block.name} — ${formatBytes(block.bytes)}`;
        if (block.aggregate || !onActivate) {
          return (
            <div
              key={block.id}
              title={title}
              style={style}
              className="absolute overflow-hidden rounded-md border p-1.5"
            >
              {content}
            </div>
          );
        }
        return (
          <button
            key={block.id}
            type="button"
            title={title}
            aria-label={`${block.name}, ${formatBytes(block.bytes)}`}
            aria-pressed={selectedId === block.id}
            onClick={() => onActivate(block)}
            style={style}
            className={cn(
              "absolute overflow-hidden rounded-md border p-1.5 text-left transition-[box-shadow] duration-120 hover:shadow-[inset_0_0_0_1px_var(--border-strong)]",
              selectedId === block.id && "shadow-[inset_0_0_0_2px_var(--accent)]",
            )}
          >
            {content}
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
