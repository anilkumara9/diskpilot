import { cn } from "@/lib/utils";
import { categoryMeta } from "@/lib/categories";
import type { Category } from "@/lib/types";

export function CategoryDot({ category, className }: { category: Category; className?: string }) {
  const meta = categoryMeta(category);
  return (
    <span
      aria-hidden="true"
      className={cn("inline-block size-2 shrink-0 rounded-full", meta.bgClass, className)}
    />
  );
}

/** Category label = colored dot + plain text, never a filled pill. */
export function CategoryBadge({
  category,
  className,
  showLabel = true,
}: {
  category: Category;
  className?: string;
  showLabel?: boolean;
}) {
  const meta = categoryMeta(category);
  return (
    <span className={cn("inline-flex shrink-0 items-center gap-1.5 whitespace-nowrap text-[12px] text-foreground", className)}>
      <CategoryDot category={category} />
      {showLabel ? <span className="whitespace-nowrap">{meta.label}</span> : null}
    </span>
  );
}
