import { cn } from "@/lib/utils";
import { tierMeta } from "@/lib/tiers";
import type { Tier } from "@/lib/types";

/** The only colored tag in the app: icon + label in the tier color on a faint tint. */
export function TierBadge({ tier, className }: { tier: Tier; className?: string }) {
  const meta = tierMeta(tier);
  const Icon = meta.icon;
  return (
    <span
      className={cn(
        "inline-flex items-center gap-1 rounded-[4px] px-1.5 py-0.5 text-[11px] font-medium leading-[14px]",
        meta.textClass,
        meta.bgSoftClass,
        className,
      )}
    >
      <Icon className="size-3" strokeWidth={1.75} aria-hidden="true" />
      {meta.label}
    </span>
  );
}
