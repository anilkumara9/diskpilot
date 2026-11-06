import { useCallback, useEffect, useState } from "react";
import { AlertTriangle, CheckCircle2 } from "lucide-react";
import * as api from "@/lib/api";
import { useAppStore } from "@/lib/store";
import { appJunkBanner, type CleanupBanner } from "@/lib/junk";
import { formatBytes, formatCount } from "@/lib/format";
import type { AppJunkGroup } from "@/lib/types";
import { Button } from "@/components/ui/button";
import { Section } from "@/components/ui/section";

/**
 * App caches: the cache locations of known applications,
 * measured by the shell where each app keeps them. Cleaning
 * moves the caches' CONTENTS to the Recycle Bin — recoverable
 * until the Bin is emptied — and the outcome banner follows the
 * shared 3-state rule (a clean where some locations failed never
 * reads as a plain success). The section hides when no app
 * caches are found, but a failed load is shown with a Retry —
 * never mistaken for "nothing to clean".
 */
export function AppJunkSection() {
  const finishedAtMs = useAppStore((s) => s.summary?.finishedAtMs);
  const [groups, setGroups] = useState<AppJunkGroup[] | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [reloadKey, setReloadKey] = useState(0);
  const [busyIds, setBusyIds] = useState<ReadonlySet<string>>(new Set());
  const [banner, setBanner] = useState<CleanupBanner | null>(null);

  const load = useCallback(async () => {
    const rows = await api.listAppJunk();
    setGroups(rows);
    setLoadError(null);
  }, []);

  /* Junk derives from the live system, not the scan, but a scan
     finishing is still a natural refresh point, as on the other
     Cleanup sections. */
  useEffect(() => {
    let live = true;
    load().catch((err: unknown) => {
      if (live) {
        setLoadError(err instanceof Error ? err.message : "App caches could not be loaded.");
      }
    });
    return () => {
      live = false;
    };
  }, [load, finishedAtMs, reloadKey]);

  const clean = async (ids: string[]) => {
    setBusyIds(new Set(ids));
    setBanner(null);
    try {
      const result = await api.cleanAppJunk(ids);
      setBanner(appJunkBanner(result));
    } catch (err) {
      setBanner({
        kind: "error",
        message: err instanceof Error ? err.message : "The app caches could not be cleaned.",
      });
    } finally {
      setBusyIds(new Set());
      /* Re-measure either way: what was cleaned is gone, what
         failed is still there to see. */
      load().catch(() => undefined);
    }
  };

  if (loadError) {
    return (
      <Section
        title="App caches"
        description="Caches that apps rebuild on their own, measured where each app keeps them."
      >
        <div className="flex items-center gap-3 rounded-[10px] border border-border bg-panel-raised px-4 py-3">
          <p role="alert" className="flex-1 text-[12px] leading-4 text-critical">{loadError}</p>
          <Button variant="secondary" size="sm" onClick={() => setReloadKey((n) => n + 1)}>
            Retry
          </Button>
        </div>
      </Section>
    );
  }

  /* Nothing loaded yet, or nothing found and nothing to report:
     the section is simply not there. */
  if (groups === null || (groups.length === 0 && banner === null)) return null;

  const busy = busyIds.size > 0;
  const allIds = groups.map((g) => g.id);

  return (
    <Section
      title="App caches"
      description="Caches that apps rebuild on their own — browsers, chat apps, developer tools, and more — measured where each app keeps them."
      action={
        groups.length > 1 ? (
          <Button
            variant="secondary"
            size="sm"
            disabled={busy}
            onClick={() => void clean(allIds)}
          >
            {busy && busyIds.size > 1 ? "Cleaning…" : "Clean all"}
          </Button>
        ) : undefined
      }
    >
      {banner ? (
        <div className="mb-3 rounded-[10px] border border-border bg-panel-raised px-4 py-3">
          <p
            role={banner.kind === "error" ? "alert" : "status"}
            className="flex items-center gap-2 text-[12px] leading-4 text-foreground"
          >
            {banner.kind === "success" ? (
              <CheckCircle2 className="size-4 shrink-0 text-tier-safe" aria-hidden="true" />
            ) : (
              <AlertTriangle
                className={
                  banner.kind === "error"
                    ? "size-4 shrink-0 text-critical"
                    : "size-4 shrink-0 text-tier-review"
                }
                aria-hidden="true"
              />
            )}
            {banner.message}
          </p>
        </div>
      ) : null}

      {groups.length > 0 ? (
        <>
          <ul className="divide-y divide-border rounded-[10px] border border-border px-3">
            {groups.map((group) => {
              const rowBusy = busyIds.has(group.id);
              return (
                <li key={group.id} className="flex items-center gap-3 py-2">
                  <div className="min-w-0 flex-1">
                    <p className="truncate text-[13px] text-foreground">
                      <span className="font-medium">{group.app}</span>
                      <span className="text-muted"> · {group.label}</span>
                    </p>
                    <p className="mt-0.5 text-[11px] leading-[14px] text-muted">{group.note}</p>
                    <p className="mt-0.5 text-[11px] text-subtle">
                      {formatCount(group.fileCount)} files · {group.paths.length}{" "}
                      location{group.paths.length === 1 ? "" : "s"}
                    </p>
                  </div>
                  <span className="w-[92px] shrink-0 text-right text-[13px] tabular text-foreground">
                    {formatBytes(group.bytes)}
                  </span>
                  <Button
                    variant="secondary"
                    size="sm"
                    disabled={busy}
                    onClick={() => void clean([group.id])}
                  >
                    {rowBusy ? "Cleaning…" : "Clean"}
                  </Button>
                </li>
              );
            })}
          </ul>
          <p className="mt-2 text-[12px] leading-4 text-muted">
            Cleaning sends cache contents to the Recycle Bin, so anything cleaned here can be
            restored until you empty it. Apps rebuild their caches as needed.
          </p>
        </>
      ) : null}
    </Section>
  );
}
