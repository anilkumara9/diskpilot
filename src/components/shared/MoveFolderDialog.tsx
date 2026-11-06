import { useEffect, useMemo, useState } from "react";
import { AlertTriangle, CheckCircle2 } from "lucide-react";
import * as api from "@/lib/api";
import { useAppStore } from "@/lib/store";
import {
  destinationDrives,
  driveRootOf,
  normalizeDestDir,
  relocationTarget,
  sameLocation,
} from "@/lib/moves";
import { basename, displayPath, formatBytes } from "@/lib/format";
import type { DriveInfo, RelocateResult } from "@/lib/types";
import { Button } from "@/components/ui/button";
import { Dialog } from "@/components/ui/dialog";
import { Input, Select } from "@/components/ui/input";

/**
 * Move a folder to another drive: the contents move, a link
 * (junction) stays at the original location so apps and games
 * keep working, and the move can be undone from History. The
 * destination is picked by DRIVE when the drive list is available
 * (the folder lands in the drive's root, keeping its name); when
 * the list cannot be read, the fallback is a plain destination
 * folder path. The shell validates everything again — this dialog
 * only ever sends a source and a destination directory.
 */
export function MoveFolderDialog({
  sourcePath,
  sourceBytes,
  onClose,
  onMoved,
}: {
  sourcePath: string;
  sourceBytes?: number;
  onClose: () => void;
  /** Called with the shell's result just before the dialog
      closes, so the parent can refresh its data. */
  onMoved: (result: RelocateResult) => void;
}) {
  const homeDir = useAppStore((s) => s.env?.homeDir);
  const [drives, setDrives] = useState<DriveInfo[] | null>(null);
  const [drivesFailed, setDrivesFailed] = useState(false);
  const [pickedDrive, setPickedDrive] = useState("");
  const [manualDest, setManualDest] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  /* A move that completed WITH a warning from the shell is held
     here: instead of the dialog vanishing, it shows what
     happened and lets the user close it themselves. */
  const [doneResult, setDoneResult] = useState<RelocateResult | null>(null);

  useEffect(() => {
    let live = true;
    api
      .getDrives()
      .then((rows) => {
        if (live) setDrives(rows);
      })
      .catch(() => {
        if (live) setDrivesFailed(true);
      });
    return () => {
      live = false;
    };
  }, []);

  const options = useMemo(
    () => (drives ? destinationDrives(drives, sourcePath) : []),
    [drives, sourcePath],
  );

  /* Drive-select mode whenever the list was read and names at
     least one drive; the manual path field is the fallback for a
     failed or empty read. */
  const useSelect = !drivesFailed && drives !== null && drives.length > 0;
  const selectedDrive = options.some((d) => d.rootPath === pickedDrive)
    ? pickedDrive
    : (options[0]?.rootPath ?? "");
  const noOtherDrive = useSelect && options.length === 0;
  /* One normalized destination for the preview AND the shell
     call: a typed "D:" must preview and send "D:\" — never a
     drive-relative path. */
  const destDir = normalizeDestDir(useSelect ? selectedDrive : manualDest);
  const target = destDir ? relocationTarget(sourcePath, destDir) : null;
  const canMove = !busy && destDir.length > 0 && !noOtherDrive;
  /* Soft honesty for the typed fallback: a same-drive move frees
     nothing and the shell will refuse it — say so here, but the
     shell stays the validator. */
  const sourceRoot = driveRootOf(sourcePath);
  const typedRoot = !useSelect && destDir ? driveRootOf(destDir) : null;
  const sameDrive =
    sourceRoot !== null && typedRoot !== null && sameLocation(sourceRoot, typedRoot);

  const move = async () => {
    if (!canMove) return;
    setBusy(true);
    setError(null);
    try {
      const result = await api.relocateFolder(sourcePath, destDir);
      onMoved(result);
      if (result.warning) {
        setDoneResult(result);
        setBusy(false);
      } else {
        onClose();
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : "The folder could not be moved.");
      setBusy(false);
    }
  };

  return (
    <Dialog
      open
      onClose={onClose}
      title={`Move ${basename(sourcePath)} to another drive`}
      description="The contents move to the other drive. A link stays at the original location, so apps and games keep working exactly as before. You can undo the move from History."
      dismissible={!busy}
      footer={
        doneResult ? (
          <Button variant="primary" onClick={onClose}>
            Close
          </Button>
        ) : (
          <>
            {sourceBytes !== undefined ? (
              <span className="mr-auto text-[12px] tabular text-muted">
                {formatBytes(sourceBytes)}
              </span>
            ) : null}
            <Button variant="secondary" onClick={onClose} disabled={busy}>
              Cancel
            </Button>
            <Button variant="primary" onClick={() => void move()} disabled={!canMove}>
              {busy ? "Moving…" : "Move folder"}
            </Button>
          </>
        )
      }
    >
      {doneResult ? (
        <div>
          <p className="flex items-center gap-2 text-[13px] font-medium text-foreground">
            <CheckCircle2 className="size-4 shrink-0 text-tier-safe" aria-hidden="true" />
            Folder moved
          </p>
          <p className="mono-path mt-2 truncate text-muted" title={doneResult.target}>
            {displayPath(doneResult.target, homeDir)}
          </p>
          <p className="mt-3 flex items-start gap-2 text-[12px] leading-4 text-tier-review">
            <AlertTriangle className="mt-px size-4 shrink-0" aria-hidden="true" />
            {doneResult.warning}
          </p>
        </div>
      ) : (
        <>
      {error ? (
        <p role="alert" className="mb-3 text-[12px] text-critical">{error}</p>
      ) : null}

      <p className="text-[12px] font-medium text-foreground">From</p>
      <p className="mono-path mt-0.5 truncate text-muted" title={sourcePath}>
        {displayPath(sourcePath, homeDir)}
      </p>

      <div className="mt-4">
        <label htmlFor="move-destination" className="text-[12px] font-medium text-foreground">
          Move to
        </label>
        {drives === null && !drivesFailed ? (
          <p className="mt-1.5 text-[12px] text-muted">Checking drives…</p>
        ) : useSelect ? (
          <Select
            id="move-destination"
            className="mt-1.5 w-full"
            value={selectedDrive}
            disabled={busy || noOtherDrive}
            onChange={(e) => setPickedDrive(e.target.value)}
          >
            {options.map((drive) => (
              <option key={drive.rootPath} value={drive.rootPath}>
                {drive.rootPath.replace(/[\\/]+$/, "")} · {formatBytes(drive.freeBytes)} free
              </option>
            ))}
          </Select>
        ) : (
          <Input
            id="move-destination"
            className="mono-path mt-1.5 w-full"
            value={manualDest}
            disabled={busy}
            onChange={(e) => setManualDest(e.target.value)}
            placeholder="Destination folder, e.g. D:\\Moved"
            aria-label="Destination folder"
            spellCheck={false}
          />
        )}
        {!useSelect && drivesFailed ? (
          <p className="mt-1.5 text-[12px] text-muted">
            Couldn&rsquo;t read the drive list, so type the destination folder instead.
          </p>
        ) : null}
        {noOtherDrive ? (
          <p className="mt-1.5 text-[12px] text-muted">
            This PC has no other drive to move this folder to.
          </p>
        ) : target ? (
          <p className="mt-1.5 text-[12px] text-muted">
            It will move to{" "}
            <span className="mono-path text-foreground" title={target}>
              {displayPath(target, homeDir)}
            </span>
          </p>
        ) : null}
        {sameDrive ? (
          <p className="mt-1.5 text-[12px] text-tier-review">
            This is the same drive — the shell will refuse a same-drive move.
          </p>
        ) : null}
      </div>

      <p className="mt-4 text-[12px] leading-4 text-muted">
        Links (junctions) inside the folder aren&rsquo;t moved.
      </p>

      {busy ? (
        <p role="status" className="mt-4 text-[12px] text-muted">
          Moving… large folders can take a while. Don&rsquo;t close DiskPilot.
        </p>
      ) : null}
        </>
      )}
    </Dialog>
  );
}
