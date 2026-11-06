import { useState } from "react";
import { FolderTree, Scale } from "lucide-react";
import { useAppStore } from "@/lib/store";
import { PageHeader } from "@/components/layout/PageHeader";
import { EmptyState } from "@/components/shared/EmptyState";
import { ResultsAge } from "@/components/shared/ResultsAge";
import { ExplorerView } from "@/features/explorer/ExplorerPage";
import { LargeFilesPage } from "@/features/large-files/LargeFilesPage";
import { cn } from "@/lib/utils";

export type StorageTab = "folders" | "largest";

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
  ];
  return (
    <div role="tablist" aria-label="Storage views" className="flex gap-1 rounded-md bg-panel-raised p-1">
      {tabs.map((t) => (
        <button
          key={t.id}
          role="tab"
          aria-selected={tab === t.id}
          onClick={() => onChange(t.id)}
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

export function StoragePage() {
  const summary = useAppStore((s) => s.summary);
  const scanning = useAppStore((s) => s.scanning);
  const [tab, setTab] = useState<StorageTab>("folders");

  return (
    <div className="flex h-full flex-col px-6 py-6">
      <PageHeader
        title="Storage"
        subtitle={summary ? undefined : "Browse what the scan measured, folder by folder."}
        actions={summary ? <ResultsAge finishedAtMs={summary.finishedAtMs} /> : undefined}
      />
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
          <ExplorerView rootPath={summary.root} embedded />
        </div>
      )}
    </div>
  );
}
