import { useEffect, useMemo, useState } from "react";
import { Code2, FolderOpen, KeyRound, Search, ShieldCheck } from "lucide-react";
import * as api from "@/lib/api";
import { useAppStore } from "@/lib/store";
import { displayPath, formatBytes } from "@/lib/format";
import type { SecretFinding, ToolItemState, ToolReport } from "@/lib/types";
import { PageHeader } from "@/components/layout/PageHeader";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Section } from "@/components/ui/section";
import { SkeletonRows } from "@/components/ui/skeleton";
import { EmptyState } from "@/components/shared/EmptyState";
import { MarkerTag } from "@/components/shared/MarkerTag";

export const STATE_TONE: Record<ToolItemState, "neutral" | "warn" | "accent"> = {
  current: "neutral",
  stale: "warn",
  orphaned: "warn",
  superseded: "warn",
  informational: "accent",
};

export const STATE_LABEL: Record<ToolItemState, string> = {
  current: "Current",
  stale: "Stale",
  orphaned: "Orphaned",
  superseded: "Superseded",
  informational: "Info",
};

export function TranscriptSecrets() {
  const [findings, setFindings] = useState<SecretFinding[] | null>(null);
  const [checking, setChecking] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const run = async () => {
    setChecking(true);
    setError(null);
    try {
      setFindings(await api.scanTranscriptSecrets());
    } catch (err) {
      setError(err instanceof Error ? err.message : "The transcript check could not run.");
    } finally {
      setChecking(false);
    }
  };

  return (
    <div className="rounded-[10px] border border-border px-4 py-3">
      <div className="flex items-start gap-3">
        <KeyRound className="mt-0.5 size-4 shrink-0 text-tier-review" aria-hidden="true" />
        <div className="min-w-0 flex-1">
          <p className="text-[13px] font-medium text-foreground">Credential check for transcripts</p>
          <p className="mt-0.5 text-[12px] leading-4 text-muted">
            Checks Claude Code, Codex, and Gemini CLI transcripts for known credential formats — API keys,
            tokens, and private keys. The check runs fully offline, shows matches masked, and changes nothing.
            If a real key appears, rotate it with the provider; deleting the transcript alone is not enough.
          </p>
        </div>
        <Button variant="secondary" size="sm" onClick={() => void run()} disabled={checking}>
          {checking ? "Checking…" : findings ? "Check again" : "Check transcripts"}
        </Button>
      </div>
      {error ? <p role="alert" className="mt-2 text-[12px] text-critical">{error}</p> : null}
      {findings !== null ? (
        findings.length === 0 ? (
          <p className="mt-3 flex items-center gap-2 text-[12px] text-tier-safe">
            <ShieldCheck className="size-4" aria-hidden="true" />
            No known credential formats were found in the transcripts that were checked.
          </p>
        ) : (
          <ul className="mt-3 divide-y divide-border">
            {findings.map((finding, i) => (
              <li key={`${finding.path}-${i}`} className="py-2">
                <p className="flex flex-wrap items-center gap-2 text-[12px]">
                  <MarkerTag tone="warn">{finding.kind}</MarkerTag>
                  <span className="font-mono text-[12px] text-critical">{finding.masked}</span>
                  <span className="text-muted">line {finding.line}</span>
                </p>
                <p className="mono-path mt-0.5 text-subtle">{finding.path}</p>
              </li>
            ))}
          </ul>
        )
      ) : null}
    </div>
  );
}

