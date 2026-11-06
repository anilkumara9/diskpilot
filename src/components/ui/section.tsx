import type { ReactNode } from "react";
import { cn } from "@/lib/utils";

export function Section({
  title,
  description,
  action,
  children,
  className,
}: {
  title?: string;
  description?: string;
  action?: ReactNode;
  children: ReactNode;
  className?: string;
}) {
  return (
    <section className={cn("mt-7 first:mt-0", className)}>
      {(title || action) && (
        <div className="mb-2.5 flex items-center justify-between gap-3">
          <div>
            {title ? (
              <h2 className="text-[13px] font-semibold leading-[18px] text-foreground">{title}</h2>
            ) : null}
            {description ? (
              <p className="mt-0.5 text-[12px] leading-4 text-muted">{description}</p>
            ) : null}
          </div>
          {action}
        </div>
      )}
      {children}
    </section>
  );
}
