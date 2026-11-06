import { useMemo, useState } from "react";
import { FileWarning, FolderOpen, Scale, Search } from "lucide-react";
import * as api from "@/lib/api";
import { useAppStore } from "@/lib/store";
import { sortFilesBySize, sortLocations } from "@/lib/analysis";
import { displayPath, formatBytes } from "@/lib/format";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Section } from "@/components/ui/section";
import { CategoryBadge } from "@/components/shared/CategoryBadge";
import { CategorySelect, type CategoryFilter } from "@/components/shared/CategorySelect";
import { EmptyState } from "@/components/shared/EmptyState";

export function LargeFilesPage({ embedded = false }: { embedded?: boolean }) {
  const summary = useAppStore((s) => s.summary);
  const homeDir = useAppStore((s) => s.env?.homeDir);
  const [query, setQuery] = useState("");
  const [categoryFilter, setCategoryFilter] = useState<CategoryFilter>("all");
  const [revealError, setRevealError] = useState<string | null>(null);

  const files = useMemo(() => {
    if (!summary) return [];
    let list = sortFilesBySize(summary.topFiles).slice(0, 500);
    const q = query.trim().toLowerCase();
    if (q) list = list.filter((f) => f.name.toLowerCase().includes(q) || f.path.toLowerCase().includes(q));
    if (categoryFilter !== "all") list = list.filter((f) => f.category === categoryFilter);
    return list;
  }, [summary, query, categoryFilter]);

  const heavy = useMemo(() => (summary ? sortLocations(summary.heavyFolders).slice(0, 12) : []), [summary]);

  const reveal = async (path: string) => {
    setRevealError(null);
    try {
      await api.revealInExplorer(path);
    } catch (err) {
      setRevealError(err instanceof Error ? err.message : "Could not open this location in Explorer.");
    }
  };

  if (!summary) {
    return (
      <EmptyState
        icon={Scale}
        title="No largest items yet"
        description="After a scan, the 500 largest files and the heaviest folders are listed here."
      />
    );
  }

  const body = (
    <div>
      <div className="flex items-start gap-2 rounded-[10px] border border-border bg-panel-raised px-3.5 py-2.5">
        <FileWarning className="mt-0.5 size-4 shrink-0 text-tier-review" aria-hidden="true" />
        <p className="text-[12px] leading-4 text-muted">
          Size alone is never a reason to delete. Large files are often the ones you care about most —
          check what a file is and where it lives before removing anything.
        </p>
      </div>

      <div className="mt-4 flex items-center gap-2">
        <div className="relative flex-1">
          <Search className="pointer-events-none absolute left-2.5 top-1/2 size-4 -translate-y-1/2 text-subtle" aria-hidden="true" />
          <Input value={query} onChange={(e) => setQuery(e.target.value)} placeholder="Filter largest files" aria-label="Filter largest files" className="pl-8" />
        </div>
        <CategorySelect value={categoryFilter} onChange={setCategoryFilter} />
      </div>

      {revealError ? <p role="alert" className="mt-2 text-[12px] text-critical">{revealError}</p> : null}

      <div className="mt-3 overflow-hidden rounded-[10px] border border-border">
        <table className="w-full border-collapse">
          <thead>
            <tr className="border-b border-border text-left">
              <th scope="col" className="px-3 py-2 text-[12px] font-medium text-muted">Name</th>
              <th scope="col" className="px-3 py-2 text-[12px] font-medium text-muted">Path</th>
              <th scope="col" className="px-3 py-2 text-[12px] font-medium text-muted">Category</th>
              <th scope="col" className="px-3 py-2 text-right text-[12px] font-medium text-muted">Size</th>
              <th scope="col" className="px-3 py-2"><span className="sr-only">Actions</span></th>
            </tr>
          </thead>
          <tbody>
            {files.map((file) => (
              <tr key={file.path} className="h-8 border-b border-border/60 transition-colors duration-120 hover:bg-hover">
                <td className="max-w-[220px] truncate px-3 py-1 text-[13px] text-foreground" title={file.name}>{file.name}</td>
                <td className="mono-path max-w-[320px] truncate px-3 py-1 text-subtle" title={file.path}>{displayPath(file.path, homeDir)}</td>
                <td className="px-3 py-1"><CategoryBadge category={file.category} /></td>
                <td className="px-3 py-1 text-right text-[13px] tabular text-foreground">{formatBytes(file.bytes)}</td>
                <td className="px-3 py-1 text-right">
                  <Button variant="ghost" size="sm" onClick={() => void reveal(file.path)}>
                    <FolderOpen className="size-3.5" aria-hidden="true" />
                    Reveal
                  </Button>
                </td>
              </tr>
            ))}
            {files.length === 0 ? (
              <tr><td colSpan={5} className="px-3 py-8 text-center text-[12px] text-muted">No files match the current filter.</td></tr>
            ) : null}
          </tbody>
        </table>
      </div>
      <p className="mt-2 text-[12px] text-muted">Showing {files.length} of the 500 largest files from the latest scan.</p>

      {heavy.length > 0 ? (
        <Section title="Heavy folders" description="Folders taking the most space, including everything inside them.">
          <ul className="divide-y divide-border">
            {heavy.map((folder) => (
              <li key={folder.path} className="flex items-center gap-3 py-2">
                <div className="min-w-0 flex-1">
                  <p className="truncate text-[13px] text-foreground">{folder.name}</p>
                  <p className="mono-path truncate text-subtle">{displayPath(folder.path, homeDir)}</p>
                </div>
                <CategoryBadge category={folder.category} className="hidden lg:flex" />
                <span className="w-[92px] text-right text-[13px] tabular text-foreground">{formatBytes(folder.bytes)}</span>
                <Button variant="ghost" size="sm" onClick={() => void reveal(folder.path)}>Reveal</Button>
              </li>
            ))}
          </ul>
        </Section>
      ) : null}
    </div>
  );

  if (embedded) return body;
  return <div className="mx-auto max-w-[880px] overflow-y-auto px-6 py-6">{body}</div>;
}
