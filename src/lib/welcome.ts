import type { AppSettings } from "./types";

/**
 * The first-run welcome shows exactly when the user has never
 * finished (or skipped) setup AND has never completed a scan —
 * a returning user with a scan on disk goes straight to the app,
 * even if their settings predate the `onboarded` field in a state
 * the shell still reports as not onboarded. Startup gating (the
 * settings must actually have loaded) lives in the store and in
 * App, not here: this is the pure decision over loaded state.
 */
export function shouldShowWelcome(
  settings: Pick<AppSettings, "onboarded">,
  hasSummary: boolean,
): boolean {
  return !settings.onboarded && !hasSummary;
}
