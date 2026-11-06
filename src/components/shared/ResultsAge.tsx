import { Clock } from "lucide-react";
import { formatRelative } from "@/lib/format";

export function ResultsAge({ finishedAtMs }: { finishedAtMs: number }) {
  return (
    <span className="inline-flex items-center gap-1 text-[12px] text-muted">
      <Clock className="size-3.5" aria-hidden="true" />
      Scanned {formatRelative(finishedAtMs)}
    </span>
  );
}
