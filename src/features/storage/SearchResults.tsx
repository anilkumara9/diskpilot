import { useState } from "react";
import { ArrowLeft, Bookmark, FolderOpen, SearchX, X } from "lucide-react";
import * as api from "@/lib/api";
import { useAppStore } from "@/lib/store";
import { categoryMeta } from "@/lib/categories";
import { displayPath, formatBytes, formatCount, formatDateShort } from "@/lib/format";
import { filtersWithoutChip } from "@/lib/search";
import type { QueryChip, SearchFilters, SearchHit } from "@/lib/types";
import { Button } from "@/components/ui/button";
import { CategoryDot } from "@/components/shared/CategoryBadge";
import { EmptyState } from "@/components/shared/EmptyState";
import { TextMatchChip } from "@/components/shared/TextMatchChip";

/**
 * Results of a natural-language search. The chip row is the app's
 * contract with the user: it shows exactly how the query was
 * understood, and removing a chip re-runs the search with that one
 * dimension dropped — the filters object the shell returned is the
 * single source of truth, adjusted only through `filtersWithoutChip`.
 */

export interface ActiveSearch {
  query: string;
  chips: QueryChip[];
  filters: SearchFilters;
  hits: SearchHit[];
  totalMatches: number;
}

/** One "understood as" chip. Shared with Quick Search, which renders
    the same interpretation row over the same filters object. */
export function Chip({
  chip,
  removable,
  disabled,
  onRemove,
}: {
  chip: QueryChip;
  removable: boolean;
  disabled: boolean;
  onRemove: (chip: QueryChip) => void;
}) {
  return (
    <span className="inline-flex items-center gap-1 rounded-full bg-panel-raised py-0.5 pl-2 pr-1 text-[11px] leading-[14px] ring-1 ring-border">
      <span className="font-medium text-foreground">{chip.label}</span>
      {removable ? (
        <button
          type="button"
          disabled={disabled}
          onClick={() => onRemove(chip)}
          aria-label={`Remove filter ${chip.label}`}
          className="flex size-4 items-center justify-center rounded-full text-subtle transition-colors duration-120 hover:bg-hover hover:text-foreground disabled:opacity-50"
        >
          <X className="size-3" aria-hidden="true" />
        </button>
      ) : (
        <span className="pr-1" aria-hidden="true" />
      )}
    </span>
  );
}

export function SearchResults({
  search,
  searching,
  saved,
  onRemoveChip,
  onSave,
  onBack,
}: {
  search: ActiveSearch;
  searching: boolean;
  saved: boolean;
  onRemoveChip: (chip: QueryChip) => void;
  onSave: () => void;
  onBack: () => void;
}) {
  const homeDir = useAppStore((s) => s.env?.homeDir);
  const [revealError, setRevealError] = useState<string | null>(null);

  const reveal = async (path: string) => {
    setRevealError(null);
    try {
      await api.revealInExplorer(path);
    } catch (err) {
      setRevealError(err instanceof Error ? err.message : "Could not open this location in Explorer.");
    }
  };

  const capped = search.totalMatches > search.hits.length;

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="flex flex-wrap items-center gap-2 pb-2">
        <Button variant="subtle" size="sm" onClick={onBack}>
          <ArrowLeft className="size-3.5" aria-hidden="true" />
          Back to folders
        </Button>
        <span className="text-[12px] text-muted" aria-live="polite">
          {searching
            ? "Searching…"
            : capped
              ? `Showing the first ${formatCount(search.hits.length)} of ${formatCount(search.totalMatches)} matches`
              : `${formatCount(search.totalMatches)} match${search.totalMatches === 1 ? "" : "es"}`}
        </span>
        <span className="ml-auto">
          <Button variant="subtle" size="sm" onClick={onSave} disabled={saved || searching}>
            <Bookmark className="size-3.5" aria-hidden="true" />
            {saved ? "Saved" : "Save search"}
          </Button>
        </span>
      </div>

      {search.chips.length > 0 ? (
        <div className="flex flex-wrap items-center gap-1.5 pb-3">
          <span className="text-[12px] text-muted">Understood as</span>
          {search.chips.map((chip) => (
            <Chip
              key={chip.kind}
              chip={chip}
              removable={filtersWithoutChip(search.filters, chip) !== null}
              disabled={searching}
              onRemove={onRemoveChip}
            />
          ))}
        </div>
      ) : null}

      {revealError ? (
        <p role="alert" className="pb-2 text-[12px] text-critical">{revealError}</p>
      ) : null}

      {search.hits.length === 0 && !searching ? (
        <EmptyState
          icon={SearchX}
          className="flex-1"
          title="No files match"
          description="Nothing in the scanned folder matches all of those filters. Remove a filter above to widen the search, or try different words."
        />
      ) : (
        <div className="min-h-0 overflow-y-auto rounded-[10px] border border-border">
          <ul className="divide-y divide-border/60 px-3">
            {search.hits.map((hit) => (
              <li key={hit.path} className="flex items-center gap-3 py-2">
                <div className="min-w-0 flex-1">
                  <div className="flex items-center gap-2">
                    <p className="flex min-w-0 items-center gap-1.5 text-[13px] text-foreground" title={hit.path}>
                      <CategoryDot category={hit.category} />
                      <span className="truncate">{hit.name}</span>
                    </p>
                    {hit.matchedInText ? <TextMatchChip /> : null}
                  </div>
                  <p className="mono-path mt-0.5 truncate text-subtle" title={hit.path}>
                    {displayPath(hit.path, homeDir)}
                  </p>
                </div>
                <span className="hidden shrink-0 text-[12px] text-muted md:block">
                  {categoryMeta(hit.category).label}
                </span>
                <span className="shrink-0 text-[12px] tabular text-muted">
                  {formatDateShort(hit.modifiedMs)}
                </span>
                <span className="w-[92px] shrink-0 text-right text-[13px] tabular text-foreground">
                  {formatBytes(hit.bytes)}
                </span>
                <Button variant="ghost" size="sm" onClick={() => void reveal(hit.path)}>
                  <FolderOpen className="size-3.5" aria-hidden="true" />
                  Reveal
                </Button>
              </li>
            ))}
          </ul>
        </div>
      )}
    </div>
  );
}
