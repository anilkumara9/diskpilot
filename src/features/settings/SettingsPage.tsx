import { useEffect, useRef, useState, type KeyboardEvent } from "react";
import { Check, FolderOpen, KeyRound, ShieldAlert, Trash2, X } from "lucide-react";
import * as api from "@/lib/api";
import { useAppStore } from "@/lib/store";
import { displayPath, formatDate, formatRelative } from "@/lib/format";
import { clampMinAgeDays } from "@/lib/maintenance";
import { removeSavedSearch } from "@/lib/search";
import { updateStatusLine } from "@/lib/updater";
import { exclusionsToText, parseExclusions, sameExclusions } from "@/lib/settings";
import type {
  AssistantProvider,
  AutoMaintenanceSettings,
  AutoScanSchedule,
  ThemePreference,
} from "@/lib/types";
import { PageHeader } from "@/components/layout/PageHeader";
import { Button } from "@/components/ui/button";
import { Checkbox, Input, Select, Textarea } from "@/components/ui/input";
import { Section } from "@/components/ui/section";
import { Switch } from "@/components/ui/switch";
import { cn } from "@/lib/utils";

const PROVIDERS: { id: AssistantProvider; label: string; note: string }[] = [
  { id: "anthropic", label: "Anthropic", note: "Requests go to api.anthropic.com." },
  { id: "openai", label: "OpenAI", note: "Requests go to api.openai.com." },
  { id: "openrouter", label: "OpenRouter", note: "Requests go to openrouter.ai." },
];

export function AssistantSettings() {
  const settings = useAppStore((s) => s.settings);
  const updateSettings = useAppStore((s) => s.updateSettings);
  const [keyDraft, setKeyDraft] = useState("");
  const [hasKey, setHasKey] = useState(false);
  const [keyStatus, setKeyStatus] = useState<string | null>(null);

  /* The key status belongs to ONE provider: switching providers
     resets the draft and any status text, then refetches — the UI
     must never describe Anthropic's saved key while OpenAI is
     selected. A failed read says so instead of claiming "no key". */
  useEffect(() => {
    let live = true;
    setKeyDraft("");
    setKeyStatus(null);
    setHasKey(false);
    void api
      .hasApiKey(settings.assistantProvider)
      .then((value) => { if (live) setHasKey(value); })
      .catch(() => {
        if (live) setKeyStatus("The saved-key status could not be read from Windows Credential Manager.");
      });
    return () => { live = false; };
  }, [settings.assistantProvider]);

  const saveKey = async () => {
    if (!keyDraft.trim()) return;
    try {
      await api.saveApiKey(settings.assistantProvider, keyDraft.trim());
      setKeyDraft("");
      setHasKey(true);
      setKeyStatus("API key saved to Windows Credential Manager.");
    } catch (err) {
      setKeyStatus(err instanceof Error ? err.message : "The key could not be saved.");
    }
  };

  const removeKey = async () => {
    try {
      await api.deleteApiKey(settings.assistantProvider);
      setHasKey(false);
      setKeyStatus("API key removed.");
    } catch (err) {
      setKeyStatus(err instanceof Error ? err.message : "The key could not be removed.");
    }
  };

  return (
    <div className="space-y-3">
      <label className="flex cursor-pointer items-center justify-between gap-4">
        <span>
          <span className="block text-[13px] font-medium text-foreground">Enable assistant</span>
          <span className="block text-[12px] text-muted">
            Off by default. When enabled, the Assistant page appears and can answer questions about scan
            results using the provider below. It stays read-only either way.
          </span>
        </span>
        <Checkbox
          checked={settings.assistantEnabled}
          onChange={(e) => void updateSettings({ assistantEnabled: e.target.checked })}
          aria-label="Enable assistant"
        />
      </label>

      {settings.assistantEnabled ? (
        <div className="space-y-3 border-t border-border pt-3">
          <div className="flex items-center justify-between gap-4">
            <span>
              <span className="block text-[13px] font-medium text-foreground">Provider</span>
              <span className="block text-[12px] text-muted">
                {PROVIDERS.find((p) => p.id === settings.assistantProvider)?.note} HTTPS only, no redirects.
              </span>
            </span>
            <Select
              id="assistant-provider"
              aria-label="Provider"
              value={settings.assistantProvider}
              onChange={(e) => void updateSettings({ assistantProvider: e.target.value as AssistantProvider })}
              className="w-[160px] shrink-0"
            >
              {PROVIDERS.map((provider) => (
                <option key={provider.id} value={provider.id}>{provider.label}</option>
              ))}
            </Select>
          </div>

          <div>
            <label htmlFor="assistant-key" className="mb-1 flex items-center gap-1.5 text-[12px] font-medium text-foreground">
              <KeyRound className="size-3.5" aria-hidden="true" />
              API key
            </label>
            <div className="flex items-center gap-2">
              <Input
                id="assistant-key"
                type="password"
                autoComplete="off"
                value={keyDraft}
                onChange={(e) => setKeyDraft(e.target.value)}
                placeholder={hasKey ? "A key is saved — enter a new one to replace it" : "Paste your API key"}
                className="min-w-0 flex-1"
              />
              <Button variant="secondary" onClick={() => void saveKey()} disabled={!keyDraft.trim()}>Save key</Button>
              {hasKey ? (
                <Button variant="ghost" onClick={() => void removeKey()}>Remove</Button>
              ) : null}
            </div>
            <p className="mt-1 flex items-center gap-1.5 text-[12px] text-muted">
              {hasKey ? <Check className="size-3.5 text-tier-safe" aria-hidden="true" /> : null}
              {keyStatus ?? "The key is stored in Windows Credential Manager and never written to a file or sent anywhere except your provider."}
            </p>
          </div>
        </div>
      ) : null}
    </div>
  );
}

