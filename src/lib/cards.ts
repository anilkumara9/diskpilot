import type { CandidateFile } from "./types";

/** Evidence label shown under each card's title. These mirror the
    core detection rules in `crates/core/src/cards.rs` exactly — when
    a threshold changes there, it changes here. */
const CARD_SUBTITLES: Record<string, string> = {
  "old-installers": "Installers older than 30 days",
  "forgotten-downloads": "In Downloads, untouched for 90+ days",
  "old-screenshots": "Screenshots older than 90 days",
  "big-old-videos": "Videos over 100 MB, older than 180 days",
};

export function cardSubtitle(card: string): string {
  return CARD_SUBTITLES[card] ?? "";
}

/** Total bytes of the candidates whose ids are currently selected. */
export function selectedCandidateBytes(
  files: CandidateFile[],
  selected: ReadonlySet<string>,
): number {
  return files.reduce((acc, f) => (selected.has(f.id) ? acc + f.bytes : acc), 0);
}
