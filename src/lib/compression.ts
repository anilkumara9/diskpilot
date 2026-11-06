/**
 * Display helpers for the Cleanup page's Compress & save section.
 * Candidates are folders the scan found large and long untouched;
 * the "untouched" figure derives from the newest modification
 * anywhere inside the folder — the last time anything in it
 * changed.
 */

const DAY_MS = 86_400_000;

/** Whole calendar months between two instants (the day-of-month
    must also have been reached, so Jan 31 → Feb 29 is one month,
    Jan 15 → Feb 14 is none). */
export function wholeMonthsBetween(fromMs: number, toMs: number): number {
  if (!Number.isFinite(fromMs) || !Number.isFinite(toMs) || toMs <= fromMs) return 0;
  const from = new Date(fromMs);
  const to = new Date(toMs);
  let months =
    (to.getFullYear() - from.getFullYear()) * 12 + (to.getMonth() - from.getMonth());
  if (to.getDate() < from.getDate()) months -= 1;
  return Math.max(0, months);
}

/** "Untouched for 14 months" — with honest fallbacks for folders
    touched recently (weeks, then days) so the label never claims
    age the data does not show. */
export function untouchedLabel(newestModifiedMs: number, nowMs: number = Date.now()): string {
  const months = wholeMonthsBetween(newestModifiedMs, nowMs);
  if (months >= 12) {
    const years = Math.floor(months / 12);
    const rest = months % 12;
    const yearPart = years === 1 ? "1 year" : `${years} years`;
    if (rest === 0) return `Untouched for ${yearPart}`;
    return `Untouched for ${yearPart} ${rest} month${rest === 1 ? "" : "s"}`;
  }
  if (months >= 1) return `Untouched for ${months} month${months === 1 ? "" : "s"}`;
  const days = Math.floor((nowMs - newestModifiedMs) / DAY_MS);
  if (days >= 14) return `Untouched for ${Math.floor(days / 7)} weeks`;
  if (days >= 1) return `Untouched for ${days} day${days === 1 ? "" : "s"}`;
  return "Touched within the last day";
}
