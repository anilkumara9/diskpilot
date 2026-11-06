/**
 * Automatic-maintenance form rules. The age threshold is clamped
 * to 7–365 days on save: shorter would permanently remove bin
 * items the user may still want back; longer is a year, beyond
 * which the setting stops meaning anything. Non-numeric input
 * falls back to the 30-day default rather than saving garbage.
 */
export const MIN_AGE_MIN_DAYS = 7;
export const MIN_AGE_MAX_DAYS = 365;
export const MIN_AGE_DEFAULT_DAYS = 30;

export function clampMinAgeDays(value: number): number {
  if (!Number.isFinite(value)) return MIN_AGE_DEFAULT_DAYS;
  const rounded = Math.round(value);
  return Math.min(MIN_AGE_MAX_DAYS, Math.max(MIN_AGE_MIN_DAYS, rounded));
}