export function DeveloperToolsPage() {
  const homeDir = useAppStore((s) => s.env?.homeDir);
  const finishedAtMs = useAppStore((s) => s.summary?.finishedAtMs);
  const [reports, setReports] = useState<ToolReport[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  const [revealError, setRevealError] = useState<string | null>(null);

  /* Reloads whenever a scan finishes — the reports measure tool
     storage, which the scan is what changes. */
  useEffect(() => {
    let live = true;
    api.getDevtoolsReports()
      .then((rows) => { if (live) setReports(rows); })
      .catch((err: unknown) => {
        if (live) setError(err instanceof Error ? err.message : "Developer tool reports could not be loaded.");
      });
    return () => { live = false; };
  }, [finishedAtMs]);

  const visible = useMemo(() => {
    if (!reports) return [];
    const q = query.trim().toLowerCase();
    if (!q) return reports;
    return reports
      .map((report) => ({
        ...report,
        sections: report.sections
          .map((section) => ({
            ...section,
            items: section.items.filter(
              (item) =>
                item.name.toLowerCase().includes(q) ||
                item.path.toLowerCase().includes(q) ||
                report.tool.toLowerCase().includes(q),
            ),
          }))
          .filter((section) => section.items.length > 0),
      }))
      .filter((report) => report.sections.length > 0);
  }, [reports, query]);

  const reveal = async (path: string) => {
    setRevealError(null);
    try {
      await api.revealInExplorer(path);
    } catch (err) {
      setRevealError(err instanceof Error ? err.message : "Could not open this location in Explorer.");
    }
  };

  return (
    <div className="h-full overflow-y-auto px-6 py-6">
      <div className="max-w-[880px]">
        <PageHeader
          title="Developer Tools"
          subtitle="Editors, AI coding tools, language toolchains, and local models — what each one stores, and what state it is in. Other apps' databases are read only and never modified."
        />
        {error ? <p role="alert" className="pb-3 text-[12px] text-critical">{error}</p> : null}
        {revealError ? <p role="alert" className="pb-3 text-[12px] text-critical">{revealError}</p> : null}

        <div className="max-w-[420px] pb-1">
          <div className="relative">
            <Search className="pointer-events-none absolute left-2.5 top-1/2 size-4 -translate-y-1/2 text-subtle" aria-hidden="true" />
            <Input value={query} onChange={(e) => setQuery(e.target.value)} placeholder="Filter tools and items" aria-label="Filter tools and items" className="pl-8" />
          </div>
        </div>

        <Section title="Transcript secrets">
          <TranscriptSecrets />
        </Section>

        {reports === null ? (
          <div className="mt-6"><SkeletonRows rows={8} /></div>
        ) : visible.length === 0 ? (
          query.trim() ? (
            <EmptyState
              icon={Code2}
              className="min-h-[360px]"
              title="No tools match this filter"
              description="Nothing in the developer tool reports matches the current filter. Clear the filter to see every tool again."
            />
          ) : (
            <EmptyState
              icon={Code2}
              className="min-h-[360px]"
              title="No developer tool data found"
              description="Per-tool storage reports appear here for editors, AI coding tools, Node.js, Rust, Python, .NET, Visual Studio, JetBrains, and local model tools detected on this PC."
            />
          )
        ) : (
          visible.map((report) => {
            const total = report.sections.reduce(
              (acc, section) => acc + section.items.reduce((a, item) => a + item.bytes, 0),
              0,
            );
            return (
              <Section
                key={report.tool}
                title={report.tool}
                action={<span className="text-[12px] tabular text-muted">{formatBytes(total)}</span>}
              >
                {report.sections.map((section) => (
                  <div key={section.title} className="mb-3 last:mb-0">
                    <h3 className="pb-1 text-[12px] font-medium text-muted">{section.title}</h3>
                    <ul className="divide-y divide-border rounded-[10px] border border-border px-3">
                      {section.items.map((item) => (
                        <li key={item.path} className="flex items-center gap-3 py-2">
                          <div className="min-w-0 flex-1">
                            <p className="flex flex-wrap items-center gap-2 text-[13px] text-foreground">
                              <span className="font-medium">{item.name}</span>
                              <MarkerTag tone={STATE_TONE[item.state]}>{STATE_LABEL[item.state]}</MarkerTag>
                            </p>
                            <p className="mono-path mt-0.5 truncate text-subtle" title={item.path}>{displayPath(item.path, homeDir)}</p>
                            {item.detail ? <p className="mt-0.5 text-[12px] leading-4 text-muted">{item.detail}</p> : null}
                          </div>
                          <span className="shrink-0 text-[13px] tabular text-foreground">{formatBytes(item.bytes)}</span>
                          <Button variant="ghost" size="sm" onClick={() => void reveal(item.path)}>
                            <FolderOpen className="size-3.5" aria-hidden="true" />
                            Reveal
                          </Button>
                        </li>
                      ))}
                    </ul>
                  </div>
                ))}
              </Section>
            );
          })
        )}

        <p className="mt-6 text-[12px] leading-4 text-muted">
          How states map to cleanup: caches, logs, and update downloads are Likely safe; checkpoints, stale
          backups, and sessions for deleted projects are Review first; history, sign-in data, and settings are
          never recommended. Cleanup choices live on the Cleanup page.
        </p>
      </div>
    </div>
  );
}