export function ExclusionsSettings() {
  const settings = useAppStore((s) => s.settings);
  const updateSettings = useAppStore((s) => s.updateSettings);
  const [draft, setDraft] = useState(() => exclusionsToText(settings.exclusions));
  const [saved, setSaved] = useState(false);

  const parsed = parseExclusions(draft);
  const dirty = !sameExclusions(parsed, settings.exclusions);

  // Keep the draft in step with the stored list until the user edits.
  useEffect(() => {
    if (!dirty) setDraft(exclusionsToText(settings.exclusions));
  }, [settings.exclusions, dirty]);

  const save = async () => {
    const persisted = await updateSettings({ exclusions: parsed });
    if (!persisted) return; // the page-level save error explains
    setDraft(exclusionsToText(parsed));
    setSaved(true);
    window.setTimeout(() => setSaved(false), 1600);
  };

  return (
    <div className="mt-4 border-t border-border pt-3">
      <label htmlFor="scan-exclusions" className="mb-1 block text-[12px] font-medium text-foreground">
        Excluded folders and patterns
      </label>
      <Textarea
        id="scan-exclusions"
        value={draft}
        onChange={(e) => setDraft(e.target.value)}
        placeholder={"One per line, for example:\n*.iso\nbuild-*\nMyBackups"}
        className="mono-path min-h-[96px]"
        spellCheck={false}
      />
      <p className="mt-1 text-[12px] leading-4 text-muted">
        One pattern per line. A pattern is a folder or file name (like{" "}
        <code className="mono-path">MyBackups</code>), or a wildcard (like{" "}
        <code className="mono-path">*.iso</code> or <code className="mono-path">build-*</code>). Anything
        matching is left out of scans entirely — it is not measured, counted, or recommended.
      </p>
      <div className="mt-2 flex items-center gap-2">
        <Button variant="secondary" onClick={() => void save()} disabled={!dirty}>
          Save exclusions
        </Button>
        {saved && !dirty ? (
          <span className="flex items-center gap-1.5 text-[12px] text-muted">
            <Check className="size-3.5 text-tier-safe" aria-hidden="true" />
            Saved — changes apply to the next scan.
          </span>
        ) : (
          <span className="text-[12px] text-muted">Changes apply to the next scan.</span>
        )}
      </div>
    </div>
  );
}

