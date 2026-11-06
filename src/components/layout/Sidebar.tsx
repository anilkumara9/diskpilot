import {
  AppWindow,
  Bot,
  Box,
  Code2,
  Copy,
  Download,
  FolderTree,
  Gamepad2,
  Gauge,
  HeartPulse,
  History,
  Images,
  Settings,
  Sparkles,
  type LucideIcon,
} from "lucide-react";
import { useAppStore } from "@/lib/store";
import { actionableCount } from "@/lib/analysis";
import { displayPath, formatBytes, formatCount, formatRelative } from "@/lib/format";
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
  { id: "developer", label: "Developer Tools", icon: Code2 },
  { id: "docker", label: "Docker", icon: Box },
];

function NavButton({
  entry,
  active,
  trailing,
  onNavigate,
}: {
  entry: NavEntry;
  active: boolean;
  trailing?: ReactNode;
  onNavigate: (page: PageId) => void;
}) {
  const Icon = entry.icon;
  return (
    <button
      type="button"
      onClick={() => onNavigate(entry.id)}
      aria-current={active ? "page" : undefined}
      className={cn(
        "flex h-7 w-full items-center gap-2 rounded-md px-2 text-[13px] transition-colors duration-120",
        active
          ? "bg-selected font-medium text-foreground"
          : "text-foreground hover:bg-hover",
      )}
    >
      <Icon
        className={cn("size-4 shrink-0", active ? "text-accent" : "text-muted")}
        strokeWidth={1.75}
        aria-hidden="true"
      />
      <span className="flex-1 truncate text-left">{entry.label}</span>
      {trailing}
    </button>
  );
}

import type { ReactNode } from "react";

function ScanStatus() {
  const scanning = useAppStore((s) => s.scanning);
  const progress = useAppStore((s) => s.progress);
  const summary = useAppStore((s) => s.summary);
  const homeDir = useAppStore((s) => s.env?.homeDir);

  return (
    <div className="px-2 pb-2" aria-live="polite">
      {scanning ? (
        <>
          <p className="text-[12px] font-medium text-foreground">Scanning…</p>
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
          <p className="text-[12px] text-muted">Scanned {formatRelative(summary.finishedAtMs)}</p>
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
  const recommendations = useAppStore((s) => s.recommendations);
  const cleanupCount = actionableCount(recommendations);

  const cleanupEntries: NavEntry[] = [
    { id: "cleanup", label: "Cleanup", icon: Sparkles },
    { id: "history", label: "History", icon: History },
  ];

  return (
    <nav
      aria-label="Primary"
      className="flex h-full w-[220px] shrink-0 flex-col border-r border-border bg-sidebar"
    >
      {/* 38px drag region with the logo header */}
      <div
        data-tauri-drag-region
        className="flex h-[38px] shrink-0 items-center gap-2 px-3"
      >
        <img src="/logo.svg" alt="" width={20} height={20} className="rounded-md" aria-hidden="true" />
        <span className="text-[13px] font-semibold tracking-[-0.01em] text-foreground">DiskPilot</span>
      </div>

      <div className="min-h-0 flex-1 overflow-y-auto px-2 pb-2">
        <div className="space-y-px">
          {TOP_ENTRIES.map((entry) => (
            <NavButton key={entry.id} entry={entry} active={page === entry.id} onNavigate={navigate} />
          ))}
        </div>

        <p className="px-2 pb-1.5 pt-5 text-[11px] font-medium leading-[14px] text-subtle">Analyze</p>
        <div className="space-y-px">
          {analyzeEntries(assistantEnabled).map((entry) => (
            <NavButton key={entry.id} entry={entry} active={page === entry.id} onNavigate={navigate} />
          ))}
        </div>

        <p className="px-2 pb-1.5 pt-5 text-[11px] font-medium leading-[14px] text-subtle">System</p>
        <div className="space-y-px">
          {SYSTEM_ENTRIES.map((entry) => (
            <NavButton key={entry.id} entry={entry} active={page === entry.id} onNavigate={navigate} />
          ))}
        </div>

        <p className="px-2 pb-1.5 pt-5 text-[11px] font-medium leading-[14px] text-subtle">Clean up</p>
        <div className="space-y-px">
          {cleanupEntries.map((entry) => (
            <NavButton
              key={entry.id}
              entry={entry}
              active={page === entry.id}
              onNavigate={navigate}
              trailing={
                entry.id === "cleanup" && cleanupCount > 0 ? (
                  <span className="rounded-full bg-accent-fill px-1.5 py-px text-[11px] font-semibold leading-[14px] text-accent-foreground tabular">
                    {cleanupCount}
                  </span>
                ) : undefined
              }
            />
          ))}
        </div>
      </div>

      <div className="shrink-0 border-t border-border px-2 pb-2 pt-3">
        <ScanStatus />
        <NavButton
          entry={{ id: "settings", label: "Settings", icon: Settings }}
          active={page === "settings"}
          onNavigate={navigate}
        />
      </div>
    </nav>
  );
}
