import type { HTMLAttributes } from "react";
import { cn } from "@/lib/utils";

export function Panel({ className, ...props }: HTMLAttributes<HTMLDivElement>) {
  return (
    <div className={cn("rounded-[10px] border border-border bg-panel", className)} {...props} />
  );
}

export function RaisedPanel({ className, ...props }: HTMLAttributes<HTMLDivElement>) {
  return (
    <div className={cn("rounded-[10px] bg-panel-raised", className)} {...props} />
  );
}
