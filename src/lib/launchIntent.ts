/**
 * Launch-intent consumption rule for Explorer's "Analyze with
 * DiskPilot". The shell delivers each requested folder once
 * (reading the intent takes it), but one Explorer click can
 * still reach the app twice — the mount-time read and the
 * launch-scan event race each other. The guard therefore
 * dedupes per delivery, not per session: a delivery is ignored
 * only while the scan it started is still running and barely
 * began (the double-delivery window). Once that scan ends, or
 * the window passes, the same folder delivered again scans
 * again — re-analyzing a folder from Explorer always works.
 */
export interface IntentDelivery {
  path: string;
  startedAt: number;
}

/** How long after a scan starts a same-path delivery still
    counts as the same request, in milliseconds. */
export const INTENT_DEDUPE_WINDOW_MS = 5_000;

/**
 * Whether a delivered intent path should be acted on. `last` is
 * the most recent delivery that started a scan (null once no
 * scan from an intent is in flight), `scanning` is the store's
 * live scan flag.
 */
export function shouldConsumeIntent(
  last: IntentDelivery | null,
  path: string,
  now: number,
  scanning: boolean,
): boolean {
  if (!last || last.path !== path) return true;
  if (!scanning) return true;
  return now - last.startedAt >= INTENT_DEDUPE_WINDOW_MS;
}
