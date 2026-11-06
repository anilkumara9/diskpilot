import type { ReactNode } from "react";
import { X } from "lucide-react";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";

export function DetailsPanel({
  title,
  subtitle,
  onClose,
  children,
  footer,
  className,
}: {
  title: string;
  subtitle?: ReactNode;
  onClose?: () => void;
  children: ReactNode;
  footer?: ReactNode;
  className?: string;
}) {
  return (
    <aside
      aria-label={`Details for ${title}`}
      className={cn(
        "flex w-[300px] shrink-0 flex-col overflow-hidden rounded-[10px] border border-border bg-panel animate-fade-in",
        className,
      )}
    >
      <div className="flex items-start gap-2 border-b border-border px-3.5 py-3">
        <div className="min-w-0 flex-1">
          <h3 className="truncate text-[13px] font-semibold leading-[18px] text-foreground" title={title}>
            {title}
          </h3>
          {subtitle ? <div className="mt-0.5">{subtitle}</div> : null}
        </div>
        {onClose ? (
          <Button variant="ghost" size="icon" onClick={onClose} aria-label="Close details panel">
            <X className="size-4" />
          </Button>
        ) : null}
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto px-3.5 py-3">{children}</div>
      {footer ? <div className="border-t border-border px-3.5 py-3">{footer}</div> : null}
    </aside>
  );
}

export function DetailRow({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="flex items-baseline justify-between gap-3 py-1 text-[12px] leading-4">
      <span className="shrink-0 text-muted">{label}</span>
      <span className="min-w-0 text-right font-medium text-foreground">{children}</span>
    </div>
  );
}
