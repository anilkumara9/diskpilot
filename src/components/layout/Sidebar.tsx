import {
  AppWindow,
  Bot,
  Box,
  ChevronsLeft,
  ChevronsRight,
  Code2,
  Copy,
  Download,
  FolderTree,
  Gamepad2,
  Gauge,
  HeartPulse,
  History,
  Images,
  Rocket,
  Settings,
  Sparkles,
  type LucideIcon,
} from "lucide-react";
import type { ReactNode } from "react";
import { useAppStore } from "@/lib/store";
import { actionableBytes } from "@/lib/analysis";
import { isUpdateNoticeable } from "@/lib/updater";
import { driveHealthStatusLabel, healthDotSummary, worstDriveHealth } from "@/lib/driveHealth";
import { displayPath, formatBytes, formatCount, formatRelative } from "@/lib/format";
import { useNowTick } from "@/lib/hooks";
import type { PageId } from "@/lib/types";
import { cn } from "@/lib/utils";

interface NavEntry {
  id: PageId;
  label: string;
  icon: LucideIcon;
}

const TOP_ENTRIES: NavEntry[] = [
  { id: "overview", label: "Overview", icon: Gauge },
  { id: "health", label: "Health", icon: HeartPulse },
];

function analyzeEntries(assistantEnabled: boolean): NavEntry[] {
  const entries: NavEntry[] = [
    { id: "storage", label: "Storage", icon: FolderTree },
    { id: "media", label: "Media", icon: Images },
    { id: "duplicates", label: "Duplicates", icon: Copy },
    { id: "downloads", label: "Downloads", icon: Download },
  ];
  if (assistantEnabled) entries.push({ id: "assistant", label: "Assistant", icon: Bot });
  return entries;
}

const SYSTEM_ENTRIES: NavEntry[] = [
  { id: "applications", label: "Applications", icon: AppWindow },
  { id: "games", label: "Games", icon: Gamepad2 },
  { id: "startup", label: "Startup", icon: Rocket },
  { id: "developer", label: "Developer Tools", icon: Code2 },
  { id: "docker", label: "Docker", icon: Box },
];

function NavButton({
  entry,
  active,
  trailing,
  badgeDot = false,
  collapsed = false,
  onNavigate,
}: {
  entry: NavEntry;
  active: boolean;
  trailing?: ReactNode;
  /** Collapsed mode only: a dot on the icon carries the badge
      that the expanded pill would show (counts and sizes are not
      readable at 64 px; the detail lives in the tooltip). */
  badgeDot?: boolean;
  collapsed?: boolean;
  onNavigate: (page: PageId) => void;
}) {
  const Icon = entry.icon;
  return (
    <button
      type="button"
      onClick={() => onNavigate(entry.id)}
      aria-current={active ? "page" : undefined}
      aria-label={collapsed ? entry.label : undefined}
      title={collapsed ? entry.label : undefined}
      className={cn(
        "flex h-7 w-full items-center rounded-md text-[13px] transition-colors duration-120",
        collapsed ? "justify-center px-0" : "gap-2 px-2",
        active
          ? "bg-selected font-medium text-foreground"
          : "text-foreground hover:bg-hover",
      )}
    >
      <span className="relative shrink-0">
        <Icon
          className={cn("size-4", active ? "text-accent" : "text-muted")}
          strokeWidth={1.75}
          aria-hidden="true"
        />
        {collapsed && badgeDot ? (
          <span
            className="absolute -right-1 -top-1 size-1.5 rounded-full bg-accent"
            aria-hidden="true"
          />
        ) : null}
      </span>
      {collapsed ? null : (
        <>
          <span className="flex-1 truncate text-left">{entry.label}</span>
          {trailing}
        </>
      )}
    </button>
  );
}

/** The group heading; in collapsed mode it collapses to air so
    the icon groups keep a visible separation without text. */
function GroupLabel({ children, collapsed }: { children: string; collapsed: boolean }) {
  if (collapsed) return <div aria-hidden="true" className="h-4" />;
  return (
    <p className="px-2 pb-1.5 pt-5 text-[11px] font-medium leading-[14px] text-subtle">
      {children}
    </p>
  );
}

/** A muted information pill (never alarm-coloured): the Cleanup
    reclaimable size, the Applications update count. */
function NavPill({ children }: { children: ReactNode }) {
  return (
    <span className="rounded-full bg-hover px-1.5 py-px text-[11px] tabular leading-[14px] text-muted">
      {children}
    </span>
  );
}

/**
 * The footer's drive-health row: a status dot for the worst
 * drive, fed by the store's once-per-launch SMART read. It is
 * ambient chrome — when no drive reported a usable status there
 * is simply no row. Clicking goes to Health, where the full
 * per-drive cards live.
 */
