import type { LucideIcon } from "lucide-react";
import { cn } from "@/lib/utils";

export interface StatItem {
  label: string;
  value: string;
  /** Optional caption under the value (e.g. a file count, a
   *  reclaimable subtotal). */
  sub?: string;
  /** Optional leading icon for the label, where the page already
   *  has a natural one. Most pages omit it. */
  icon?: LucideIcon;
}

/**
 * StatStrip: the one header-stats pattern for the whole app. A
 * page's headline numbers sit in equal bordered blocks on a
 * responsive grid — never as loose text on the page background —
 * so every page opens with the same composed band: 12 px muted
 * label (with a 14 px icon where the page has one), 20 px
 * semibold tabular value, optional 11 px muted sub-caption.
 * Blocks stretch to equal height and wrap cleanly at the
 * minimum window width.
 *
 * The `bare` variant drops the block outline for strips that
 * already sit inside a surfaced panel (Health's forecast panel),
 * keeping the same grid and type rhythm without boxing a box.
 */
export function StatStrip({
  stats,
  variant = "card",
  className,
}: {
  stats: StatItem[];
  variant?: "card" | "bare";
  className?: string;
}) {
  return (
    <div
      className={cn(
        "grid grid-cols-[repeat(auto-fit,minmax(150px,1fr))] gap-3",
        className,
      )}
    >
      {stats.map((stat) => {
        const Icon = stat.icon;
        return (
          <div
            key={stat.label}
            className={
              variant === "card"
                ? "rounded-[10px] border border-border px-3.5 py-3"
                : undefined
            }
          >
            <p className="flex items-center gap-1.5 text-[12px] text-muted">
              {Icon ? (
                <Icon className="size-3.5 shrink-0" aria-hidden="true" />
              ) : null}
              {stat.label}
            </p>
            <p className="mt-1 text-[20px] font-semibold leading-6 tabular text-foreground">
              {stat.value}
            </p>
            {stat.sub ? (
              <p className="mt-0.5 text-[11px] text-muted">{stat.sub}</p>
            ) : null}
          </div>
        );
      })}
    </div>
  );
}
