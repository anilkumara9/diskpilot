import { useEffect, useState } from "react";
import { Check, FolderOpen, KeyRound, ShieldAlert, Trash2, X } from "lucide-react";
import * as api from "@/lib/api";
import { useAppStore } from "@/lib/store";
import { displayPath } from "@/lib/format";
import { removeSavedSearch } from "@/lib/search";
import { exclusionsToText, parseExclusions, sameExclusions } from "@/lib/settings";
import type { AssistantProvider, ThemePreference } from "@/lib/types";
import { PageHeader } from "@/components/layout/PageHeader";
import { Button } from "@/components/ui/button";
import { Checkbox, Input, Select, Textarea } from "@/components/ui/input";
import { Section } from "@/components/ui/section";

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

  useEffect(() => {
    let live = true;
    void api.hasApiKey(settings.assistantProvider).then((value) => { if (live) setHasKey(value); });
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
          <div>
            <label htmlFor="assistant-provider" className="mb-1 block text-[12px] font-medium text-foreground">
              Provider
            </label>
            <Select
              id="assistant-provider"
              value={settings.assistantProvider}
              onChange={(e) => void updateSettings({ assistantProvider: e.target.value as AssistantProvider })}
              className="max-w-[280px]"
            >
              {PROVIDERS.map((provider) => (
                <option key={provider.id} value={provider.id}>{provider.label}</option>
              ))}
            </Select>
            <p className="mt-1 text-[12px] text-muted">
              {PROVIDERS.find((p) => p.id === settings.assistantProvider)?.note} HTTPS only, no redirects.
            </p>
          </div>

          <div>
            <label htmlFor="assistant-key" className="mb-1 flex items-center gap-1.5 text-[12px] font-medium text-foreground">
              <KeyRound className="size-3.5" aria-hidden="true" />
              API key
            </label>
            <div className="flex max-w-[480px] items-center gap-2">
              <Input
                id="assistant-key"
                type="password"
                autoComplete="off"
                value={keyDraft}
                onChange={(e) => setKeyDraft(e.target.value)}
                placeholder={hasKey ? "A key is saved — enter a new one to replace it" : "Paste your API key"}
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
    await updateSettings({ exclusions: parsed });
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

export function SettingsPage() {
  const settings = useAppStore((s) => s.settings);
  const env = useAppStore((s) => s.env);
  const homeDir = useAppStore((s) => s.env?.homeDir);
  const updateSettings = useAppStore((s) => s.updateSettings);
  const setRecycleBinOpen = useAppStore((s) => s.setRecycleBinOpen);

  const pickRoot = async () => {
    const folder = await api.pickFolder(settings.scanRoot || undefined);
    if (folder) await updateSettings({ scanRoot: folder });
  };

  return (
    <div className="h-full overflow-y-auto px-6 py-6">
      <div className="max-w-[760px]">
        <PageHeader title="Settings" subtitle="Scanning, appearance, the optional assistant, and safety notes." showScanControl={false} />

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
          <ExclusionsSettings />
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

        <Section title="About">
          <dl className="space-y-1.5 text-[12px] leading-4">
            <div className="flex gap-2"><dt className="w-[110px] text-muted">Version</dt><dd className="text-foreground">{env?.appVersion ?? "—"}</dd></div>
            <div className="flex gap-2"><dt className="w-[110px] text-muted">OS</dt><dd className="text-foreground">{env?.os ?? "—"}</dd></div>
            <div className="flex gap-2"><dt className="w-[110px] text-muted">Architecture</dt><dd className="text-foreground">{env?.arch ?? "—"}</dd></div>
            <div className="flex gap-2"><dt className="w-[110px] text-muted">Privacy</dt><dd className="text-foreground">Everything is local. No account, no telemetry. Only the optional assistant talks to the network, directly to your chosen provider.</dd></div>
          </dl>
        </Section>
      </div>
    </div>
  );
}
