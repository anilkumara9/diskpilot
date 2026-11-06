import { useCallback, useEffect, useState } from "react";
import {
  Box,
  Container,
  Eye,
  HardDrive,
  Image as ImageIcon,
  Layers,
  PowerOff,
  RefreshCw,
  Wrench,
} from "lucide-react";
import * as api from "@/lib/api";
import { formatBytes } from "@/lib/format";
import type {
  DockerActionRequest,
  DockerActionPreview,
  DockerActionResult,
  DockerContainer,
  DockerImage,
  DockerOverview,
  DockerState,
  DockerVolume,
} from "@/lib/types";
import { PageHeader } from "@/components/layout/PageHeader";
import { Button } from "@/components/ui/button";
import { Dialog } from "@/components/ui/dialog";
import { Section } from "@/components/ui/section";
import { SkeletonRows } from "@/components/ui/skeleton";
import { CopyButton } from "@/components/shared/CopyButton";
import { DetailsPanel, DetailRow } from "@/components/shared/DetailsPanel";
import { EmptyState } from "@/components/shared/EmptyState";
import { MarkerTag } from "@/components/shared/MarkerTag";
import { useAppStore } from "@/lib/store";

/* ---------------- Action dialog ---------------- */

export function DockerActionDialog({
  request,
  preview,
  onClose,
  onExecuted,
}: {
  request: DockerActionRequest;
  preview: DockerActionPreview;
  onClose: () => void;
  onExecuted: () => void;
}) {
  const setBanner = useAppStore((s) => s.setBanner);
  const [running, setRunning] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [result, setResult] = useState<DockerActionResult | null>(null);

  const execute = async () => {
    setRunning(true);
    setError(null);
    try {
      const executed = await api.dockerExecuteAction(request);
      setResult(executed);
      setBanner({ kind: "success", message: "Docker action complete." });
      onExecuted();
    } catch (err) {
      setError(err instanceof Error ? err.message : "The Docker action could not be run.");
    } finally {
      setRunning(false);
    }
  };

  return (
    <Dialog
      open
      onClose={onClose}
      title="Preview Docker action"
      description="The action is re-checked immediately before it runs. This is the exact command that will be executed."
      dismissible={!running}
      footer={
        result ? (
          <Button variant="primary" onClick={onClose}>Done</Button>
        ) : (
          <>
            <Button variant="secondary" onClick={onClose} disabled={running}>Cancel</Button>
            <Button variant="primary" onClick={() => void execute()} disabled={running}>
              {running ? "Running…" : "Run this command"}
            </Button>
          </>
        )
      }
    >
      <p className="text-[12px] leading-4 text-muted">{preview.description}</p>
      <div className="mt-3 flex items-center gap-2">
        <code className="mono-path flex-1 rounded-md bg-panel-raised px-2.5 py-2 text-foreground">{preview.command}</code>
        <CopyButton text={preview.command} />
      </div>
      {result ? (
        <div className="mt-3 space-y-2">
          <p className="text-[12px] font-medium text-foreground">Result</p>
          <div className="flex items-center gap-2">
            <code className="mono-path flex-1 rounded-md bg-panel-raised px-2.5 py-2 text-foreground">{result.command}</code>
            <CopyButton text={result.command} />
          </div>
          {result.output ? (
            <pre className="mono-path max-h-[180px] overflow-y-auto whitespace-pre-wrap rounded-md bg-panel-raised px-2.5 py-2 text-foreground">{result.output}</pre>
          ) : (
            <p className="text-[12px] text-muted">The command produced no output.</p>
          )}
        </div>
      ) : null}
      {error ? <p role="alert" className="mt-3 text-[12px] text-critical">{error}</p> : null}
    </Dialog>
  );
}

/* ---------------- Inspect panel ---------------- */

export function InspectPanel({
  title,
  rows,
  onClose,
}: {
  title: string;
  rows: { label: string; value: string }[];
  onClose: () => void;
}) {
  return (
    <DetailsPanel title={title} onClose={onClose}>
      <div>
        {rows.map((row) => (
          <DetailRow key={row.label} label={row.label}>
            <span className="tabular">{row.value}</span>
          </DetailRow>
        ))}
      </div>
      <p className="mt-3 text-[12px] leading-4 text-muted">
        Volumes are listed and inspected only — DiskPilot never removes a Docker volume.
      </p>
    </DetailsPanel>
  );
}

/* ---------------- Tables ---------------- */

