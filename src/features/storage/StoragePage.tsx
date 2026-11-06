import { useRef, useState } from "react";
import { FolderTree, Scale, Search } from "lucide-react";
import * as api from "@/lib/api";
import { useAppStore } from "@/lib/store";
import { addSavedSearch, filtersWithoutChip } from "@/lib/search";
import { tabIndexForKey } from "@/lib/tabs";
import type { QueryChip } from "@/lib/types";
import { PageHeader } from "@/components/layout/PageHeader";
import { Input } from "@/components/ui/input";
import { EmptyState } from "@/components/shared/EmptyState";
import { ResultsAge } from "@/components/shared/ResultsAge";
import { ExplorerView } from "@/features/explorer/ExplorerPage";
import { LargeFilesPage } from "@/features/large-files/LargeFilesPage";
import { SearchResults, type ActiveSearch } from "@/features/storage/SearchResults";
import { cn } from "@/lib/utils";

export type StorageTab = "folders" | "largest" | "map";

export function StorageTabs({
  tab,
  onChange,
}: {
  tab: StorageTab;
  onChange: (tab: StorageTab) => void;
}) {
  const tabs: { id: StorageTab; label: string }[] = [
    { id: "folders", label: "Folders" },
    { id: "largest", label: "Largest items" },
    { id: "map", label: "Map" },
  ];
  const selectedIndex = tabs.findIndex((t) => t.id === tab);
  const buttonRefs = useRef<(HTMLButtonElement | null)[]>([]);

  /* Tabs pattern: only the selected tab is tabbable (roving
     tabindex); Left/Right/Home/End move and select together. */
  const onKeyDown = (event: React.KeyboardEvent, index: number) => {
    const next = tabIndexForKey(index, event.key, tabs.length);
    if (next === null) return;
    event.preventDefault();
    onChange(tabs[next].id);
    buttonRefs.current[next]?.focus();
  };

  return (
    <div role="tablist" aria-label="Storage views" className="flex gap-1 rounded-md bg-panel-raised p-1">
      {tabs.map((t, index) => (
        <button
          key={t.id}
          ref={(el) => {
            buttonRefs.current[index] = el;
          }}
          role="tab"
          aria-selected={tab === t.id}
          tabIndex={index === selectedIndex ? 0 : -1}
          onClick={() => onChange(t.id)}
          onKeyDown={(e) => onKeyDown(e, index)}
          className={cn(
            "h-6 rounded-[5px] px-3 text-[12px] font-medium transition-colors duration-120",
            tab === t.id
              ? "bg-panel text-foreground shadow-[0_1px_3px_rgba(0,0,0,0.14)] ring-1 ring-border"
              : "text-muted hover:text-foreground",
          )}
        >
          {t.label}
        </button>
      ))}
    </div>
  );
}

/**
 * Storage: browse the scan folder by folder, or search it in plain
 * language. The search box parses the query with the core parser;
 * the results view shows the parser's chips so the interpretation is
 * never a surprise, and saved searches (Settings keeps the list) run
 * again with one click.
 */
