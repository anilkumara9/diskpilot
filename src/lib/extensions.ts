/**
 * Display helpers for the Storage page's Types view, over the
 * shell's per-extension statistics for the current scan.
 */

/** The chip label for one extension: ".zip" — or the spelled-out
    "(no extension)" for the empty extension. Tolerant of engines
    that include the leading dot. */
export function extensionLabel(extension: string): string {
  if (extension === "") return "(no extension)";
  return extension.startsWith(".") ? extension : `.${extension}`;
}

/**
 * The share of the scanned bytes covered by the `top` largest
 * types, as a fraction (0–1). Zero when nothing was scanned or no
 * types exist — never NaN.
 */
export function topTypesCoverage(
  stats: { bytes: number }[],
  totalBytes: number,
  top = 10,
): number {
  if (!Number.isFinite(totalBytes) || totalBytes <= 0 || stats.length === 0) return 0;
  const sorted = [...stats].sort((a, b) => b.bytes - a.bytes);
  const sum = sorted.slice(0, top).reduce((acc, s) => acc + s.bytes, 0);
  return Math.min(1, sum / totalBytes);
}
