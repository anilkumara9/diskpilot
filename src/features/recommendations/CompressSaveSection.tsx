import { useCallback, useEffect, useMemo, useState } from "react";
import * as api from "@/lib/api";
import { useAppStore } from "@/lib/store";
import { untouchedLabel } from "@/lib/compression";
import { basename, displayPath, formatBytes, formatCount } from "@/lib/format";
import type { CompressionCandidate } from "@/lib/types";
import { Button } from "@/components/ui/button";
import { Dialog } from "@/components/ui/dialog";
import { Section } from "@/components/ui/section";
import { MarkerTag } from "@/components/shared/MarkerTag";

/**
 * Compress & save: large, long-untouched folders from the scan
 * that Windows can compress in place (NTFS compression). Nothing
 * is deleted and the folder keeps working; the shell does the
 * work and reports the outcome per folder. The section hides
 * entirely when there is nothing to offer.
 *
 * Undo surface (v1): folders compressed from this page are
 * tracked for the session and keep their row — badged
 * "Compressed in this session" — with a Decompress action, even
 * though the shell stops returning them as candidates. The
 * badge says exactly that and nothing more.
 */

interface RowState {
  busy: boolean;
  compressedHere: boolean;
  note: string | null;
  noteIsError: boolean;
}

const IDLE: RowState = { busy: false, compressedHere: false, note: null, noteIsError: false };

