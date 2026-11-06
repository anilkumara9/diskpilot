import { useCallback, useEffect, useState } from "react";
import { Info, Rocket } from "lucide-react";
import { useAppStore } from "@/lib/store";
import * as api from "@/lib/api";
import { displayPath, formatBytes, formatCount } from "@/lib/format";
import type { StartupItem, StartupSource } from "@/lib/types";
import { PageHeader } from "@/components/layout/PageHeader";
import { Button } from "@/components/ui/button";
import { Section } from "@/components/ui/section";
import { Switch } from "@/components/ui/switch";
import { SkeletonRows } from "@/components/ui/skeleton";
import { EmptyState } from "@/components/shared/EmptyState";
import { MarkerTag } from "@/components/shared/MarkerTag";
import { StatStrip } from "@/components/shared/StatStrip";

/* ---------------- Groups ---------------- */

/** Fixed group order with the names a Windows user recognises.
    Groups with no entries are omitted entirely. */
const SOURCE_GROUPS: { source: StartupSource; label: string }[] = [
  { source: "registryUser", label: "Registry — current user" },
  { source: "registryMachine", label: "Registry — all users" },
  { source: "folderUser", label: "Startup folder — current user" },
  { source: "folderMachine", label: "Startup folder — all users" },
];

/* ---------------- Row ---------------- */

function StartupRow({
  item,
  busy,
  onToggle,
}: {
  item: StartupItem;
  busy: boolean;
  onToggle: (item: StartupItem, enabled: boolean) => void;
}) {
  const homeDir = useAppStore((s) => s.env?.homeDir);
  return (
    <li className="flex items-center gap-3 py-2">
      <div className="min-w-0 flex-1">
        <p className="file-name truncate text-[13px] text-foreground" title={item.name}>
          {item.name}
        </p>
        <p className="mono-path mt-0.5 truncate text-subtle" title={item.command}>
          {displayPath(item.command, homeDir)}
        </p>
        {!item.targetExists ? (
          <p className="mt-1">
            <MarkerTag tone="warn">Target not found</MarkerTag>
          </p>
        ) : null}
      </div>
      {item.targetBytes !== null ? (
        <span className="w-[92px] shrink-0 text-right text-[13px] tabular text-foreground">
          {formatBytes(item.targetBytes)}
        </span>
      ) : null}
      <Switch
        checked={item.enabled}
        disabled={busy}
        onChange={(enabled) => onToggle(item, enabled)}
        ariaLabel={`${item.enabled ? "Disable" : "Enable"} ${item.name} at sign-in`}
      />
    </li>
  );
}

/* ---------------- Page ---------------- */

export function StartupPage() {
  const [items, setItems] = useState<StartupItem[] | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [toggleError, setToggleError] = useState<string | null>(null);
  const [busyIds, setBusyIds] = useState<ReadonlySet<string>>(new Set());
  const [reloadKey, setReloadKey] = useState(0);

  /* Startup entries come from the registry and the Startup
     folders, not from a scan: loaded on mount and re-read after
     every toggle so the page always shows what Windows holds.
     A failed read is an error state — never the clean-PC empty
     state. */
  const load = useCallback(async () => {
    const rows = await api.getStartupItems();
    setItems(rows);
    setLoadError(null);
    return rows;
  }, []);

  useEffect(() => {
    let live = true;
    load().catch((err: unknown) => {
      if (live) {
        setLoadError(
          err instanceof Error ? err.message : "Startup entries could not be loaded.",
        );
      }
    });
    return () => {
      live = false;
    };
  }, [load, reloadKey]);

  const toggle = async (item: StartupItem, enabled: boolean) => {
    setToggleError(null);
    setBusyIds((prev) => new Set(prev).add(item.id));
    /* Optimistic: the switch moves at once. A failure reverts it
       and reports the shell's message — the entry never looks
       changed when Windows did not change it. */
    setItems((prev) =>
      prev ? prev.map((row) => (row.id === item.id ? { ...row, enabled } : row)) : prev,
    );
    try {
      await api.setStartupItemEnabled(item.id, enabled);
      await load();
    } catch (err) {
      setItems((prev) =>
        prev
          ? prev.map((row) => (row.id === item.id ? { ...row, enabled: item.enabled } : row))
          : prev,
      );
      setToggleError(
        err instanceof Error ? err.message : "The startup entry could not be changed.",
      );
    } finally {
      setBusyIds((prev) => {
        const next = new Set(prev);
        next.delete(item.id);
        return next;
      });
    }
  };

  const enabledCount = items?.filter((i) => i.enabled).length ?? 0;

  return (
    <div className="h-full overflow-y-auto px-6 py-6">
      <div className="max-w-[880px]">
        <PageHeader
          title="Startup"
          subtitle="Programs that open when you sign in to Windows"
          showScanControl={false}
        />
        {toggleError ? (
          <p role="alert" className="pb-3 text-[12px] text-critical">{toggleError}</p>
        ) : null}

        {loadError ? (
          <EmptyState
            icon={Rocket}
            className="min-h-[360px]"
            title="Startup entries couldn't be loaded"
            description={loadError}
            action={
              <Button variant="secondary" onClick={() => setReloadKey((n) => n + 1)}>
                Retry
              </Button>
            }
          />
        ) : items === null ? (
          <SkeletonRows rows={7} />
        ) : items.length === 0 ? (
          <EmptyState
            icon={Rocket}
            className="min-h-[360px]"
            title="Nothing opens at sign-in"
            description="Windows starts clean on this PC."
          />
        ) : (
          <>
            <StatStrip
              stats={[
                { label: "Entries", value: formatCount(items.length) },
                { label: "Enabled", value: formatCount(enabledCount) },
                { label: "Disabled", value: formatCount(items.length - enabledCount) },
              ]}
            />

            {SOURCE_GROUPS.map(({ source, label }) => {
              const group = items.filter((i) => i.source === source);
              if (group.length === 0) return null;
              return (
                <Section key={source} title={label}>
                  <ul className="divide-y divide-border rounded-[10px] border border-border px-3">
                    {group.map((item) => (
                      <StartupRow
                        key={item.id}
                        item={item}
                        busy={busyIds.has(item.id)}
                        onToggle={(row, enabled) => void toggle(row, enabled)}
                      />
                    ))}
                  </ul>
                </Section>
              );
            })}

            <div className="mt-6 flex gap-3 rounded-[10px] border border-border bg-panel-raised px-4 py-3">
              <Info className="mt-0.5 size-4 shrink-0 text-muted" aria-hidden="true" />
              <p className="text-[12px] leading-4 text-muted">
                Disabling is reversible — DiskPilot keeps the entry and restores it exactly when
                you re-enable it. Machine-wide entries need DiskPilot running as administrator.
              </p>
            </div>
          </>
        )}
      </div>
    </div>
  );
}
