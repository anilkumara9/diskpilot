import { describe, expect, it } from "vitest";
import { cardSubtitle, selectedCandidateBytes } from "./cards";
import type { CandidateFile } from "./types";

describe("card subtitles", () => {
  it("labels every card with its evidence rule", () => {
    expect(cardSubtitle("old-installers")).toBe("Installers older than 30 days");
    expect(cardSubtitle("forgotten-downloads")).toBe("In Downloads, untouched for 90+ days");
    expect(cardSubtitle("old-screenshots")).toBe("Screenshots older than 90 days");
    expect(cardSubtitle("big-old-videos")).toBe("Videos over 100 MB, older than 180 days");
  });

  it("has no label for an unknown card id", () => {
    expect(cardSubtitle("something-else")).toBe("");
  });
});

describe("selected candidate bytes", () => {
  const files: CandidateFile[] = [
    { id: "a", path: "/x/a.msi", name: "a.msi", bytes: 100, modifiedMs: 1 },
    { id: "b", path: "/x/b.msi", name: "b.msi", bytes: 250, modifiedMs: 1 },
    { id: "c", path: "/x/c.msi", name: "c.msi", bytes: 40, modifiedMs: 1 },
  ];

  it("sums only the selected files", () => {
    expect(selectedCandidateBytes(files, new Set(["a", "c"]))).toBe(140);
    expect(selectedCandidateBytes(files, new Set(["a", "b", "c"]))).toBe(390);
  });

  it("is zero when nothing is selected", () => {
    expect(selectedCandidateBytes(files, new Set())).toBe(0);
    expect(selectedCandidateBytes([], new Set(["a"]))).toBe(0);
  });
});
