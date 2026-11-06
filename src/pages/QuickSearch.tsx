import { useEffect, useRef, useState } from "react";
import { CornerDownLeft, Search, SearchX } from "lucide-react";
import * as api from "@/lib/api";
import { categoryMeta } from "@/lib/categories";
import { displayPath, formatBytes, formatCount, formatDateShort, parentPath } from "@/lib/format";
import { filtersWithoutChip } from "@/lib/search";
import { applyTheme, watchSystemTheme } from "@/lib/theme";
import type {
  QueryChip,
  SearchFilters,
  SearchHit,
  ThemePreference,
} from "@/lib/types";
import { CategoryDot } from "@/components/shared/CategoryBadge";
import { Chip } from "@/features/storage/SearchResults";
import { cn } from "@/lib/utils";

/**
 * Quick Search: the frameless popup window behind Ctrl+Shift+Space.
 * It renders instead of the main app when the bundle is loaded with
 * `?window=quicksearch` (see main.tsx). The window is created hidden
 * by the shell and shares the app's managed state, so searches read
 * the same scan index as the Storage page.
 *
 * The panel IS the window: one raised surface, a large search field,
 * the parser's "understood as" chips, and a keyboard-first results
 * list. Enter reveals the active file in Explorer and hides the
 * window; Escape hides it and clears the query.
 */

interface QuickResults {
  chips: QueryChip[];
  filters: SearchFilters;
  hits: SearchHit[];
  totalMatches: number;
}

const EXAMPLE_QUERIES = ["pdfs from last week", "videos over 100 MB", "screenshots", "installers"];

async function hideWindow(): Promise<void> {
  try {
    const { getCurrentWindow } = await import("@tauri-apps/api/window");
    await getCurrentWindow().hide();
  } catch {
    /* harness / browser: no window to hide */
  }
}