export function CompressSaveSection() {
  const homeDir = useAppStore((s) => s.env?.homeDir);
  const finishedAtMs = useAppStore((s) => s.summary?.finishedAtMs);
  const [candidates, setCandidates] = useState<CompressionCandidate[] | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [reloadKey, setReloadKey] = useState(0);
  const [rowStates, setRowStates] = useState<Record<string, RowState>>({});
  /* Folders compressed from this page, kept so their rows (and
     the undo) survive the shell filtering them out of later
     candidate lists. Page-session state only. */
  const [sessionCompressed, setSessionCompressed] = useState<
    Record<string, CompressionCandidate>
  >({});
  const [confirming, setConfirming] = useState<CompressionCandidate | null>(null);

  const load = useCallback(async () => {
    const rows = await api.getCompressionCandidates();
    setCandidates(rows);
    setLoadError(null);
    return rows;
  }, []);

  /* Candidates derive from the scan: loaded on mount and re-read
     whenever a scan finishes. A failed read is reported in the
     section, never mistaken for "nothing to compress". */
  useEffect(() => {
    let live = true;
    load().catch((err: unknown) => {
      if (live) {
        setLoadError(
          err instanceof Error ? err.message : "Compression candidates could not be loaded.",
        );
      }
    });
    return () => {
      live = false;
    };
  }, [load, finishedAtMs, reloadKey]);

  const rows = useMemo(() => {
    const byPath = new Map<string, CompressionCandidate>();
    for (const candidate of candidates ?? []) byPath.set(candidate.path, candidate);
    for (const [path, candidate] of Object.entries(sessionCompressed)) {
      if (!byPath.has(path)) byPath.set(path, candidate);
    }
    return [...byPath.values()];
  }, [candidates, sessionCompressed]);

  const setRow = (path: string, patch: Partial<RowState>) =>
    setRowStates((prev) => ({ ...prev, [path]: { ...(prev[path] ?? IDLE), ...patch } }));

  const applyCompression = async (candidate: CompressionCandidate, compress: boolean) => {
    setRow(candidate.path, { busy: true, note: null, noteIsError: false });
    try {
      const result = await api.setFolderCompression(candidate.path, compress);
      if (result.ok) {
        if (compress) {
          setSessionCompressed((prev) => ({ ...prev, [candidate.path]: candidate }));
          setRow(candidate.path, {
            busy: false,
            compressedHere: true,
            note: "Compressed — Windows will report the smaller size on the next scan.",
            noteIsError: false,
          });
        } else {
          setSessionCompressed((prev) => {
            const next = { ...prev };
            delete next[candidate.path];
            return next;
          });
          setRow(candidate.path, {
            busy: false,
            compressedHere: false,
            note: "Decompressed — this folder is back to its uncompressed state.",
            noteIsError: false,
          });
        }
      } else {
        setRow(candidate.path, {
          busy: false,
          note: result.error ?? "The folder's compression state could not be changed.",
          noteIsError: true,
        });
      }
    } catch (err) {
      setRow(candidate.path, {
        busy: false,
        note: err instanceof Error ? err.message : "The folder could not be reached.",
        noteIsError: true,
      });
    }
  };

  if (loadError) {
    return (
      <Section
        title="Compress & save"
        description="Old, untouched folders can be compressed by Windows in place — nothing is deleted, apps keep working, and you can undo it any time."
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

  /* Nothing loaded yet, or nothing to offer and nothing done
     this session: the section is simply not there. */
  if (candidates === null || rows.length === 0) return null;

  const confirmingState = confirming ? (rowStates[confirming.path] ?? IDLE) : IDLE;

  return (
    <>
      <Section
        title="Compress & save"
        description="Old, untouched folders can be compressed by Windows in place — nothing is deleted, apps keep working, and you can undo it any time."
      >
        <ul className="divide-y divide-border rounded-[10px] border border-border px-3">
          {rows.map((candidate) => {
            const state = rowStates[candidate.path] ?? IDLE;
            return (
              <li key={candidate.path} className="py-2">
                <div className="flex items-center gap-3">
                  <div className="min-w-0 flex-1">
                    <p className="file-name truncate text-[13px] text-foreground" title={candidate.path}>
                      {basename(candidate.path)}
                    </p>
                    <p className="mono-path mt-0.5 truncate text-subtle" title={candidate.path}>
                      {displayPath(candidate.path, homeDir)}
                    </p>
                    <p className="mt-0.5 text-[11px] text-muted">
                      {formatCount(candidate.fileCount)} files ·{" "}
                      {untouchedLabel(candidate.newestModifiedMs)}
                    </p>
                  </div>
                  {state.compressedHere ? (
                    <MarkerTag tone="accent" className="shrink-0">Compressed in this session</MarkerTag>
                  ) : null}
                  <span className="w-[92px] shrink-0 text-right text-[13px] tabular text-foreground">
                    {formatBytes(candidate.bytes)}
                  </span>
                  {state.compressedHere ? (
                    <Button
                      variant="ghost"
                      size="sm"
                      disabled={state.busy}
                      onClick={() => void applyCompression(candidate, false)}
                    >
                      {state.busy ? "Working…" : "Decompress"}
                    </Button>
                  ) : (
                    <Button
                      variant="secondary"
                      size="sm"
                      disabled={state.busy}
                      onClick={() => setConfirming(candidate)}
                    >
                      Compress
                    </Button>
                  )}
                </div>
                {state.note ? (
                  <p
                    role={state.noteIsError ? "alert" : "status"}
                    className={
                      state.noteIsError
                        ? "pt-1 text-[12px] text-critical"
                        : "pt-1 text-[12px] text-muted"
                    }
                  >
                    {state.note}
                  </p>
                ) : null}
              </li>
            );
          })}
        </ul>
      </Section>

      {confirming ? (
        <Dialog
          open
          onClose={() => setConfirming(null)}
          title="Compress this folder?"
          description={`${basename(confirming.path)} will be compressed in place by Windows (NTFS compression). Nothing is deleted, and programs keep working — files uncompress automatically when opened. A large folder can take a while to compress. You can undo this from this page at any time.`}
          dismissible={!confirmingState.busy}
          footer={
            <>
              <span className="mr-auto text-[12px] tabular text-muted">
                {formatBytes(confirming.bytes)} · {formatCount(confirming.fileCount)} files
              </span>
              <Button
                variant="secondary"
                onClick={() => setConfirming(null)}
                disabled={confirmingState.busy}
              >
                Cancel
              </Button>
              <Button
                variant="primary"
                disabled={confirmingState.busy}
                onClick={async () => {
                  await applyCompression(confirming, true);
                  setConfirming(null);
                }}
              >
                {confirmingState.busy ? "Compressing…" : "Compress folder"}
              </Button>
            </>
          }
        >
          <p className="mono-path truncate text-muted" title={confirming.path}>
            {displayPath(confirming.path, homeDir)}
          </p>
        </Dialog>
      ) : null}
    </>
  );
}
