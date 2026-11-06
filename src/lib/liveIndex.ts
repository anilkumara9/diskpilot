import type { ScanSummary } from "./types";

/** Which live-index chip the Overview meta line shows, if any.
    Stale wins over live: a summary the shell could not keep
    exactly current must never present itself as live. */
export type LiveChipState = "live" | "stale" | null;

export function liveChipState(
  summary: Pick<ScanSummary, "liveIndexActive" | "indexStale"> | null | undefined,
): LiveChipState {
  if (!summary) return null;
  if (summary.indexStale) return "stale";
  if (summary.liveIndexActive) return "live";
  return null;
}