export function SavedSearchesSettings() {
  const savedSearches = useAppStore((s) => s.settings.savedSearches);
  const updateSettings = useAppStore((s) => s.updateSettings);

  if (savedSearches.length === 0) {
    return (
      <p className="text-[12px] leading-4 text-muted">
        No saved searches yet. Run a search on the Storage page and choose “Save search” to keep
        it here and above the search box.
      </p>
    );
  }
  return (
    <ul className="divide-y divide-border rounded-[10px] border border-border px-3">
      {savedSearches.map((saved) => (
        <li key={saved} className="flex items-center gap-2 py-2">
          <span className="min-w-0 flex-1 truncate text-[13px] text-foreground">{saved}</span>
          <Button
            variant="ghost"
            size="icon"
            aria-label={`Remove saved search ${saved}`}
            onClick={() => void updateSettings({ savedSearches: removeSavedSearch(savedSearches, saved) })}
          >
            <X className="size-4" aria-hidden="true" />
          </Button>
        </li>
      ))}
    </ul>
  );
}

export function WatchSettings() {
  const settings = useAppStore((s) => s.settings);
  const updateSettings = useAppStore((s) => s.updateSettings);
  const [lowDraft, setLowDraft] = useState(() => String(settings.lowSpaceAlertGb));
  const [growthDraft, setGrowthDraft] = useState(() => String(settings.growthAlertGb));
  const [autostart, setAutostart] = useState<boolean | null>(null);
  const [autostartBusy, setAutostartBusy] = useState(false);

  /* The toggle reflects the registration actually in effect, which
     the shell reports — off Windows there is nothing to register,
     so it stays off there. */
  useEffect(() => {
    let live = true;
    void api.getAutostart().then((value) => {
      if (live) setAutostart(value);
    });
    return () => {
      live = false;
    };
  }, []);

  const commitNumber = (raw: string, apply: (n: number) => Partial<typeof settings>) => {
    const value = Number(raw);
    if (Number.isFinite(value) && value >= 0) {
      void updateSettings(apply(value));
    }
  };

  const toggleAutostart = async (checked: boolean) => {
    setAutostartBusy(true);
    try {
      const applied = await api.setAutostart(checked);
      setAutostart(applied);
      await updateSettings({ autoStart: applied });
    } finally {
      setAutostartBusy(false);
    }
  };

  return (
    <div className="space-y-4">
      <div className="flex items-start justify-between gap-4">
        <span>
          <span className="block text-[13px] font-medium text-foreground">Scheduled scans</span>
          <span className="block text-[12px] leading-4 text-muted">
            While DiskPilot is running in the tray, it rescans on this schedule and can alert you.
          </span>
        </span>
        <Select
          aria-label="Scheduled scans"
          value={settings.autoScan}
          onChange={(e) => void updateSettings({ autoScan: e.target.value as AutoScanSchedule })}
          className="w-[160px] shrink-0"
        >
          <option value="off">Off</option>
          <option value="daily">Daily</option>
          <option value="weekly">Weekly</option>
        </Select>
      </div>

      <div className="flex items-center justify-between gap-4">
        <label htmlFor="low-space-alert" className="text-[13px] font-medium text-foreground">
          Alert when free space drops below
        </label>
        <span className="flex shrink-0 items-center gap-2">
          <Input
            id="low-space-alert"
            type="number"
            min={0}
            step={1}
            value={lowDraft}
            onChange={(e) => setLowDraft(e.target.value)}
            onBlur={() => commitNumber(lowDraft, (n) => ({ lowSpaceAlertGb: n }))}
            onKeyDown={(e) => {
              if (e.key === "Enter") (e.target as HTMLInputElement).blur();
            }}
            className="w-24"
          />
          <span className="w-[132px] text-[12px] text-muted">GB</span>
        </span>
      </div>

      <div className="flex items-center justify-between gap-4">
        <label htmlFor="growth-alert" className="text-[13px] font-medium text-foreground">
          Alert when the disk grows by more than
        </label>
        <span className="flex shrink-0 items-center gap-2">
          <Input
            id="growth-alert"
            type="number"
            min={0}
            step={1}
            value={growthDraft}
            onChange={(e) => setGrowthDraft(e.target.value)}
            onBlur={() => commitNumber(growthDraft, (n) => ({ growthAlertGb: n }))}
            onKeyDown={(e) => {
              if (e.key === "Enter") (e.target as HTMLInputElement).blur();
            }}
            className="w-24"
          />
          <span className="w-[132px] text-[12px] text-muted">GB between checks</span>
        </span>
      </div>

      <label className="flex cursor-pointer items-center justify-between gap-4 border-t border-border pt-3">
        <span>
          <span className="block text-[13px] font-medium text-foreground">
            Start DiskPilot when you sign in to Windows
          </span>
          <span className="block text-[12px] leading-4 text-muted">
            DiskPilot starts in the tray, so scheduled scans and alerts are always watching.
          </span>
        </span>
        <Checkbox
          checked={autostart ?? settings.autoStart}
          disabled={autostartBusy}
          onChange={(e) => void toggleAutostart(e.target.checked)}
          aria-label="Start DiskPilot when you sign in to Windows"
        />
      </label>
    </div>
  );
}

