import { useState } from "react";
import { CloudUpload } from "lucide-react";
import * as api from "@/lib/api";
import { formatBytes } from "@/lib/format";
import type { FreeUpResult } from "@/lib/types";
import { Button } from "@/components/ui/button";
import { Dialog } from "@/components/ui/dialog";

/**
 * Confirmation for marking cloud files online-only. The copy is the
 * contract: the files are MARKED online-only on this PC and stay in
 * the provider's cloud; the provider releases the local space as it
 * syncs — marking does not free space on the spot, and the copy
 * never claims it did. The shell does the work through
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
      setError(err instanceof Error ? err.message : "The items could not be marked online-only.");
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
          ? `About ${formatBytes(bytes)} stored on this PC can be marked online-only; ${provider} releases the space as it syncs.`
          : `Files stored on this PC can be marked online-only; ${provider} releases the space as it syncs.`
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
        The files stay in {provider} — they are marked online-only on this PC, and {provider}{" "}
        releases the local space as it syncs (not all of it at once). Each file downloads again
        automatically the next time you open it. Nothing is deleted from {provider}.
      </p>
    </Dialog>
  );
}