export default function QuickSearchApp() {
  const [themePref, setThemePref] = useState<ThemePreference>("system");
  const [homeDir, setHomeDir] = useState<string | undefined>(undefined);
  const [hasScan, setHasScan] = useState<boolean | null>(null);
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<QuickResults | null>(null);
  const [searching, setSearching] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [activeIndex, setActiveIndex] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLUListElement>(null);
  const requestSeq = useRef(0);

  /* One-time setup: theme from saved settings, home dir for path
     display, and whether a scan index exists to search at all. */
  useEffect(() => {
    let live = true;
    void (async () => {
      const [settings, env, summary] = await Promise.all([
        api.getSettings(),
        api.getEnvironment(),
        api.getScanSummary(),
      ]);
      if (!live) return;
      if (settings) {
        setThemePref(settings.theme);
        applyTheme(settings.theme);
      }
      setHomeDir(env?.homeDir);
      setHasScan(summary !== null);
    })();
    return () => {
      live = false;
    };
  }, []);

  useEffect(() => watchSystemTheme(() => themePref), [themePref]);

  /* The shell shows/hides this window without reloading it: whenever
     it regains focus, put the caret back in the search field. */
  useEffect(() => {
    const refocus = () => inputRef.current?.focus();
    window.addEventListener("focus", refocus);
    return () => window.removeEventListener("focus", refocus);
  }, []);

  const runSearch = async (text: string) => {
    const trimmed = text.trim();
    if (!trimmed) {
      setResults(null);
      setError(null);
      setSearching(false);
      return;
    }
    const seq = ++requestSeq.current;
    setSearching(true);
    setError(null);
    try {
      const response = await api.searchFiles(trimmed);
      if (requestSeq.current !== seq) return;
      setResults({
        chips: response.chips,
        filters: response.filters,
        hits: response.hits,
        totalMatches: response.totalMatches,
      });
      setActiveIndex(0);
    } catch (err) {
      if (requestSeq.current !== seq) return;
      setResults(null);
      setError(err instanceof Error ? err.message : "The search could not be run.");
    } finally {
      if (requestSeq.current === seq) setSearching(false);
    }
  };

  /* Debounced search: 120 ms after the last keystroke. */
  useEffect(() => {
    if (!query.trim()) {
      requestSeq.current += 1;
      setResults(null);
      setError(null);
      setSearching(false);
      return;
    }
    const timer = window.setTimeout(() => void runSearch(query), 120);
    return () => window.clearTimeout(timer);
  }, [query]);

  const removeChip = async (chip: QueryChip) => {
    if (!results) return;
    const nextFilters = filtersWithoutChip(results.filters, chip);
    if (!nextFilters) return;
    const seq = ++requestSeq.current;
    setSearching(true);
    setError(null);
    try {
      const response = await api.runSearchFilters(nextFilters);
      if (requestSeq.current !== seq) return;
      setResults({
        filters: nextFilters,
        chips: results.chips.filter((c) => c.kind !== chip.kind),
        hits: response.hits,
        totalMatches: response.totalMatches,
      });
      setActiveIndex(0);
    } catch (err) {
      if (requestSeq.current !== seq) return;
      setError(err instanceof Error ? err.message : "The search could not be re-run.");
    } finally {
      if (requestSeq.current === seq) setSearching(false);
    }
  };

  const activate = async (hit: SearchHit) => {
    try {
      await api.revealInExplorer(hit.path);
    } catch {
      /* still hide: the shortcut toggle reopens instantly */
    }
    await hideWindow();
  };

  const dismiss = () => {
    setQuery("");
    setResults(null);
    setError(null);
    void hideWindow();
  };

  const onKeyDown = (event: React.KeyboardEvent) => {
    const hits = results?.hits ?? [];
    if (event.key === "Escape") {
      event.preventDefault();
      dismiss();
    } else if (event.key === "ArrowDown") {
      event.preventDefault();
      setActiveIndex((i) => {
        const next = Math.min(i + 1, hits.length - 1);
        return hits.length === 0 ? 0 : next;
      });
    } else if (event.key === "ArrowUp") {
      event.preventDefault();
      setActiveIndex((i) => Math.max(i - 1, 0));
    } else if (event.key === "Enter") {
      const hit = hits[activeIndex];
      if (hit) {
        event.preventDefault();
        void activate(hit);
      }
    }
  };

  /* Keep the active row in view while arrowing through results. */
  useEffect(() => {
    listRef.current
      ?.querySelector(`[data-index="${activeIndex}"]`)
      ?.scrollIntoView({ block: "nearest" });
  }, [activeIndex]);

  const hits = results?.hits ?? [];
  const capped = results !== null && results.totalMatches > results.hits.length;

  return (
    <div className="flex h-full flex-col overflow-hidden rounded-[10px] border border-border-strong bg-panel-raised shadow-[0_24px_70px_-12px_rgba(0,0,0,0.5),0_4px_16px_rgba(0,0,0,0.24)]">
      {/* Drag strip, then the search field */}
      <div data-tauri-drag-region className="h-3 w-full shrink-0" aria-hidden="true" />
      <div className="shrink-0 px-4 pb-2">
        <div className="relative">
          <Search
            className="pointer-events-none absolute left-3 top-1/2 size-5 -translate-y-1/2 text-subtle"
            aria-hidden="true"
          />
          <input
            ref={inputRef}
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={onKeyDown}
            placeholder="Search files — try “pdfs from last week over 5 MB”"
            aria-label="Quick search files"
            autoFocus
            spellCheck={false}
            className="h-11 w-full rounded-md border border-border-strong bg-panel pl-10 pr-16 text-[15px] text-foreground shadow-[inset_0_1px_2px_rgba(0,0,0,0.05)] outline-none transition-colors duration-120 placeholder:text-subtle hover:border-subtle focus:border-accent"
          />
          <kbd className="pointer-events-none absolute right-3 top-1/2 -translate-y-1/2 rounded border border-border bg-panel-raised px-1.5 py-0.5 text-[11px] text-subtle">
            esc
          </kbd>
        </div>
      </div>

      {results && results.chips.length > 0 ? (
        <div className="flex shrink-0 flex-wrap items-center gap-1.5 px-4 pb-2">
          <span className="text-[12px] text-muted">Understood as</span>
          {results.chips.map((chip) => (
            <Chip
              key={chip.kind}
              chip={chip}
              removable={filtersWithoutChip(results.filters, chip) !== null}
              disabled={searching}
              onRemove={(c) => void removeChip(c)}
            />
          ))}
        </div>
      ) : null}

      <div className="min-h-0 flex-1 overflow-y-auto px-2 pb-2">
        {hasScan === false ? (
          <div className="flex h-full flex-col items-center justify-center px-6 text-center">
            <p className="text-[13px] font-medium text-foreground">No scan to search yet</p>
            <p className="mt-1 max-w-[380px] text-[12px] leading-4 text-muted">
              Run a scan in DiskPilot first — Quick Search looks through the latest scan's index.
            </p>
          </div>
        ) : !query.trim() ? (
          <div className="px-2 pt-2">
            <p className="text-[12px] leading-4 text-muted">
              Search the latest scan in plain language. Open Quick Search from anywhere with{" "}
              <span className="font-medium text-foreground">Ctrl+Shift+Space</span>; press Enter to
              reveal a file in Explorer.
            </p>
            <div className="mt-3 flex flex-wrap gap-1.5">
              {EXAMPLE_QUERIES.map((example) => (
                <button
                  key={example}
                  type="button"
                  onClick={() => {
                    setQuery(example);
                    inputRef.current?.focus();
                  }}
                  className="rounded-full bg-panel px-2.5 py-1 text-[12px] font-medium text-foreground ring-1 ring-border transition-colors duration-120 hover:bg-hover"
                >
                  {example}
                </button>
              ))}
            </div>
          </div>
        ) : error ? (
          <p role="alert" className="px-2 pt-3 text-[12px] text-critical">{error}</p>
        ) : hits.length === 0 && !searching ? (
          <div className="flex h-full flex-col items-center justify-center px-6 text-center">
            <SearchX className="size-6 text-subtle" strokeWidth={1.5} aria-hidden="true" />
            <p className="mt-2 text-[13px] font-medium text-foreground">No files match</p>
            <p className="mt-1 max-w-[380px] text-[12px] leading-4 text-muted">
              Nothing in the scanned folder matches all of those filters. Remove a filter above to
              widen the search, or try different words.
            </p>
          </div>
        ) : (
          <ul ref={listRef} role="listbox" aria-label="Search results" className="divide-y divide-border/60">
            {hits.map((hit, index) => (
              <li key={hit.path}>
                <button
                  type="button"
                  role="option"
                  aria-selected={index === activeIndex}
                  data-index={index}
                  onMouseEnter={() => setActiveIndex(index)}
                  onClick={() => void activate(hit)}
                  className={cn(
                    "flex w-full items-center gap-3 rounded-md px-2 py-2 text-left transition-colors duration-120",
                    index === activeIndex ? "bg-selected" : "",
                  )}
                >
                  <span className="min-w-0 flex-1">
                    <span className="flex items-center gap-1.5 truncate text-[13px] text-foreground" title={hit.path}>
                      <CategoryDot category={hit.category} />
                      <span className="truncate">{hit.name}</span>
                    </span>
                    <span className="mono-path mt-0.5 block truncate text-subtle" title={hit.path}>
                      {displayPath(parentPath(hit.path), homeDir)}
                    </span>
                  </span>
                  <span className="hidden shrink-0 text-[12px] text-muted sm:block">
                    {categoryMeta(hit.category).label}
                  </span>
                  <span className="shrink-0 text-[12px] tabular text-muted">
                    {formatDateShort(hit.modifiedMs)}
                  </span>
                  <span className="w-[84px] shrink-0 text-right text-[13px] tabular text-foreground">
                    {formatBytes(hit.bytes)}
                  </span>
                  {index === activeIndex ? (
                    <CornerDownLeft className="size-3.5 shrink-0 text-subtle" aria-hidden="true" />
                  ) : null}
                </button>
              </li>
            ))}
          </ul>
        )}
      </div>

      <div className="flex shrink-0 items-center gap-3 border-t border-border px-4 py-2 text-[11px] text-subtle" aria-live="polite">
        <span>
          {searching
            ? "Searching…"
            : results
              ? capped
                ? `Showing the first ${formatCount(hits.length)} of ${formatCount(results.totalMatches)} matches`
                : `${formatCount(results.totalMatches)} match${results.totalMatches === 1 ? "" : "es"}`
              : "Quick Search"}
        </span>
        <span className="ml-auto">↑↓ to choose · Enter to reveal · Esc to close</span>
      </div>
    </div>
  );
}
