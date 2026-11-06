import { Clock } from "lucide-react";
import { formatRelative } from "@/lib/format";
import { useNowTick } from "@/lib/hooks";

export function ResultsAge({ finishedAtMs }: { finishedAtMs: number }) {
  /* Re-render on a 30 s tick so "Scanned 2 minutes ago" ages
     truthfully while the page sits open. */
  const now = useNowTick();
  return (
    <span className="inline-flex items-center gap-1 text-[12px] text-muted">
      <Clock className="size-3.5" aria-hidden="true" />
      Scanned {formatRelative(finishedAtMs, now)}
    </span>
  );
}
