import { useEffect, useState } from "react";
import { Images } from "lucide-react";
import { useAppStore } from "@/lib/store";
import * as api from "@/lib/api";
import { displayPath, formatBytes, formatCount, formatDateShort, parentPath } from "@/lib/format";
import type { MediaItem, MediaSummary, SimilarGroup } from "@/lib/types";
import { PageHeader } from "@/components/layout/PageHeader";
import { ScanControl } from "@/components/layout/ScanControl";
import { Button } from "@/components/ui/button";
import { Section } from "@/components/ui/section";
import { SkeletonRows } from "@/components/ui/skeleton";
import { EmptyState } from "@/components/shared/EmptyState";

/* ---------------- Figures ---------------- */

function MediaFigure({ label, bytes, sub }: { label: string; bytes: number; sub: string }) {
  return (
    <div className="min-w-[140px]">
      <p className="text-[12px] text-muted">{label}</p>
      <p className="mt-0.5 text-[20px] font-semibold leading-6 tabular text-foreground">
        {formatBytes(bytes)}
      </p>
      <p className="mt-0.5 text-[12px] text-muted">{sub}</p>
    </div>
  );
}

/* ---------------- Biggest lists ---------------- */

/** One biggest-media list: name over its folder and date, size
    right-aligned. A row click reveals the file in Explorer — the
    same review gesture the Duplicates page teaches. */
function BiggestList({
  items,
  homeDir,
  onReveal,
}: {
  items: MediaItem[];
  homeDir?: string;
  onReveal: (path: string) => void;
}) {
  if (items.length === 0) {
    return <p className="text-[12px] text-muted">None found in this scan.</p>;
  }
  return (
    <ul className="divide-y divide-border">
      {items.map((item) => (
        <li key={item.path}>
          <button
            type="button"
            onClick={() => onReveal(item.path)}
            title={`Reveal ${item.name} in Explorer`}
            className="flex w-full items-center gap-3 py-2 text-left transition-colors duration-120 hover:bg-hover"
          >
            <span className="min-w-0 flex-1">
              <span className="block truncate text-[13px] text-foreground" title={item.path}>
                {item.name}
              </span>
              <span className="mt-0.5 flex items-baseline gap-1.5 text-[11px] text-muted">
                <span className="mono-path min-w-0 truncate" title={item.path}>
                  {displayPath(parentPath(item.path), homeDir)}
                </span>
                <span aria-hidden="true">·</span>
                <span className="shrink-0">{formatDateShort(item.modifiedMs)}</span>
              </span>
            </span>
            <span className="w-[92px] shrink-0 text-right text-[13px] tabular text-foreground">
              {formatBytes(item.bytes)}
            </span>
          </button>
        </li>
      ))}
    </ul>
  );
}

/* ---------------- Page ---------------- */

export function MediaPage() {
  const summary = useAppStore((s) => s.summary);
  const scanning = useAppStore((s) => s.scanning);
  const homeDir = useAppStore((s) => s.env?.homeDir);
  const navigate = useAppStore((s) => s.navigate);
  const setDuplicatesMode = useAppStore((s) => s.setDuplicatesMode);

  const [media, setMedia] = useState<MediaSummary | null>(null);
  const [similar, setSimilar] = useState<SimilarGroup[] | null>(null);
  const [revealError, setRevealError] = useState<string | null>(null);

  /* Media totals and look-alike groups both derive from the scan
     index, so they refresh with the scan that produced it. */
  const finishedAtMs = summary?.finishedAtMs;
  useEffect(() => {
    if (!summary) {
      setMedia(null);
      setSimilar(null);
      return;
    }
    let live = true;
    void (async () => {
      const [m, s] = await Promise.all([api.getMediaSummary(), api.getSimilarImages()]);
      if (!live) return;
      setMedia(m);
      setSimilar(s);
    })();
    return () => {
      live = false;
    };
  }, [summary, finishedAtMs]);

  const reveal = async (path: string) => {
    setRevealError(null);
    try {
      await api.revealInExplorer(path);
    } catch (err) {
      setRevealError(err instanceof Error ? err.message : "Could not open this location in Explorer.");
    }
  };

  const reviewLookalikes = () => {
    setDuplicatesMode("similar");
    navigate("duplicates");
  };

  const similarWasted = (similar ?? []).reduce((acc, g) => acc + g.wastedBytes, 0);

  return (
    <div className="h-full overflow-y-auto px-6 py-6">
      <div className="max-w-[880px]">
        <PageHeader
          title="Media"
          subtitle="Photos and videos in the scanned folder — what each kind costs you, and the biggest files of each."
        />
        {revealError ? (
          <p role="alert" className="pb-3 text-[12px] text-critical">{revealError}</p>
        ) : null}

        {!summary && !scanning ? (
          <EmptyState
            icon={Images}
            className="min-h-[360px]"
            title="No media measured yet"
            description="Photos and videos appear here after a scan — the totals for each kind, and the biggest files worth reviewing first."
            action={<ScanControl compact />}
          />
        ) : media === null ? (
          <SkeletonRows rows={7} />
        ) : (
          <>
            <div className="flex flex-wrap gap-x-10 gap-y-4">
              <MediaFigure
                label="Photos"
                bytes={media.photos.bytes}
                sub={`${formatCount(media.photos.count)} files`}
              />
              <MediaFigure
                label="Videos"
                bytes={media.videos.bytes}
                sub={`${formatCount(media.videos.count)} files`}
              />
              <MediaFigure
                label="Screenshots"
                bytes={media.screenshots.bytes}
                sub={`${formatCount(media.screenshots.count)} files · included in Photos`}
              />
            </div>

            <Section title="Biggest videos" description="The largest video files in this scan, biggest first.">
              <BiggestList items={media.biggestVideos} homeDir={homeDir} onReveal={(p) => void reveal(p)} />
            </Section>

            <Section title="Biggest photos" description="The largest photos in this scan, biggest first.">
              <BiggestList items={media.biggestPhotos} homeDir={homeDir} onReveal={(p) => void reveal(p)} />
            </Section>

            {similar !== null && similar.length > 0 ? (
              <div className="mt-6 flex items-center gap-3 rounded-[10px] border border-border bg-panel-raised px-4 py-3">
                <Images className="size-4 shrink-0 text-accent" aria-hidden="true" />
                <p className="min-w-0 flex-1 text-[12px] leading-4 text-muted">
                  <span className="font-medium text-foreground">
                    {similar.length} look-alike photo group{similar.length === 1 ? "" : "s"}
                  </span>{" "}
                  — up to {formatBytes(similarWasted)} in extra copies. Look-alikes are reviewed
                  by eye, never removed automatically.
                </p>
                <Button variant="secondary" size="sm" onClick={reviewLookalikes}>
                  Review in Duplicates
                </Button>
              </div>
            ) : null}
          </>
        )}
      </div>
    </div>
  );
}
