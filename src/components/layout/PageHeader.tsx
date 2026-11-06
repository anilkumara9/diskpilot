import type { ReactNode } from "react";
import { ScanControl } from "./ScanControl";

export function PageHeader({
  title,
  subtitle,
  actions,
  showScanControl = true,
}: {
  title: string;
  subtitle?: string;
  actions?: ReactNode;
  showScanControl?: boolean;
}) {
  return (
    <header className="flex items-start justify-between gap-4 pb-4">
      <div className="min-w-0">
        <h1 className="text-[15px] font-semibold leading-5 text-foreground">{title}</h1>
        {subtitle ? <p className="mt-0.5 text-[12px] leading-4 text-muted">{subtitle}</p> : null}
      </div>
      <div className="flex shrink-0 items-center gap-2">
        {actions}
        {showScanControl ? <ScanControl /> : null}
      </div>
    </header>
  );
}
