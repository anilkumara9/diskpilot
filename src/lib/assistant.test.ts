import { describe, expect, it } from "vitest";
import { resolveHighlights } from "./assistant";
import type { Recommendation } from "./types";

function rec(id: string): Recommendation {
  return {
    id,
    title: `Recommendation ${id}`,
    paths: [`C:\\Data\\${id}`],
    bytes: 1024,
    tier: "likelySafe",
    methods: [],
    whyDetected: "",
    whySafe: "",
    risks: "",
    confidence: "high",
    source: "test",
  };
}

describe("resolveHighlights", () => {
  const recommendations = [rec("a"), rec("b"), rec("c")];

  it("resolves ids in the order the assistant gave them", () => {
    expect(resolveHighlights(["c", "a"], recommendations).map((r) => r.id)).toEqual(["c", "a"]);
  });

  it("skips ids that no longer resolve", () => {
    expect(resolveHighlights(["a", "gone", "b"], recommendations).map((r) => r.id)).toEqual(["a", "b"]);
  });

  it("returns an empty list for missing or empty ids", () => {
    expect(resolveHighlights(undefined, recommendations)).toEqual([]);
    expect(resolveHighlights([], recommendations)).toEqual([]);
    expect(resolveHighlights(["a"], [])).toEqual([]);
  });
});