const FREQUENCY_OPTIONS = [
  { id: "daily", label: "Daily" },
  { id: "weekly", label: "Weekly" },
] as const;

export function MaintenanceSettings() {
  const settings = useAppStore((s) => s.settings);
  const updateSettings = useAppStore((s) => s.updateSettings);
  const maintenance = settings.autoMaintenance;
  const [ageDraft, setAgeDraft] = useState(() => String(maintenance.minAgeDays));
  const frequencyRefs = useRef<(HTMLButtonElement | null)[]>([]);

  // Keep the draft in step with the stored value until edited.
  useEffect(() => {
    setAgeDraft(String(maintenance.minAgeDays));
  }, [maintenance.minAgeDays]);

  const apply = (patch: Partial<AutoMaintenanceSettings>) => {
    void updateSettings({ autoMaintenance: { ...maintenance, ...patch } });
  };

  const commitAge = () => {
    const clamped = clampMinAgeDays(Number(ageDraft));
    setAgeDraft(String(clamped));
    if (clamped !== maintenance.minAgeDays) apply({ minAgeDays: clamped });
  };

  /* APG radio pattern: arrow keys move the selection (wrapping)
     and commit it through the same apply path as a click; focus
     follows the newly selected option. */
  const onFrequencyKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const current = FREQUENCY_OPTIONS.findIndex((o) => o.id === maintenance.frequency);
    let next: number;
    if (event.key === "ArrowRight" || event.key === "ArrowDown") {
      next = (current + 1) % FREQUENCY_OPTIONS.length;
    } else if (event.key === "ArrowLeft" || event.key === "ArrowUp") {
      next = (current - 1 + FREQUENCY_OPTIONS.length) % FREQUENCY_OPTIONS.length;
    } else {
      return;
    }
    event.preventDefault();
    const option = FREQUENCY_OPTIONS[next];
    if (option.id !== maintenance.frequency) apply({ frequency: option.id });
    frequencyRefs.current[next]?.focus();
  };

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between gap-4">
        <span>
          <span className="block text-[13px] font-medium text-foreground">Enabled</span>
          <span className="block text-[12px] leading-4 text-muted">
            Runs while DiskPilot is open or in the tray, on the schedule below.
          </span>
        </span>
        <Switch
          checked={maintenance.enabled}
          onChange={(checked) => apply({ enabled: checked })}
          ariaLabel="Enable automatic maintenance"
        />
      </div>

      {maintenance.enabled ? (
        <>
          <div className="flex items-center justify-between gap-4">
            <span className="text-[13px] font-medium text-foreground">Frequency</span>
            <div
              role="radiogroup"
              aria-label="Maintenance frequency"
              className="flex gap-1 rounded-md bg-panel-raised p-1"
              onKeyDown={onFrequencyKeyDown}
            >
              {FREQUENCY_OPTIONS.map((option, index) => (
                <button
                  key={option.id}
                  ref={(el) => {
                    frequencyRefs.current[index] = el;
                  }}
                  type="button"
                  role="radio"
                  aria-checked={maintenance.frequency === option.id}
                  tabIndex={maintenance.frequency === option.id ? 0 : -1}
                  onClick={() => apply({ frequency: option.id })}
                  className={cn(
                    "h-6 rounded-[5px] px-3 text-[12px] font-medium transition-colors duration-120",
                    maintenance.frequency === option.id
                      ? "bg-panel text-foreground shadow-[0_1px_3px_rgba(0,0,0,0.14)] ring-1 ring-border"
                      : "text-muted hover:text-foreground",
                  )}
                >
                  {option.label}
                </button>
              ))}
            </div>
          </div>

          <div className="flex items-center justify-between gap-4">
            <label htmlFor="maintenance-min-age" className="text-[13px] font-medium text-foreground">
              Remove items older than
            </label>
            <span className="flex shrink-0 items-center gap-2">
              <Input
                id="maintenance-min-age"
                type="number"
                min={7}
                max={365}
                step={1}
                value={ageDraft}
                onChange={(e) => setAgeDraft(e.target.value)}
                onBlur={commitAge}
                onKeyDown={(e) => {
                  if (e.key === "Enter") (e.target as HTMLInputElement).blur();
                }}
                className="w-24"
              />
              <span className="w-[132px] text-[12px] text-muted">days (7–365)</span>
            </span>
          </div>
        </>
      ) : null}

      <p className="text-[12px] leading-4 text-muted">
        When due, DiskPilot permanently removes Recycle Bin items older than the age you set and
        clears old temporary files through the same safety checks as a manual cleanup. Every run
        is recorded in History.
      </p>
      <p className="text-[12px] leading-4 text-muted">
        Last run:{" "}
        <span
          className="text-foreground"
          title={settings.lastMaintenanceMs ? formatDate(settings.lastMaintenanceMs) : undefined}
        >
          {settings.lastMaintenanceMs ? formatRelative(settings.lastMaintenanceMs) : "Never"}
        </span>
      </p>
    </div>
  );
}

