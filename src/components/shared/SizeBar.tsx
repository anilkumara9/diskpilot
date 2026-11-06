import { useEffect, useState } from "react";
import { cn } from "@/lib/utils";

/**
 * The one meter for the whole app. At 4 px and taller the fill is a
 * gradient — the full series colour easing to the same colour
 * lightened (82 % colour into white) — over a track of the border
 * token at half strength; at 2 px the fill stays flat, where a
 * gradient could not be seen anyway. The width grows in on mount
 * and transitions when the fraction changes; the global
 * reduced-motion rule collapses both to instant.
 */
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
  const [mounted, setMounted] = useState(false);
  useEffect(() => {
    let inner = 0;
    const outer = requestAnimationFrame(() => {
      inner = requestAnimationFrame(() => setMounted(true));
    });
    return () => {
      cancelAnimationFrame(outer);
      cancelAnimationFrame(inner);
    };
  }, []);
  const color = colorVar ?? "var(--accent)";
  return (
    <div
      aria-hidden="true"
      className={cn("w-full overflow-hidden rounded-full", className)}
      style={{
        height,
        background: "color-mix(in srgb, var(--border) 50%, transparent)",
      }}
    >
      <div
        className="h-full rounded-full transition-[width] duration-300 ease-out"
        style={{
          width: mounted ? `${pct}%` : "0%",
          background:
            height >= 4
              ? `linear-gradient(90deg, ${color}, color-mix(in srgb, ${color} 82%, white))`
              : color,
        }}
      />
    </div>
  );
}
