import { describe, expect, it } from "vitest";
import {
  addSavedSearch,
  filtersAreEmpty,
  filtersWithoutChip,
  MAX_SAVED_SEARCHES,
  removeSavedSearch,
} from "./search";
import type { QueryChip, SearchFilters } from "./types";

const FULL: SearchFilters = {
  kinds: ["pdf"],
  minBytes: 5_000_000,
  maxBytes: null,
  modifiedAfterMs: 1_700_000_000_000,
  modifiedBeforeMs: 1_700_100_000_000,
  source: "gmail",
  terms: ["board", "deck"],
};

function chip(kind: QueryChip["kind"]): QueryChip {
  return { kind, label: kind };
}

describe("filtersWithoutChip", () => {
  it("clears exactly the chip's dimension, keeping the rest", () => {
    expect(filtersWithoutChip(FULL, chip("kind"))).toEqual({ ...FULL, kinds: [] });
    expect(filtersWithoutChip(FULL, chip("size"))).toEqual({
      ...FULL,
      minBytes: null,
      maxBytes: null,
    });
    expect(filtersWithoutChip(FULL, chip("date"))).toEqual({
      ...FULL,
      modifiedAfterMs: null,
      modifiedBeforeMs: null,
    });
    expect(filtersWithoutChip(FULL, chip("source"))).toEqual({ ...FULL, source: null });
    expect(filtersWithoutChip(FULL, chip("term"))).toEqual({ ...FULL, terms: [] });
  });

  it("does not mutate the input filters", () => {
    const input: SearchFilters = { ...FULL, kinds: ["pdf"], terms: ["board"] };
    const result = filtersWithoutChip(input, chip("kind"));
    expect(input.kinds).toEqual(["pdf"]);
    expect(result?.kinds).toEqual([]);
    expect(result?.terms).toEqual(["board"]);
  });

  it("returns null for a chip kind this client does not know", () => {
    const future = { kind: "owner", label: "Owner" } as unknown as QueryChip;
    expect(filtersWithoutChip(FULL, future)).toBeNull();
  });

  it("clearing every dimension leaves empty filters", () => {
    let filters = FULL;
    for (const kind of ["kind", "size", "date", "source", "term"] as const) {
      const next = filtersWithoutChip(filters, chip(kind));
      if (next) filters = next;
    }
    expect(filtersAreEmpty(filters)).toBe(true);
    expect(filtersAreEmpty(FULL)).toBe(false);
  });
});

describe("saved searches", () => {
  it("adds newest first, trimmed, and de-duplicates case-insensitively", () => {
    const start = ["images over 5 MB"];
    const once = addSavedSearch(start, "  pdfs from last week  ");
    expect(once).toEqual(["pdfs from last week", "images over 5 MB"]);
    const twice = addSavedSearch(once, "PDFS FROM LAST WEEK");
    expect(twice).toEqual(["PDFS FROM LAST WEEK", "images over 5 MB"]);
  });

  it("ignores blank queries", () => {
    expect(addSavedSearch(["a"], "   ")).toEqual(["a"]);
    expect(addSavedSearch([], "")).toEqual([]);
  });

  it("caps the list, dropping the oldest", () => {
    let list: string[] = [];
    for (let i = 0; i < MAX_SAVED_SEARCHES + 3; i += 1) {
      list = addSavedSearch(list, `query ${i}`);
    }
    expect(list).toHaveLength(MAX_SAVED_SEARCHES);
    expect(list[0]).toBe(`query ${MAX_SAVED_SEARCHES + 2}`);
    expect(list).not.toContain("query 0");
  });

  it("removes by exact stored text", () => {
    expect(removeSavedSearch(["a", "B", "c"], "B")).toEqual(["a", "c"]);
    expect(removeSavedSearch(["a"], "missing")).toEqual(["a"]);
  });
});
