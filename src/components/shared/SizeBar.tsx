import { cn } from "@/lib/utils";

/** 2px meter under a size figure, plus a taller standalone variant. */
export function SizeBar({
  fraction,
  colorVar,
  className,
  height = 2,
}: {
  fraction: number;
  colorVar?: string;
  className?: string;
  height?: number;
}) {
  const pct = Math.max(0, Math.min(1, fraction)) * 100;
  return (
    <div
      aria-hidden="true"
      className={cn("w-full overflow-hidden rounded-full bg-hover", className)}
      style={{ height }}
    >
      <div
        className="h-full rounded-full transition-[width] duration-200"
        style={{ width: `${pct}%`, background: colorVar ?? "var(--accent)" }}
      />
    </div>
  );
}
