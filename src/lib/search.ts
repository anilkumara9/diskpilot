import type { QueryChip, SearchFilters } from "./types";

/* Pure helpers for the natural-language search UI: chip removal and
   the saved-search list. Kept free of store/API imports so they can
   be unit-tested directly.

   The core parser emits exactly one chip per filter dimension
   ("kind", "size", "date", "source", "term"), so removing a chip
   clears exactly one dimension of the filters and the chip row and
   the filters can never drift apart. */

/**
 * The filters with the chip's dimension cleared, or null when the
 * chip kind is not one this client knows how to drop (a future core
 * dimension) — the UI then renders that chip without a remove
 * button. The input is never mutated.
 */
export function filtersWithoutChip(
  filters: SearchFilters,
  chip: QueryChip,
): SearchFilters | null {
  const next: SearchFilters = {
    ...filters,
    kinds: [...filters.kinds],
    terms: [...filters.terms],
  };
  switch (chip.kind) {
    case "kind":
      next.kinds = [];
      return next;
    case "size":
      next.minBytes = null;
      next.maxBytes = null;
      return next;
    case "date":
      next.modifiedAfterMs = null;
      next.modifiedBeforeMs = null;
      return next;
    case "source":
      next.source = null;
      return next;
    case "term":
      next.terms = [];
      return next;
    default:
      return null;
  }
}

/** True when no dimension constrains the search any more. */
export function filtersAreEmpty(filters: SearchFilters): boolean {
  return (
    filters.kinds.length === 0 &&
    filters.terms.length === 0 &&
    filters.minBytes === null &&
    filters.maxBytes === null &&
    filters.modifiedAfterMs === null &&
    filters.modifiedBeforeMs === null &&
    filters.source === null
  );
}

/** How many saved searches are kept; the oldest fall off the end. */
export const MAX_SAVED_SEARCHES = 12;

/**
 * Add a query to the saved-search list (most recent first). The
 * query is trimmed; blank queries and case-insensitive duplicates
 * leave the list unchanged apart from ordering — re-saving an
 * existing search moves it to the front rather than duplicating it.
 */
export function addSavedSearch(list: string[], query: string): string[] {
  const trimmed = query.trim();
  if (!trimmed) return list;
  const rest = list.filter((saved) => saved.toLowerCase() !== trimmed.toLowerCase());
  return [trimmed, ...rest].slice(0, MAX_SAVED_SEARCHES);
}

/** Remove one saved search (exact match, as stored). */
export function removeSavedSearch(list: string[], query: string): string[] {
  return list.filter((saved) => saved !== query);
}