export function ExplorerIntegrationSettings() {
  const [enabled, setEnabled] = useState<boolean | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [reloadKey, setReloadKey] = useState(0);

  /* The switch reflects the registration actually in effect,
     read from the shell — a failed read says so rather than
     leaving the switch describing a guessed state. */
  useEffect(() => {
    let live = true;
    api
      .getExplorerIntegration()
      .then((value) => {
        if (!live) return;
        setEnabled(value);
        setLoadError(null);
      })
      .catch((err: unknown) => {
        if (live) {
          setLoadError(
            err instanceof Error ? err.message : "The Explorer integration state could not be read.",
          );
        }
      });
    return () => {
      live = false;
    };
  }, [reloadKey]);

  const toggle = async (checked: boolean) => {
    setBusy(true);
    setActionError(null);
    try {
      await api.setExplorerIntegration(checked);
      /* The switch shows the registration actually in effect,
         so re-read it (the autostart pattern) rather than
         trusting the requested value landed. If the re-read
         fails, the error shows and the switch keeps its last
         confirmed state. */
      const applied = await api.getExplorerIntegration();
      setEnabled(applied);
    } catch (err) {
      setActionError(
        err instanceof Error ? err.message : "The Explorer integration could not be changed.",
      );
    } finally {
      setBusy(false);
    }
  };

  return (
    <div>
      <div className="flex items-center justify-between gap-4">
        <span>
          <span className="block text-[13px] font-medium text-foreground">Analyze with DiskPilot</span>
          <span className="block text-[12px] leading-4 text-muted">
            Adds &lsquo;Analyze with DiskPilot&rsquo; to the right-click menu for folders and
            drives.
          </span>
        </span>
        <Switch
          checked={enabled ?? false}
          disabled={enabled === null || busy}
          onChange={(checked) => void toggle(checked)}
          ariaLabel="Add Analyze with DiskPilot to the Explorer right-click menu"
        />
      </div>
      {loadError ? (
        <p role="alert" className="mt-2 flex items-center gap-2 text-[12px] text-critical">
          {loadError}
          <Button variant="subtle" size="sm" onClick={() => setReloadKey((n) => n + 1)}>
            Retry
          </Button>
        </p>
      ) : null}
      {actionError ? (
        <p role="alert" className="mt-2 text-[12px] text-critical">{actionError}</p>
      ) : null}
    </div>
  );
}