function DriveHealthRow({ collapsed }: { collapsed: boolean }) {
  const driveHealth = useAppStore((s) => s.driveHealth);
  const navigate = useAppStore((s) => s.navigate);
  const worst = driveHealth ? worstDriveHealth(driveHealth) : null;
  if (!worst) return null;

  const summary = healthDotSummary(worst);
  const dotClass =
    worst.status === "critical"
      ? "bg-critical"
      : worst.status === "caution"
        ? "bg-tier-review"
        : "bg-tier-safe";

  if (collapsed) {
    return (
      <button
        type="button"
        onClick={() => navigate("health")}
        title={summary}
        aria-label={summary}
        className="mb-1 flex h-7 w-full items-center justify-center rounded-md transition-colors duration-120 hover:bg-hover"
      >
        <span className={cn("size-2 rounded-full", dotClass)} aria-hidden="true" />
      </button>
    );
  }
  return (
    <button
      type="button"
      onClick={() => navigate("health")}
      title={summary}
      aria-label={summary}
      className="mb-1 flex h-7 w-full items-center gap-2 rounded-md px-2 text-[12px] text-muted transition-colors duration-120 hover:bg-hover hover:text-foreground"
    >
      <span className={cn("size-2 shrink-0 rounded-full", dotClass)} aria-hidden="true" />
      <span className="min-w-0 flex-1 truncate text-left">
        {worst.model} · {driveHealthStatusLabel(worst.status)}
      </span>
    </button>
  );
}

function ScanStatus() {
  const scanning = useAppStore((s) => s.scanning);
  const progress = useAppStore((s) => s.progress);
  const summary = useAppStore((s) => s.summary);
  const homeDir = useAppStore((s) => s.env?.homeDir);
  const cancelScan = useAppStore((s) => s.cancelScan);
  /* The "Scanned … ago" line ages on the same 30 s tick as the
     results-age label elsewhere. */
  const now = useNowTick();

  return (
    <div className="px-2 pb-2" aria-live="polite">
      {scanning ? (
        <>
          <p className="flex items-center justify-between gap-2">
            <span className="text-[12px] font-medium text-foreground">Scanning…</span>
            {/* Cancellation lives here (visible from every page), not
                only beside the scan control on some pages. */}
            <button
              type="button"
              onClick={() => void cancelScan()}
              className="rounded px-1 text-[11px] font-medium leading-[14px] text-muted transition-colors duration-120 hover:bg-hover hover:text-foreground"
            >
              Cancel
            </button>
          </p>
          <div className="scan-indeterminate mt-1.5 h-[3px] rounded-full bg-hover" aria-hidden="true" />
          {progress?.currentPath ? (
            <p className="mono-path mt-1.5 truncate text-subtle" title={progress.currentPath}>
              {displayPath(progress.currentPath, homeDir)}
            </p>
          ) : null}
          {progress ? (
            <p className="mt-0.5 text-[11px] tabular text-subtle">
              {formatCount(progress.filesSeen)} files · {formatBytes(progress.bytesSeen)}
            </p>
          ) : null}
        </>
      ) : summary ? (
        <>
          <p className="text-[12px] text-muted">Scanned {formatRelative(summary.finishedAtMs, now)}</p>
          <p className="mono-path mt-0.5 truncate text-subtle" title={summary.root}>
            {displayPath(summary.root, homeDir)}
          </p>
        </>
      ) : (
        <p className="text-[12px] text-muted">Not scanned yet</p>
      )}
    </div>
  );
}

