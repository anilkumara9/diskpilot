import type { ComponentType, ReactNode } from "react";
import { cn } from "@/lib/utils";

export function EmptyState({
  icon: Icon,
  title,
  description,
  action,
  className,
}: {
  icon: ComponentType<{ className?: string; strokeWidth?: number }>;
  title: string;
  description?: string;
  action?: ReactNode;
  className?: string;
}) {
  return (
    <div
      className={cn(
        "flex flex-col items-center justify-center px-6 py-12 text-center",
        className,
      )}
    >
      <Icon className="size-8 text-subtle" strokeWidth={1.5} aria-hidden="true" />
      <h3 className="mt-3 text-[13px] font-semibold text-foreground">{title}</h3>
      {description ? (
        <p className="mt-1 max-w-[420px] text-[12px] leading-4 text-muted">{description}</p>
      ) : null}
      {action ? <div className="mt-4">{action}</div> : null}
    </div>
  );
}
