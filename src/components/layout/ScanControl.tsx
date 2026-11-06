import { FolderOpen, Play, Square } from "lucide-react";
import { useAppStore } from "@/lib/store";
import * as api from "@/lib/api";
import { displayPath } from "@/lib/format";
import { Button } from "@/components/ui/button";

export function ScanControl({ compact = false }: { compact?: boolean }) {
  const scanning = useAppStore((s) => s.scanning);
  const scanRoot = useAppStore((s) => s.settings.scanRoot);
  const homeDir = useAppStore((s) => s.env?.homeDir);
  const startScan = useAppStore((s) => s.startScan);
  const cancelScan = useAppStore((s) => s.cancelScan);
  const updateSettings = useAppStore((s) => s.updateSettings);

  const pick = async () => {
    const folder = await api.pickFolder(scanRoot || undefined);
    if (folder) await updateSettings({ scanRoot: folder });
  };

  if (scanning) {
    return (
      <div className="flex items-center gap-2">
        {!compact ? (
          <span className="mono-path max-w-[280px] truncate text-muted" title={scanRoot}>
            {displayPath(scanRoot, homeDir)}
          </span>
        ) : null}
        <Button variant="secondary" size="sm" onClick={() => void cancelScan()}>
          <Square className="size-3" aria-hidden="true" />
          Cancel scan
        </Button>
      </div>
    );
  }

  return (
    <div className="flex items-center gap-2">
      {!compact ? (
        <button
          type="button"
          onClick={() => void pick()}
          title="Choose the folder to scan"
          className="mono-path flex max-w-[280px] items-center gap-1.5 truncate rounded-md px-1.5 py-1 text-muted transition-colors duration-120 hover:bg-hover hover:text-foreground"
        >
          <FolderOpen className="size-3.5 shrink-0" aria-hidden="true" />
          <span className="truncate">{displayPath(scanRoot, homeDir) || "Choose a folder"}</span>
        </button>
      ) : null}
      <Button variant="primary" size="sm" onClick={() => void startScan()} disabled={!scanRoot}>
        <Play className="size-3.5" aria-hidden="true" />
        Scan
      </Button>
    </div>
  );
}
