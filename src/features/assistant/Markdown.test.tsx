import { act } from "react";
import { createRoot } from "react-dom/client";
import { describe, expect, it } from "vitest";
import { Markdown } from "./Markdown";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

function renderMarkdown(source: string): HTMLElement {
  const container = document.createElement("div");
  const root = createRoot(container);
  act(() => {
    root.render(<Markdown>{source}</Markdown>);
  });
  return container;
}

describe("Markdown (assistant replies)", () => {
  it("renders emphasis, lists and inline code as elements, not raw characters", () => {
    const el = renderMarkdown("The **largest** folder wins.\n\n- one\n- two\n\nRun `du` first.");
    expect(el.querySelector("strong")?.textContent).toBe("largest");
    expect(el.querySelectorAll("li")).toHaveLength(2);
    expect(el.querySelector("code")?.textContent).toBe("du");
    expect(el.textContent).not.toContain("**");
  });

  it("renders fenced code in a pre block with a copy action", () => {
    const el = renderMarkdown("```\nGet-ChildItem C:\\\n```");
    const pre = el.querySelector("pre");
    expect(pre?.textContent).toContain("Get-ChildItem");
    expect(el.textContent).toContain("Copy");
  });

  it("never renders raw HTML from the source as markup", () => {
    const el = renderMarkdown('Hello <b>world</b> <script>alert("x")</script>');
    expect(el.querySelector("b")).toBeNull();
    expect(el.querySelector("script")).toBeNull();
    expect(el.textContent).toContain("Hello");
  });
});
