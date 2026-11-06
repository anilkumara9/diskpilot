import { describe, expect, it } from "vitest";
import { tabIndexForKey } from "./tabs";

describe("tabIndexForKey", () => {
  it("arrows move one tab and wrap at the ends", () => {
    expect(tabIndexForKey(0, "ArrowRight", 3)).toBe(1);
    expect(tabIndexForKey(2, "ArrowRight", 3)).toBe(0);
    expect(tabIndexForKey(0, "ArrowLeft", 3)).toBe(2);
    expect(tabIndexForKey(2, "ArrowLeft", 3)).toBe(1);
  });

  it("Home and End jump to the ends", () => {
    expect(tabIndexForKey(1, "Home", 3)).toBe(0);
    expect(tabIndexForKey(1, "End", 3)).toBe(2);
  });

  it("ignores other keys and empty strips", () => {
    expect(tabIndexForKey(0, "Enter", 3)).toBeNull();
    expect(tabIndexForKey(0, "ArrowDown", 3)).toBeNull();
    expect(tabIndexForKey(0, "ArrowRight", 0)).toBeNull();
  });
});
