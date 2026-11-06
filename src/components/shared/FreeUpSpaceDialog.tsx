import { useState } from "react";
import { CloudUpload } from "lucide-react";
import * as api from "@/lib/api";
import { formatBytes } from "@/lib/format";
import type { FreeUpResult } from "@/lib/types";
import { Button } from "@/components/ui/button";
import { Dialog } from "@/components/ui/dialog";

/**
 * Confirmation for marking cloud files online-only. The copy is the
 * contract: the files stay in the provider's cloud and re-download
 * when opened — nothing is deleted. The shell does the work through
 * `free_up_space`; the caller reports the result (banner) and
 * refreshes its own data via `onDone`.
 */
export function FreeUpSpaceDialog({
  open,
  provider,
  paths,
  bytes,
  onClose,
  onDone,
}: {
  open: boolean;
  provider: string;
  paths: string[];
  /** Local bytes expected to be freed, when known. */
  bytes?: number | null;
  onClose: () => void;
  onDone: (result: FreeUpResult) => void;
}) {
  const [running, setRunning] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const confirm = async () => {
    setRunning(true);
    setError(null);
    try {
      const result = await api.freeUpSpace(paths);
      onDone(result);
      onClose();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Space could not be freed up.");
      setRunning(false);
    }
  };

  return (
    <Dialog
      open={open}
      onClose={onClose}
      title={`Free up space in ${provider}?`}
      description={
        bytes != null
          ? `About ${formatBytes(bytes)} stored on this PC can be freed.`
          : "Files stored on this PC can be freed."
      }
      dismissible={!running}
      footer={
        <>
          <Button variant="secondary" onClick={onClose} disabled={running}>
            Cancel
          </Button>
          <Button variant="primary" onClick={() => void confirm()} disabled={running}>
            <CloudUpload className="size-4" aria-hidden="true" />
            {running ? "Working…" : "Free up space"}
          </Button>
        </>
      }
    >
      {error ? (
        <p role="alert" className="mb-3 text-[12px] text-critical">
          {error}
        </p>
      ) : null}
      <p className="text-[13px] leading-[18px] text-foreground">
        The files stay in {provider} — only the copies on this PC are removed. Each file downloads
        again automatically the next time you open it. Nothing is deleted from {provider}.
      </p>
    </Dialog>
  );
}