export function UpdatesSettings() {
  const env = useAppStore((s) => s.env);
  const update = useAppStore((s) => s.update);
  const checkForUpdate = useAppStore((s) => s.checkForUpdate);
  const installUpdate = useAppStore((s) => s.installUpdate);
  const restartToApply = useAppStore((s) => s.restartToApply);

  const pct =
    update.status === "downloading"
      ? update.total && update.total > 0
        ? Math.min(100, Math.floor((update.downloaded / update.total) * 100))
        : null
      : null;

  return (
    <div className="mt-4 border-t border-border pt-3">
      <div className="flex items-center justify-between gap-4">
        <span>
          <span className="block text-[13px] font-medium text-foreground">Updates</span>
          <span className="block text-[12px] leading-4 text-muted">
            {updateStatusLine(update, env?.appVersion ?? null)}
          </span>
        </span>
        {update.status === "available" ? (
          <Button variant="primary" onClick={() => void installUpdate()}>
            Download &amp; install
          </Button>
        ) : update.status === "readyToRestart" ? (
          <Button variant="primary" onClick={() => void restartToApply()}>
            Restart to apply
          </Button>
        ) : (
          <Button
            variant="secondary"
            onClick={() => void checkForUpdate(true)}
            disabled={update.status === "checking" || update.status === "downloading"}
          >
            {update.status === "checking" ? "Checking…" : "Check for updates"}
          </Button>
        )}
      </div>
      {update.status === "downloading" ? (
        <div
          className="mt-2 h-[3px] overflow-hidden rounded-full bg-hover"
          role="progressbar"
          aria-label={`Downloading update${pct === null ? "" : `, ${pct}%`}`}
          aria-valuemin={0}
          aria-valuemax={100}
          aria-valuenow={pct ?? undefined}
        >
          <div
            className="h-full rounded-full bg-accent transition-[width] duration-150"
            style={{ width: pct === null ? "35%" : `${pct}%` }}
          />
        </div>
      ) : null}
      {update.status === "available" && update.notes ? (
        <div className="mt-3 rounded-[10px] border border-border bg-panel-raised px-4 py-3">
          <p className="text-[12px] font-medium text-foreground">What&rsquo;s new in {update.version}</p>
          <p className="mt-1 whitespace-pre-wrap text-[12px] leading-4 text-muted">{update.notes}</p>
        </div>
      ) : null}
      <p className="mt-2 text-[12px] leading-4 text-muted">
        DiskPilot checks once each launch and only speaks up when a new version exists. Update
        packages are cryptographically signed; an update that fails verification is never installed.
      </p>
    </div>
  );
}

