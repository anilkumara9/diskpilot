import { cn } from "@/lib/utils";

export function Skeleton({ className }: { className?: string }) {
  return <div aria-hidden="true" className={cn("skeleton-shimmer rounded-md", className)} />;
}

export function SkeletonRows({ rows = 5, rowClass = "h-8" }: { rows?: number; rowClass?: string }) {
  return (
    <div className="space-y-1.5" aria-busy="true" aria-label="Loading">
      {Array.from({ length: rows }).map((_, i) => (
        <Skeleton key={i} className={rowClass} />
      ))}
    </div>
  );
}
