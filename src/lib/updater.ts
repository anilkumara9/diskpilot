import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";

/** The update flow as the UI presents it. Every failure lands in
    `error` only for a manual check; the automatic launch check
    treats failure as silence (see the store). Update artifacts are
    signed — the shell verifies the signature against the public key
    in tauri.conf.json before anything is installed. */
export type UpdateState =
  | { status: "idle" }
  | { status: "checking" }
  | { status: "upToDate" }
  | { status: "available"; version: string; notes: string | null }
  | { status: "downloading"; version: string; downloaded: number; total: number | null }
  | { status: "readyToRestart"; version: string }
  | { status: "error" };

export const IDLE_UPDATE_STATE: UpdateState = { status: "idle" };

/** Download progress as a whole percent, or null when the total is
    unknown (the updater does not always send a content length). */
export function updateProgressPercent(downloaded: number, total: number | null): number | null {
  if (total === null || total <= 0) return null;
  const pct = Math.floor((downloaded / total) * 100);
  return Math.min(100, Math.max(0, pct));
}

/** Whether the sidebar should flag that an update is waiting.
    Only a found, not-yet-installed update (or one installed and
    awaiting restart) earns the flag — checking/downloading states
    are already visible in Settings. */
export function isUpdateNoticeable(state: UpdateState): boolean {
  return state.status === "available" || state.status === "readyToRestart";
}

/** The one-line status the About block shows for each state. */
export function updateStatusLine(state: UpdateState, currentVersion: string | null): string {
  switch (state.status) {
    case "idle":
      return "Updates are signed and verified before they install.";
    case "checking":
      return "Checking for updates…";
    case "upToDate":
      return `DiskPilot ${currentVersion ?? ""} is the latest version.`.replace("  ", " ");
    case "available":
      return `Version ${state.version} is available.`;
    case "downloading": {
      const pct = updateProgressPercent(state.downloaded, state.total);
      return pct === null
        ? `Downloading version ${state.version}…`
        : `Downloading version ${state.version}… ${pct}%`;
    }
    case "readyToRestart":
      return `Version ${state.version} is installed. Restart DiskPilot to apply it.`;
    case "error":
      return "Couldn't check for updates. Check your connection and try again.";
  }
}

let pendingUpdate: Update | null = null;

/** Ask the update endpoint whether a newer signed release exists.
    Returns the public facts, keeping the Update handle here for the
    install step. Throws when the check itself fails (offline, no
    release published yet, signature metadata unreachable). */
export async function checkForAvailableUpdate(): Promise<{ version: string; notes: string | null } | null> {
  const update = await check();
  pendingUpdate = update;
  if (!update) return null;
  return { version: update.version, notes: update.body?.trim() ? update.body : null };
}

/** Download and install the update found by the last check,
    reporting byte progress. Resolves once the install has finished
    and only a restart remains. */
export async function downloadAndInstallPending(
  onProgress: (downloaded: number, total: number | null) => void,
): Promise<void> {
  if (!pendingUpdate) throw new Error("No update is waiting to install.");
  let downloaded = 0;
  let total: number | null = null;
  await pendingUpdate.downloadAndInstall((event) => {
    switch (event.event) {
      case "Started":
        total = event.data.contentLength ?? null;
        onProgress(0, total);
        break;
      case "Progress":
        downloaded += event.data.chunkLength;
        onProgress(downloaded, total);
        break;
      case "Finished":
        onProgress(total ?? downloaded, total);
        break;
    }
  });
  pendingUpdate = null;
}

/** Restart into the freshly installed version. */
export async function restartToApplyUpdate(): Promise<void> {
  await relaunch();
}