export function Sidebar() {
  const page = useAppStore((s) => s.page);
  const navigate = useAppStore((s) => s.navigate);
  const assistantEnabled = useAppStore((s) => s.settings.assistantEnabled);
  const collapsed = useAppStore((s) => s.settings.sidebarCollapsed);
  const updateSettings = useAppStore((s) => s.updateSettings);
  const updateNoticeable = useAppStore((s) => isUpdateNoticeable(s.update));
  const recommendations = useAppStore((s) => s.recommendations);
  const softwareUpdatesBadge = useAppStore((s) => s.softwareUpdatesBadge);
  /* The Cleanup pill is the same figure the Cleanup page itself
     headlines: total bytes across the actionable recommendations
     of the current scan. No scan (or nothing actionable) → no
     pill. It is information, not an alarm, so it stays muted. */
  const cleanupBytes = actionableBytes(recommendations);
  const updateCount = softwareUpdatesBadge?.count ?? 0;
  /* The badge tooltip's "last checked" ages on the shared tick. */
  const now = useNowTick();

  const cleanupEntries: NavEntry[] = [
    { id: "cleanup", label: "Cleanup", icon: Sparkles },
    { id: "history", label: "History", icon: History },
  ];

  return (
    <nav
      aria-label="Primary"
      className={cn(
        "flex h-full shrink-0 flex-col overflow-hidden border-r border-border bg-sidebar transition-[width] duration-200 motion-reduce:transition-none",
        collapsed ? "w-16" : "w-[220px]",
      )}
    >
      {/* 38px drag region with the logo header */}
      <div
        data-tauri-drag-region
        className={cn(
          "flex h-[38px] shrink-0 items-center gap-2",
          collapsed ? "justify-center px-0" : "px-3",
        )}
      >
        <img src="/logo.svg" alt="" width={20} height={20} className="rounded-md" aria-hidden="true" />
        {collapsed ? null : (
          <span className="text-[13px] font-semibold tracking-[-0.01em] text-foreground">DiskPilot</span>
        )}
      </div>

      <div className="min-h-0 flex-1 overflow-y-auto px-2 pb-2">
        <div className="space-y-px">
          {TOP_ENTRIES.map((entry) => (
            <NavButton
              key={entry.id}
              entry={entry}
              active={page === entry.id}
              collapsed={collapsed}
              onNavigate={navigate}
            />
          ))}
        </div>

        <GroupLabel collapsed={collapsed}>Analyze</GroupLabel>
        <div className="space-y-px">
          {analyzeEntries(assistantEnabled).map((entry) => (
            <NavButton
              key={entry.id}
              entry={entry}
              active={page === entry.id}
              collapsed={collapsed}
              onNavigate={navigate}
            />
          ))}
        </div>

        <GroupLabel collapsed={collapsed}>System</GroupLabel>
        <div className="space-y-px">
          {SYSTEM_ENTRIES.map((entry) => (
            <NavButton
              key={entry.id}
              entry={entry}
              active={page === entry.id}
              collapsed={collapsed}
              onNavigate={navigate}
              badgeDot={entry.id === "applications" && updateCount > 0}
              trailing={
                entry.id === "applications" && updateCount > 0 ? (
                  <span
                    title={`${formatCount(updateCount)} app updates found (last checked ${formatRelative(softwareUpdatesBadge!.checkedAtMs, now)})`}
                  >
                    <NavPill>{formatCount(updateCount)}</NavPill>
                  </span>
                ) : undefined
              }
            />
          ))}
        </div>

        <GroupLabel collapsed={collapsed}>Clean up</GroupLabel>
        <div className="space-y-px">
          {cleanupEntries.map((entry) => (
            <NavButton
              key={entry.id}
              entry={entry}
              active={page === entry.id}
              collapsed={collapsed}
              onNavigate={navigate}
              badgeDot={entry.id === "cleanup" && cleanupBytes > 0}
              trailing={
                entry.id === "cleanup" && cleanupBytes > 0 ? (
                  <NavPill>{formatBytes(cleanupBytes)}</NavPill>
                ) : undefined
              }
            />
          ))}
        </div>
      </div>

      <div className="shrink-0 border-t border-border px-2 pb-2 pt-3">
        {collapsed ? null : <ScanStatus />}
        <DriveHealthRow collapsed={collapsed} />
        <NavButton
          entry={{ id: "settings", label: "Settings", icon: Settings }}
          active={page === "settings"}
          collapsed={collapsed}
          onNavigate={navigate}
          badgeDot={updateNoticeable}
          trailing={
            updateNoticeable ? (
              <span className="flex shrink-0 items-center gap-1.5 text-[11px] font-medium leading-[14px] text-accent">
                <span className="size-1.5 rounded-full bg-accent" aria-hidden="true" />
                Update available
              </span>
            ) : undefined
          }
        />
        <button
          type="button"
          onClick={() => void updateSettings({ sidebarCollapsed: !collapsed })}
          aria-expanded={!collapsed}
          aria-label={collapsed ? "Expand sidebar" : "Collapse sidebar"}
          title={collapsed ? "Expand sidebar" : "Collapse sidebar"}
          className={cn(
            "mt-1 flex h-7 w-full items-center rounded-md text-[12px] text-muted transition-colors duration-120 hover:bg-hover hover:text-foreground",
            collapsed ? "justify-center px-0" : "gap-2 px-2",
          )}
        >
          {collapsed ? (
            <ChevronsRight className="size-4" aria-hidden="true" />
          ) : (
            <>
              <ChevronsLeft className="size-4 shrink-0" aria-hidden="true" />
              <span className="flex-1 truncate text-left">Collapse sidebar</span>
            </>
          )}
        </button>
      </div>
    </nav>
  );
}
