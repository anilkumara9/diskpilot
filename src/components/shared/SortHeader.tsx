import { ArrowDown, ArrowUp, ArrowUpDown } from "lucide-react";
import { cn } from "@/lib/utils";
import type { SortDirection } from "@/lib/analysis";

export function SortHeader({
  label,
  sortKeyValue,
  activeKey,
  direction,
  onSort,
  align = "left",
  className,
}: {
  label: string;
  sortKeyValue: string;
  activeKey: string;
  direction: SortDirection;
  onSort: (key: string) => void;
  align?: "left" | "right";
  className?: string;
}) {
  const active = activeKey === sortKeyValue;
  return (
    <button
      type="button"
      onClick={() => onSort(sortKeyValue)}
      aria-sort={active ? (direction === "asc" ? "ascending" : "descending") : undefined}
      className={cn(
        "inline-flex items-center gap-1 text-[12px] font-medium text-muted transition-colors duration-120 hover:text-foreground",
        align === "right" && "flex-row-reverse",
        active && "text-foreground",
        className,
      )}
    >
      {label}
      {active ? (
        direction === "asc" ? (
          <ArrowUp className="size-3" aria-hidden="true" />
        ) : (
          <ArrowDown className="size-3" aria-hidden="true" />
        )
      ) : (
        <ArrowUpDown className="size-3 opacity-50" aria-hidden="true" />
      )}
    </button>
  );
}