export function DockerTables({
  overview,
  onAction,
  onInspectVolume,
}: {
  overview: DockerOverview;
  onAction: (request: DockerActionRequest) => void;
  onInspectVolume: (volume: DockerVolume) => void;
}) {
  const stopped = (c: DockerContainer) => !/^(up|running|healthy)/i.test(c.state);
  return (
    <div>
      <Section title={`Containers (${overview.containers.length})`}>
        {overview.containers.length === 0 ? (
          <p className="text-[12px] text-muted">No containers.</p>
        ) : (
          <div className="overflow-hidden rounded-[10px] border border-border">
            <table className="w-full border-collapse">
              <thead>
                <tr className="border-b border-border text-left">
                  <th scope="col" className="px-3 py-2 text-[12px] font-medium text-muted">Name</th>
                  <th scope="col" className="px-3 py-2 text-[12px] font-medium text-muted">State</th>
                  <th scope="col" className="px-3 py-2 text-[12px] font-medium text-muted">Image</th>
                  <th scope="col" className="px-3 py-2 text-[12px] font-medium text-muted">Compose project</th>
                  <th scope="col" className="px-3 py-2 text-right text-[12px] font-medium text-muted">Disk</th>
                  <th scope="col" className="px-3 py-2"><span className="sr-only">Actions</span></th>
                </tr>
              </thead>
              <tbody>
                {overview.containers.map((container) => (
                  <tr key={container.id} className="h-8 border-b border-border/60 hover:bg-hover">
                    <td className="px-3 py-1 text-[13px] font-medium text-foreground">{container.name}</td>
                    <td className="px-3 py-1"><MarkerTag tone={stopped(container) ? "warn" : "neutral"}>{container.state}</MarkerTag></td>
                    <td className="max-w-[200px] truncate px-3 py-1 font-mono text-[12px] text-muted">{container.image}</td>
                    <td className="px-3 py-1 text-[12px] text-muted">{container.composeProject ?? "—"}</td>
                    <td className="px-3 py-1 text-right text-[13px] tabular text-foreground">{formatBytes(container.sizeBytes)}</td>
                    <td className="px-3 py-1 text-right">
                      {stopped(container) ? (
                        <Button variant="ghost" size="sm" onClick={() => onAction({ kind: "removeStoppedContainer", id: container.id, name: container.name })}>
                          Remove stopped container
                        </Button>
                      ) : (
                        <span className="text-[11px] text-subtle">Running — left alone</span>
                      )}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </Section>

      <Section title={`Images (${overview.images.length})`}>
        {overview.images.length === 0 ? (
          <p className="text-[12px] text-muted">No images.</p>
        ) : (
          <div className="overflow-hidden rounded-[10px] border border-border">
            <table className="w-full border-collapse">
              <thead>
                <tr className="border-b border-border text-left">
                  <th scope="col" className="px-3 py-2 text-[12px] font-medium text-muted">Image</th>
                  <th scope="col" className="px-3 py-2 text-right text-[12px] font-medium text-muted">Size</th>
                  <th scope="col" className="px-3 py-2 text-right text-[12px] font-medium text-muted">Unique layers</th>
                  <th scope="col" className="px-3 py-2 text-right text-[12px] font-medium text-muted">Shared layers</th>
                  <th scope="col" className="px-3 py-2 text-right text-[12px] font-medium text-muted">Containers</th>
                  <th scope="col" className="px-3 py-2"><span className="sr-only">Actions</span></th>
                </tr>
              </thead>
              <tbody>
                {overview.images.map((image: DockerImage) => (
                  <tr key={image.id} className="h-8 border-b border-border/60 hover:bg-hover">
                    <td className="px-3 py-1 font-mono text-[12px] text-foreground">{image.repository}:{image.tag}</td>
                    <td className="px-3 py-1 text-right text-[13px] tabular text-foreground">{formatBytes(image.sizeBytes)}</td>
                    <td className="px-3 py-1 text-right text-[13px] tabular text-muted">{formatBytes(image.uniqueBytes)}</td>
                    <td className="px-3 py-1 text-right text-[13px] tabular text-muted">{formatBytes(image.sharedBytes)}</td>
                    <td className="px-3 py-1 text-right text-[13px] tabular text-muted">{image.containers}</td>
                    <td className="px-3 py-1 text-right">
                      {image.containers === 0 ? (
                        <Button variant="ghost" size="sm" onClick={() => onAction({ kind: "removeUnusedImage", id: image.id, reference: `${image.repository}:${image.tag}` })}>
                          Remove unused image
                        </Button>
                      ) : (
                        <span className="text-[11px] text-subtle">In use</span>
                      )}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </Section>

      <Section title={`Volumes (${overview.volumes.length})`} description="Volumes hold application data. They are never removed by DiskPilot.">
        {overview.volumes.length === 0 ? (
          <p className="text-[12px] text-muted">No volumes.</p>
        ) : (
          <ul className="divide-y divide-border">
            {overview.volumes.map((volume) => (
              <li key={volume.name} className="flex items-center gap-3 py-2">
                <Layers className="size-4 shrink-0 text-muted" aria-hidden="true" />
                <span className="min-w-0 flex-1 truncate font-mono text-[12px] text-foreground">{volume.name}</span>
                <span className="text-[12px] text-muted">{volume.links} link{volume.links === 1 ? "" : "s"}</span>
                <span className="w-[92px] text-right text-[13px] tabular text-foreground">{formatBytes(volume.sizeBytes)}</span>
                <Button variant="ghost" size="sm" onClick={() => onInspectVolume(volume)}>
                  <Eye className="size-3.5" aria-hidden="true" />
                  Inspect
                </Button>
              </li>
            ))}
          </ul>
        )}
      </Section>
    </div>
  );
}

/* ---------------- Page ---------------- */

export function DockerPage() {
  const [overview, setOverview] = useState<DockerOverview | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [preview, setPreview] = useState<{ request: DockerActionRequest; preview: DockerActionPreview } | null>(null);
  const [previewError, setPreviewError] = useState<string | null>(null);
  const [inspecting, setInspecting] = useState<DockerVolume | null>(null);

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      setOverview(await api.getDockerOverview());
    } catch (err) {
      setError(err instanceof Error ? err.message : "Docker status could not be loaded.");
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => { void load(); }, [load]);

  const requestAction = async (request: DockerActionRequest) => {
    setPreviewError(null);
    try {
      setPreview({ request, preview: await api.dockerPreviewAction(request) });
    } catch (err) {
      setPreviewError(err instanceof Error ? err.message : "This action is not available right now.");
    }
  };

  /** Display state derived from the DTO's installed/running flags. */
  const state: DockerState | null = !overview
    ? null
    : !overview.installed
      ? "notInstalled"
      : !overview.running
        ? "notRunning"
        : "ready";

  /** Space Docker reports as reclaimable that the virtual disk still
      holds on to until it is compacted. Only meaningful while the
      engine is running and reporting real totals. */
  const vhdxGap =
    overview && overview.vhdxBytes != null && state === "ready"
      ? overview.vhdxBytes -
        (overview.imagesReclaimableBytes +
          overview.containersReclaimableBytes +
          overview.buildCacheReclaimableBytes)
      : null;

  return (
    <div className="h-full overflow-y-auto px-6 py-6">
      <div className="max-w-[880px]">
        <PageHeader
          title="Docker"
          subtitle="Usage from docker system df, with a small closed set of safe actions. Every action is previewed with its exact command first."
          showScanControl={false}
          actions={
            <Button variant="secondary" onClick={() => void load()} disabled={loading}>
              <RefreshCw className="size-3.5" aria-hidden="true" />
              Refresh
            </Button>
          }
        />
        {error ? <p role="alert" className="pb-3 text-[12px] text-critical">{error}</p> : null}
        {previewError ? <p role="alert" className="pb-3 text-[12px] text-critical">{previewError}</p> : null}

        {loading && !overview ? (
          <SkeletonRows rows={7} />
        ) : !overview || state === "notInstalled" ? (
          <EmptyState
            icon={Box}
            className="min-h-[360px]"
            title="Docker is not installed"
            description="No Docker installation was found in the standard locations on this PC. If Docker Desktop is installed elsewhere, its storage still appears under Docker storage after a scan."
          />
        ) : state === "notRunning" ? (
          <EmptyState
            icon={PowerOff}
            className="min-h-[360px]"
            title="Docker is not running"
            description="Docker Desktop is installed but the engine is not responding. Start Docker Desktop, then refresh this page. The WSL disk diagnostics below still apply."
            action={<Button variant="secondary" onClick={() => void load()}>Refresh</Button>}
          />
        ) : null}

        {overview && state === "ready" ? (
          <>
            <div className="grid grid-cols-2 gap-3 md:grid-cols-4">
              {[
                { icon: ImageIcon, label: "Images", value: overview.imagesTotalBytes, sub: `${formatBytes(overview.imagesReclaimableBytes)} reclaimable` },
                { icon: Container, label: "Containers", value: overview.containersTotalBytes, sub: `${formatBytes(overview.containersReclaimableBytes)} reclaimable` },
                { icon: Layers, label: "Volumes", value: overview.volumesTotalBytes, sub: "Never removed" },
                { icon: Wrench, label: "Build cache", value: overview.buildCacheBytes, sub: `${formatBytes(overview.buildCacheReclaimableBytes)} reclaimable` },
              ].map((card) => (
                <div key={card.label} className="rounded-[10px] border border-border px-3.5 py-3">
                  <p className="flex items-center gap-1.5 text-[12px] text-muted">
                    <card.icon className="size-3.5" aria-hidden="true" />
                    {card.label}
                  </p>
                  <p className="mt-1 text-[20px] font-semibold leading-6 tabular text-foreground">{formatBytes(card.value)}</p>
                  <p className="text-[11px] text-muted">{card.sub}</p>
                </div>
              ))}
            </div>

            {overview.buildCacheReclaimableBytes > 0 ? (
              <div className="mt-4 flex items-center gap-3 rounded-[10px] border border-border bg-panel-raised px-4 py-3">
                <p className="flex-1 text-[12px] leading-4 text-muted">
                  <span className="font-medium text-foreground">{formatBytes(overview.buildCacheReclaimableBytes)}</span> of
                  unused build cache can be pruned. Only unused cache is listed and pruned — active cache is left alone.
                </p>
                <Button variant="secondary" size="sm" onClick={() => void requestAction({ kind: "pruneBuildCache" })}>
                  Preview builder prune
                </Button>
              </div>
            ) : null}

            <div className="flex gap-4">
              <div className="min-w-0 flex-1">
                <DockerTables
                  overview={overview}
                  onAction={(request) => void requestAction(request)}
                  onInspectVolume={setInspecting}
                />
              </div>
              {inspecting ? (
                <div className="pt-6">
                  <InspectPanel
                    title={inspecting.name}
                    rows={[
                      { label: "Size", value: formatBytes(inspecting.sizeBytes) },
                      { label: "Links", value: String(inspecting.links) },
                    ]}
                    onClose={() => setInspecting(null)}
                  />
                </div>
              ) : null}
            </div>
          </>
        ) : null}

        {overview && overview.vhdxBytes != null && overview.vhdxPath ? (
          <Section
            title="Docker Desktop disk image (WSL 2)"
            description="On Windows, Docker Desktop stores everything in a WSL 2 virtual disk. Deleting images shrinks what Docker reports, but the virtual disk file itself only shrinks after compaction."
          >
            <div className="rounded-[10px] border border-border px-4 py-3">
              <p className="flex items-center gap-2 text-[12px] text-muted">
                <HardDrive className="size-4 shrink-0" aria-hidden="true" />
                <span className="mono-path text-foreground">{overview.vhdxPath}</span>
              </p>
              <dl className="mt-2 grid grid-cols-2 gap-x-8 gap-y-1.5 text-[12px]">
                <div><dt className="text-muted">Virtual disk size on disk</dt><dd className="tabular text-foreground">{formatBytes(overview.vhdxBytes)}</dd></div>
                {vhdxGap != null && vhdxGap > 0 ? (
                  <div><dt className="text-muted">Potentially reclaimable gap</dt><dd className="tabular text-foreground">{formatBytes(vhdxGap)}</dd></div>
                ) : null}
              </dl>
              {overview.vhdxCompactionSteps && overview.vhdxCompactionSteps.length > 0 ? (
                <>
                  <h3 className="mt-3 text-[12px] font-semibold text-foreground">Compaction steps (shown for you to run — DiskPilot never runs them)</h3>
                  <ol className="mt-1.5 list-decimal space-y-1 pl-5 text-[12px] leading-4 text-muted">
                    {overview.vhdxCompactionSteps.map((step, i) => (
                      <li key={i}>{step}</li>
                    ))}
                  </ol>
                </>
              ) : null}
            </div>
          </Section>
        ) : null}
      </div>

      {preview ? (
        <DockerActionDialog request={preview.request} preview={preview.preview} onClose={() => setPreview(null)} onExecuted={() => void load()} />
      ) : null}
    </div>
  );
}
