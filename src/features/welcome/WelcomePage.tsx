import { useEffect, useState, type ComponentType } from "react";
import { BellRing, FolderOpen, FolderTree, ShieldCheck } from "lucide-react";
import * as api from "@/lib/api";
import { useAppStore } from "@/lib/store";
import { displayPath } from "@/lib/format";
import type { AutoScanSchedule } from "@/lib/types";
import { Button } from "@/components/ui/button";
import { Checkbox, Input, Select } from "@/components/ui/input";
import { Section } from "@/components/ui/section";

/**
 * First-run welcome: one calm scrolling page — what DiskPilot does,
 * what it should watch, and the alert schedule — ending in a single
 * primary action. Shown instead of the app shell only when setup
 * was never finished and no scan exists yet (see shouldShowWelcome).
 *
 * Every choice on the page is a draft: "Start first scan" persists
 * them all (with `onboarded`) through the normal settings save
 * flow, while "Skip setup" persists `onboarded` and nothing else.
 * The Watch rows mirror the Settings Watch field structure exactly
 * (same rows, same fixed number-field layout) but stay unbound
 * until the primary action — including the sign-in toggle, which
 * only registers with Windows when setup is confirmed.
 */

const VALUE_ROWS: { icon: ComponentType<{ className?: string }>; title: string; line: string }[] = [
  {
    icon: FolderTree,
    title: "See where your space goes",
    line: "Every folder mapped by size, with plain-language search across your files.",
  },
  {
    icon: ShieldCheck,
    title: "Clean up safely",
    line: "Nothing is deleted without an exact plan you review first — and cleanup goes to the Recycle Bin, so it can be undone.",
  },
  {
    icon: BellRing,
    title: "Get warned before the disk fills",
    line: "Watch checks in the background and alerts you while there is still room to act.",
  },
];

/** Parse a threshold draft the way Settings commits it: a finite,
    non-negative number wins; anything else keeps the saved value. */
function parseThreshold(draft: string, fallback: number): number {
  const value = Number(draft);
  return Number.isFinite(value) && value >= 0 ? value : fallback;
}

