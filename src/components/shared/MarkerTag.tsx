import { Badge } from "@/components/ui/badge";
import { cn } from "@/lib/utils";

/** Small neutral marker for states like Clone, Hard link, Leftover, Stale. */
export function MarkerTag({
  children,
  tone = "neutral",
  className,
}: {
  children: string;
  tone?: "neutral" | "warn" | "accent";
  className?: string;
}) {
  return (
    <Badge
      className={cn(
        tone === "neutral" && "bg-panel-raised text-muted",
        tone === "warn" && "bg-tier-review/12 text-tier-review",
        tone === "accent" && "bg-accent/12 text-accent",
        className,
      )}
    >
      {children}
    </Badge>
  );
}
