import { useEffect, useState } from "react";

/**
 * A "now" that re-renders on an interval, for relative-time labels
 * ("Scanned 3 minutes ago") that would otherwise freeze at whatever
 * they said when the component mounted. Returns the current time in
 * ms and refreshes it every `intervalMs` (30 s by default — fine
 * enough for minute-grained labels, cheap enough to run always).
 */
export function useNowTick(intervalMs = 30_000): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const timer = window.setInterval(() => setNow(Date.now()), intervalMs);
    return () => window.clearInterval(timer);
  }, [intervalMs]);
  return now;
}
