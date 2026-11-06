/**
 * Keyboard model for the app's tab strips (Storage tabs, the
 * Duplicates mode switch), following the WAI-ARIA tabs pattern:
 * Left/Right move one tab (wrapping), Home/End jump to the ends,
 * and only the selected tab is in the Tab order (roving tabindex).
 * These strips switch instantly, so arrow keys both move focus and
 * select — the pattern's "automatic activation".
 */

/** The index a key press selects, or null when the key is not a
    tab-navigation key. */
export function tabIndexForKey(
  current: number,
  key: string,
  count: number,
): number | null {
  if (count <= 0) return null;
  switch (key) {
    case "ArrowRight":
      return (current + 1) % count;
    case "ArrowLeft":
      return (current - 1 + count) % count;
    case "Home":
      return 0;
    case "End":
      return count - 1;
    default:
      return null;
  }
}
