import { CATEGORY_ORDER, categoryMeta } from "@/lib/categories";
import type { Category } from "@/lib/types";
import { Select } from "@/components/ui/input";
import { cn } from "@/lib/utils";

export type CategoryFilter = Category | "all";

export function CategorySelect({
  value,
  onChange,
  className,
  ariaLabel = "Filter by category",
}: {
  value: CategoryFilter;
  onChange: (value: CategoryFilter) => void;
  className?: string;
  ariaLabel?: string;
}) {
  return (
    <Select
      aria-label={ariaLabel}
      value={value}
      onChange={(e) => onChange(e.target.value as CategoryFilter)}
      className={cn("w-[170px]", className)}
    >
      <option value="all">All categories</option>
      {CATEGORY_ORDER.map((cat) => (
        <option key={cat} value={cat}>
          {categoryMeta(cat).label}
        </option>
      ))}
    </Select>
  );
}
