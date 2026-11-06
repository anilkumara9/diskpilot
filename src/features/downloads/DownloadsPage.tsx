import { useEffect, useState } from "react";
import { ChevronDown, ChevronRight, Download, FolderOpen } from "lucide-react";
import * as api from "@/lib/api";
import { useAppStore } from "@/lib/store";
import { displayPath, formatBytes, formatDateShort, formatRelative } from "@/lib/format";
import type { ProvenanceGroup, RecentFile } from "@/lib/types";
import { PageHeader } from "@/components/layout/PageHeader";
import { Button } from "@/components/ui/button";
import { Section } from "@/components/ui/section";
import { SkeletonRows } from "@/components/ui/skeleton";
import { EmptyState } from "@/components/shared/EmptyState";
import { MarkerTag } from "@/components/shared/MarkerTag";
import { cn } from "@/lib/utils";

/**
 * The Downloads folder, two ways: newest first, and grouped by the
 * source Windows recorded for each file (Zone.Identifier zone data —
 * written by browsers and some apps at download time). Provenance is
 * only ever what the zone data says; a file without it shows no
 * source rather than a guessed one.
 */

function RevealButton({ path, onReveal }: { path: string; onReveal: (path: string) => void }) {
  return (
    <Button variant="ghost" size="sm" onClick={() => onReveal(path)}>
      <FolderOpen className="size-3.5" aria-hidden="true" />
      Reveal
    </Button>
  );
}

function RecentList({
  files,
  onReveal,
}: {
  files: RecentFile[];
  onReveal: (path: string) => void;
}) {
  return (
    <ul className="divide-y divide-border rounded-[10px] border border-border px-3">
      {files.map((file) => (
        <li key={file.path} className="flex items-center gap-3 py-2">
          <div className="min-w-0 flex-1">
            <p className="truncate text-[13px] text-foreground" title={file.path}>
              {file.name}
            </p>
            <p className="mt-0.5 flex flex-wrap items-center gap-1.5 text-[11px] text-muted">
              <span>{formatRelative(file.modifiedMs)}</span>
              {file.source ? <MarkerTag>{file.source.label}</MarkerTag> : null}
            </p>
          </div>
          <span className="shrink-0 text-[13px] tabular text-foreground">
            {formatBytes(file.bytes)}
          </span>
          <RevealButton path={file.path} onReveal={onReveal} />
        </li>
      ))}
    </ul>
  );
}

function SourceGroup({
  group,
  homeDir,
  onReveal,
}: {
  group: ProvenanceGroup;
  homeDir?: string;
  onReveal: (path: string) => void;
}) {
  const [expanded, setExpanded] = useState(false);
  const Icon = expanded ? ChevronDown : ChevronRight;
  return (
    <div className="rounded-[10px] border border-border">
      <button
        type="button"
        onClick={() => setExpanded((v) => !v)}
        aria-expanded={expanded}
        className="flex w-full items-center gap-2 px-3 py-2.5 text-left transition-colors duration-120 hover:bg-hover"
      >
        <Icon className="size-4 shrink-0 text-subtle" aria-hidden="true" />
        <span className="min-w-0 flex-1">
          <span className="block truncate text-[13px] font-medium text-foreground">{group.label}</span>
          {group.host ? (
            <span className="mono-path block truncate text-subtle">{group.host}</span>
          ) : null}
        </span>
        <span className="shrink-0 text-[12px] tabular text-muted">
          {group.files.length} file{group.files.length === 1 ? "" : "s"} · {formatBytes(group.totalBytes)}
        </span>
      </button>
      {expanded ? (
        <ul className="animate-fade-in divide-y divide-border/60 border-t border-border px-3">
          {group.files.map((file) => (
            <li key={file.path} className="flex items-center gap-3 py-2">
              <div className="min-w-0 flex-1">
                <p className="mono-path truncate text-foreground" title={file.path}>
                  {displayPath(file.path, homeDir)}
                </p>
                <p className="mt-0.5 text-[11px] text-muted">{formatDateShort(file.modifiedMs)}</p>
              </div>
              <span className="shrink-0 text-[13px] tabular text-foreground">
                {formatBytes(file.bytes)}
              </span>
              <RevealButton path={file.path} onReveal={onReveal} />
            </li>
          ))}
        </ul>
      ) : null}
    </div>
  );
}

export function DownloadsPage() {
  const homeDir = useAppStore((s) => s.env?.homeDir);
  const [recent, setRecent] = useState<RecentFile[] | null>(null);
  const [groups, setGroups] = useState<ProvenanceGroup[] | null>(null);
  const [revealError, setRevealError] = useState<string | null>(null);

  useEffect(() => {
    let live = true;
    void Promise.all([api.getRecentDownloads(), api.getDownloadProvenance()]).then(
      ([recentRows, groupsRows]) => {
        if (!live) return;
        setRecent(recentRows);
        setGroups(groupsRows);
      },
    );
    return () => {
      live = false;
    };
  }, []);

  const reveal = async (path: string) => {
    setRevealError(null);
    try {
      await api.revealInExplorer(path);
    } catch (err) {
      setRevealError(err instanceof Error ? err.message : "Could not open this location in Explorer.");
    }
  };

  const loading = recent === null || groups === null;
  const nothingFound = !loading && recent.length === 0 && groups.length === 0;

  return (
    <div className="h-full overflow-y-auto px-6 py-6">
      <div className="max-w-[880px]">
        <PageHeader
          title="Downloads"
          subtitle="What landed in your Downloads folder, newest first — and, where Windows recorded it, where each file came from."
          showScanControl={false}
        />
        {revealError ? (
          <p role="alert" className="pb-3 text-[12px] text-critical">{revealError}</p>
        ) : null}

        {loading ? (
          <SkeletonRows rows={6} />
        ) : nothingFound ? (
          <EmptyState
            icon={Download}
            className="min-h-[360px]"
            title="Nothing in Downloads"
            description="Recent downloads and the sources Windows recorded for them appear here. If the Downloads folder is empty — or your browser saves downloads somewhere else — there is nothing to show yet."
          />
        ) : (
          <>
            <Section
              title="Recent"
              description={
                recent.length >= 25
                  ? "The 25 newest files in the Downloads folder."
                  : "The newest files in the Downloads folder."
              }
            >
              {recent.length > 0 ? (
                <RecentList files={recent} onReveal={(p) => void reveal(p)} />
              ) : (
                <p className="text-[12px] text-muted">No files are in the Downloads folder.</p>
              )}
            </Section>

            <Section
              title="By source"
              description="Grouped by the source recorded in each file's zone data when it was downloaded."
            >
              {groups.length > 0 ? (
                <div className="space-y-2">
                  {groups.map((group) => (
                    <SourceGroup
                      key={group.label}
                      group={group}
                      homeDir={homeDir}
                      onReveal={(p) => void reveal(p)}
                    />
                  ))}
                </div>
              ) : (
                <div
                  className={cn(
                    "rounded-[10px] border border-border bg-panel-raised px-4 py-3",
                    "text-[12px] leading-4 text-muted",
                  )}
                >
                  No source information was recorded for these downloads. Browsers and some
                  apps note where a file came from as it is saved; files copied from another
                  folder or drive carry no source, and DiskPilot never guesses one.
                </div>
              )}
            </Section>
          </>
        )}
      </div>
    </div>
  );
}