export function StoragePage() {
  const summary = useAppStore((s) => s.summary);
  const scanning = useAppStore((s) => s.scanning);
  const savedSearches = useAppStore((s) => s.settings.savedSearches);
  const updateSettings = useAppStore((s) => s.updateSettings);
  const [tab, setTab] = useState<StorageTab>("folders");
  const [draft, setDraft] = useState("");
  const [search, setSearch] = useState<ActiveSearch | null>(null);
  const [searching, setSearching] = useState(false);
  const [searchError, setSearchError] = useState<string | null>(null);

  const runQuery = async (query: string) => {
    const trimmed = query.trim();
    if (!trimmed) return;
    setSearching(true);
    setSearchError(null);
    try {
      const response = await api.searchFiles(trimmed);
      setSearch({
        query: trimmed,
        chips: response.chips,
        filters: response.filters,
        hits: response.hits,
        totalMatches: response.totalMatches,
      });
    } catch (err) {
      setSearchError(err instanceof Error ? err.message : "The search could not be run.");
    } finally {
      setSearching(false);
    }
  };

  const removeChip = async (chip: QueryChip) => {
    if (!search) return;
    const nextFilters = filtersWithoutChip(search.filters, chip);
    if (!nextFilters) return;
    setSearching(true);
    setSearchError(null);
    try {
      const response = await api.runSearchFilters(nextFilters);
      setSearch({
        ...search,
        filters: nextFilters,
        chips: search.chips.filter((c) => c.kind !== chip.kind),
        hits: response.hits,
        totalMatches: response.totalMatches,
      });
    } catch (err) {
      setSearchError(err instanceof Error ? err.message : "The search could not be re-run.");
    } finally {
      setSearching(false);
    }
  };

  const saveCurrentSearch = async () => {
    if (!search) return;
    await updateSettings({ savedSearches: addSavedSearch(savedSearches, search.query) });
  };

  const isSaved = search
    ? savedSearches.some((saved) => saved.toLowerCase() === search.query.toLowerCase())
    : false;

  return (
    <div className="flex h-full flex-col px-6 py-6">
      <PageHeader
        title="Storage"
        subtitle={summary ? undefined : "Browse what the scan measured, folder by folder."}
        actions={summary ? <ResultsAge finishedAtMs={summary.finishedAtMs} /> : undefined}
      />

      <div className="pb-3">
        <form
          role="search"
          onSubmit={(e) => {
            e.preventDefault();
            void runQuery(draft);
          }}
        >
          <div className="relative">
            <Search
              className="pointer-events-none absolute left-2.5 top-1/2 size-4 -translate-y-1/2 text-subtle"
              aria-hidden="true"
            />
            <Input
              value={draft}
              onChange={(e) => setDraft(e.target.value)}
              placeholder="Search files — try “pdfs from last week over 5 MB”"
              aria-label="Search files in plain language"
              className="pl-8"
            />
          </div>
        </form>
        {savedSearches.length > 0 ? (
          <div className="flex flex-wrap items-center gap-1.5 pt-2">
            <span className="text-[12px] text-muted">Saved</span>
            {savedSearches.map((saved) => (
              <button
                key={saved}
                type="button"
                onClick={() => {
                  setDraft(saved);
                  void runQuery(saved);
                }}
                className="rounded-full bg-panel-raised px-2 py-0.5 text-[11px] leading-[14px] font-medium text-foreground ring-1 ring-border transition-colors duration-120 hover:bg-hover"
              >
                {saved}
              </button>
            ))}
          </div>
        ) : null}
        {searchError ? (
          <p role="alert" className="pt-2 text-[12px] text-critical">{searchError}</p>
        ) : null}
      </div>

      {search ? (
        <SearchResults
          search={search}
          searching={searching}
          saved={isSaved}
          onRemoveChip={(chip) => void removeChip(chip)}
          onSave={() => void saveCurrentSearch()}
          onBack={() => {
            setSearch(null);
            setSearchError(null);
          }}
        />
      ) : (
        <>
          <div className="pb-3">
            <StorageTabs tab={tab} onChange={setTab} />
          </div>

          {tab === "largest" ? (
            <div className="min-h-0 flex-1 overflow-y-auto">
              <LargeFilesPage embedded />
            </div>
          ) : !summary ? (
            <EmptyState
              icon={scanning ? Scale : FolderTree}
              className="flex-1"
              title={scanning ? "Scan in progress" : "No scan data yet"}
              description={
                scanning
                  ? "Folders appear here as the scan measures them."
                  : "Run a scan from the header. Every folder is then listed with its size, category, and cleanup guidance."
              }
            />
          ) : (
            <div className="min-h-0 flex-1">
              <ExplorerView
                rootPath={summary.root}
                embedded
                mode={tab === "map" ? "map" : "list"}
              />
            </div>
          )}
        </>
      )}
    </div>
  );
}