export function SettingsPage() {
  const settings = useAppStore((s) => s.settings);
  const env = useAppStore((s) => s.env);
  const homeDir = useAppStore((s) => s.env?.homeDir);
  const updateSettings = useAppStore((s) => s.updateSettings);
  const settingsSaveError = useAppStore((s) => s.settingsSaveError);
  const setRecycleBinOpen = useAppStore((s) => s.setRecycleBinOpen);

  const pickRoot = async () => {
    const folder = await api.pickFolder(settings.scanRoot || undefined);
    if (folder) await updateSettings({ scanRoot: folder });
  };

  return (
    <div className="h-full overflow-y-auto px-6 py-6">
      <div className="max-w-[760px]">
        <PageHeader title="Settings" subtitle="Scanning, appearance, the optional assistant, and safety notes." showScanControl={false} />

        {settingsSaveError ? (
          <p role="alert" className="mb-4 flex items-center gap-2 rounded-[10px] border border-critical/30 bg-critical/10 px-4 py-3 text-[12px] leading-4 text-critical">
            <ShieldAlert className="size-4 shrink-0" aria-hidden="true" />
            Couldn&rsquo;t save — your change will not persist. ({settingsSaveError})
          </p>
        ) : null}

        <Section title="Scanning" description="The scan root is measured read-only. The default is your user profile folder.">
          <div className="flex items-center gap-2">
            <span className="flex h-8 flex-1 items-center rounded-md border border-border bg-panel-raised px-2.5" title={settings.scanRoot}>
              <span className="mono-path truncate text-foreground">
                {displayPath(settings.scanRoot, homeDir) || "No folder chosen"}
              </span>
            </span>
            <Button variant="secondary" onClick={() => void pickRoot()}>
              <FolderOpen className="size-4" aria-hidden="true" />
              Choose folder…
            </Button>
          </div>
          <div className="mt-3 flex items-start gap-2 rounded-[10px] border border-border bg-panel-raised px-4 py-3">
            <ShieldAlert className="mt-0.5 size-4 shrink-0 text-muted" aria-hidden="true" />
            <p className="text-[12px] leading-4 text-muted">
              Protected folders are never scanned for cleanup: C:\Windows (including WinSxS), C:\Program Files,
              C:\Program Files (x86), C:\ProgramData, C:\Recovery, and drive roots are off limits, and system
              files such as hiberfil.sys and pagefile.sys are never touched. Windows.old is reported only —
              use Disk Cleanup or Storage Sense for it.
            </p>
          </div>
          <label className="mt-4 flex cursor-pointer items-center justify-between gap-4 border-t border-border pt-3">
            <span>
              <span className="block text-[13px] font-medium text-foreground">Read text in images</span>
              <span className="block text-[12px] leading-4 text-muted">
                Find words inside screenshots and images. Reading happens on this PC (Windows
                built-in OCR); the text is never sent anywhere.
              </span>
            </span>
            <Checkbox
              checked={settings.ocrEnabled}
              onChange={(e) => void updateSettings({ ocrEnabled: e.target.checked })}
              aria-label="Read text in images"
            />
          </label>
          <label className="mt-4 flex cursor-pointer items-center justify-between gap-4 border-t border-border pt-3">
            <span>
              <span className="block text-[13px] font-medium text-foreground">Live index</span>
              <span className="block text-[12px] leading-4 text-muted">
                While DiskPilot is running, changes on the scanned drive are applied to the index
                within about a minute. Needs administrator access to the drive's change journal;
                without it, the index simply refreshes on each scan.
              </span>
            </span>
            <Checkbox
              checked={settings.liveIndex}
              onChange={(e) => void updateSettings({ liveIndex: e.target.checked })}
              aria-label="Live index"
            />
          </label>
          <ExclusionsSettings />
        </Section>

        <Section
          title="Watch"
          description="Scheduled scans and alerts run while DiskPilot is in the tray. Alerts are raised after a scheduled scan, never during one you started yourself."
        >
          <WatchSettings />
        </Section>

        <Section
          title="Automatic maintenance"
          description="A standing cleanup with a strict age floor — off by default, and every run is recorded."
        >
          <MaintenanceSettings />
        </Section>

        <Section
          title="Saved searches"
          description="Plain-language searches bookmarked from the Storage page. They always run against the latest scan."
        >
          <SavedSearchesSettings />
        </Section>

        <Section title="Appearance">
          <div className="flex items-center justify-between gap-4">
            <span>
              <span className="block text-[13px] font-medium text-foreground">Theme</span>
              <span className="block text-[12px] text-muted">Follow the Windows setting, or pin light or dark.</span>
            </span>
            <Select
              aria-label="Theme"
              value={settings.theme}
              onChange={(e) => void updateSettings({ theme: e.target.value as ThemePreference })}
              className="w-[160px]"
            >
              <option value="system">System</option>
              <option value="light">Light</option>
              <option value="dark">Dark</option>
            </Select>
          </div>
        </Section>

        <Section title="Assistant" description="Optional and disabled by default.">
          <AssistantSettings />
        </Section>

        <Section title="Recycle Bin" description="Emptying the Recycle Bin permanently removes its contents and frees the space for real.">
          <Button variant="secondary" onClick={() => setRecycleBinOpen(true)}>
            <Trash2 className="size-4" aria-hidden="true" />
            Empty Recycle Bin…
          </Button>
        </Section>

        <Section
          title="Explorer integration"
          description="Right-click any folder or drive in Windows Explorer to scan it here."
        >
          <ExplorerIntegrationSettings />
        </Section>

        <Section title="About & updates">
          <dl className="space-y-1.5 text-[12px] leading-4">
            <div className="flex gap-2"><dt className="w-[110px] text-muted">Version</dt><dd className="text-foreground">{env?.appVersion ?? "—"}</dd></div>
            <div className="flex gap-2"><dt className="w-[110px] text-muted">OS</dt><dd className="text-foreground">{env?.os ?? "—"}</dd></div>
            <div className="flex gap-2"><dt className="w-[110px] text-muted">Architecture</dt><dd className="text-foreground">{env?.arch ?? "—"}</dd></div>
            <div className="flex gap-2"><dt className="w-[110px] text-muted">Privacy</dt><dd className="text-foreground">Everything is local. No account, no telemetry. Only the optional assistant talks to the network, directly to your chosen provider.</dd></div>
          </dl>
          <UpdatesSettings />
        </Section>
      </div>
    </div>
  );
}