export function WelcomePage() {
  const settings = useAppStore((s) => s.settings);
  const homeDir = useAppStore((s) => s.env?.homeDir);
  const updateSettings = useAppStore((s) => s.updateSettings);
  const navigate = useAppStore((s) => s.navigate);
  const startScan = useAppStore((s) => s.startScan);

  const [rootDraft, setRootDraft] = useState(settings.scanRoot);
  /* The page's pitch is "get warned before the disk fills", so the
     schedule draft starts at Daily — an untouched draft must not
     silently mean "no watch at all". A schedule the user already
     chose (weekly) is respected; only the never-configured "off"
     becomes Daily. The global settings default is unchanged. */
  const [scheduleDraft, setScheduleDraft] = useState<AutoScanSchedule>(
    settings.autoScan === "off" ? "daily" : settings.autoScan,
  );
  const [lowDraft, setLowDraft] = useState(() => String(settings.lowSpaceAlertGb));
  const [growthDraft, setGrowthDraft] = useState(() => String(settings.growthAlertGb));
  /* The sign-in toggle starts from the registration actually in
     effect, exactly as in Settings; null until the shell answers. */
  const [autostartDraft, setAutostartDraft] = useState<boolean | null>(null);
  const [busy, setBusy] = useState(false);
  const [saveError, setSaveError] = useState<string | null>(null);

  useEffect(() => {
    let live = true;
    void api.getAutostart().then((value) => {
      if (live) setAutostartDraft(value);
    });
    return () => {
      live = false;
    };
  }, []);

  const pickRoot = async () => {
    const folder = await api.pickFolder(rootDraft || undefined);
    if (folder) setRootDraft(folder);
  };

  const startFirstScan = async () => {
    setBusy(true);
    setSaveError(null);
    let autoStart = settings.autoStart;
    const wantedAutostart = autostartDraft ?? settings.autoStart;
    if (wantedAutostart !== settings.autoStart) {
      autoStart = await api.setAutostart(wantedAutostart);
    }
    /* A failed save must not silently proceed: `onboarded` is what
       dismisses this page, so leaving on a failed save would strand
       the user's choices AND their only route back here. Stay, and
       say why. */
    const saved = await updateSettings({
      onboarded: true,
      scanRoot: rootDraft,
      autoScan: scheduleDraft,
      lowSpaceAlertGb: parseThreshold(lowDraft, settings.lowSpaceAlertGb),
      growthAlertGb: parseThreshold(growthDraft, settings.growthAlertGb),
      autoStart,
    });
    if (!saved) {
      setSaveError("Your setup couldn't be saved — please try again. Nothing was changed.");
      setBusy(false);
      return;
    }
    navigate("overview");
    await startScan();
  };

  const skip = async () => {
    setBusy(true);
    setSaveError(null);
    const saved = await updateSettings({ onboarded: true });
    if (!saved) {
      setSaveError("Setup couldn't be marked as finished — please try again.");
      setBusy(false);
      return;
    }
    navigate("overview");
  };

  return (
    <div className="flex h-full flex-col bg-background text-foreground">
      {/* The window stays draggable while the shell is hidden. */}
      <div data-tauri-drag-region className="h-[38px] w-full shrink-0" aria-hidden="true" />
      <div className="min-h-0 flex-1 overflow-y-auto px-6 pb-10">
        <div className="mx-auto w-full max-w-[640px]">
          <header className="pt-4">
            <img
              src="/logo.svg"
              alt=""
              width={52}
              height={52}
              className="rounded-[12px]"
              aria-hidden="true"
            />
            <h1 className="mt-5 text-[28px] font-semibold leading-8 text-foreground">
              Welcome to DiskPilot
            </h1>
            <p className="mt-2 max-w-[520px] text-[13px] leading-[18px] text-muted">
              DiskPilot shows you exactly what is taking up space on this PC, helps you clear it
              safely, and keeps watch afterwards. Everything stays on this PC — no account, no
              uploads.
            </p>
          </header>

          <ul className="mt-7 space-y-4">
            {VALUE_ROWS.map((row) => (
              <li key={row.title} className="flex items-start gap-3">
                <span className="flex size-9 shrink-0 items-center justify-center rounded-[10px] bg-panel-raised ring-1 ring-border">
                  <row.icon className="size-[18px] text-accent" aria-hidden="true" />
                </span>
                <span>
                  <span className="block text-[13px] font-semibold leading-[18px] text-foreground">
                    {row.title}
                  </span>
                  <span className="mt-0.5 block text-[12px] leading-4 text-muted">{row.line}</span>
                </span>
              </li>
            ))}
          </ul>

          <Section
            title="What should DiskPilot watch?"
            description="The folder measured by scans. Your user profile is the usual choice — protected system folders are always left alone."
          >
            <div className="flex items-center gap-2">
              <span
                className="flex h-8 flex-1 items-center rounded-md border border-border bg-panel-raised px-2.5"
                title={rootDraft}
              >
                <span className="mono-path truncate text-foreground">
                  {displayPath(rootDraft, homeDir) || "No folder chosen"}
                </span>
              </span>
              <Button variant="secondary" onClick={() => void pickRoot()}>
                <FolderOpen className="size-4" aria-hidden="true" />
                Choose folder…
              </Button>
            </div>
          </Section>

          <Section
            title="Watch & alerts"
            description="Scheduled scans and alerts run while DiskPilot is in the tray. You can change all of this later in Settings."
          >
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
                  value={scheduleDraft}
                  onChange={(e) => setScheduleDraft(e.target.value as AutoScanSchedule)}
                  className="w-[160px] shrink-0"
                >
                  <option value="off">Off</option>
                  <option value="daily">Daily</option>
                  <option value="weekly">Weekly</option>
                </Select>
              </div>

              <div className="flex items-center justify-between gap-4">
                <label htmlFor="welcome-low-space-alert" className="text-[13px] font-medium text-foreground">
                  Alert when free space drops below
                </label>
                <span className="flex shrink-0 items-center gap-2">
                  <Input
                    id="welcome-low-space-alert"
                    type="number"
                    min={0}
                    step={1}
                    value={lowDraft}
                    onChange={(e) => setLowDraft(e.target.value)}
                    className="w-24"
                  />
                  <span className="w-[132px] text-[12px] text-muted">GB</span>
                </span>
              </div>

              <div className="flex items-center justify-between gap-4">
                <label htmlFor="welcome-growth-alert" className="text-[13px] font-medium text-foreground">
                  Alert when the disk grows by more than
                </label>
                <span className="flex shrink-0 items-center gap-2">
                  <Input
                    id="welcome-growth-alert"
                    type="number"
                    min={0}
                    step={1}
                    value={growthDraft}
                    onChange={(e) => setGrowthDraft(e.target.value)}
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
                  checked={autostartDraft ?? settings.autoStart}
                  onChange={(e) => setAutostartDraft(e.target.checked)}
                  aria-label="Start DiskPilot when you sign in to Windows"
                />
              </label>
            </div>
          </Section>

          <div className="mt-9 border-t border-border pt-5">
            {saveError ? (
              <p role="alert" className="pb-3 text-[12px] leading-4 text-critical">
                {saveError}
              </p>
            ) : null}
            <div className="flex items-center gap-3">
              <Button
                size="lg"
                variant="primary"
                disabled={busy || !rootDraft}
                onClick={() => void startFirstScan()}
              >
                Start first scan
              </Button>
              <Button variant="ghost" disabled={busy} onClick={() => void skip()}>
                Skip setup
              </Button>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
