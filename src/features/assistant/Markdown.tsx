import { isValidElement, type ComponentPropsWithoutRef, type MouseEvent, type ReactNode } from "react";
import ReactMarkdown, { type Components } from "react-markdown";
import remarkGfm from "remark-gfm";
import * as api from "@/lib/api";
import { cn } from "@/lib/utils";
import { CopyButton } from "@/components/shared/CopyButton";

/**
 * Markdown rendering for assistant replies, mapped onto the design
 * system (see docs/DESIGN.md, "Assistant messages"). Safety posture:
 * react-markdown's default behaviour — raw HTML in the source is never
 * rendered as markup — and links never navigate the webview: absolute
 * URLs open through the app's external opener, anything else is inert.
 */

function extractText(node: ReactNode): string {
  if (typeof node === "string" || typeof node === "number") return String(node);
  if (Array.isArray(node)) return node.map(extractText).join("");
  if (isValidElement<{ children?: ReactNode }>(node)) return extractText(node.props.children);
  return "";
}

function openLink(href: string | undefined, event: MouseEvent) {
  event.preventDefault();
  if (href && /^(https?:|mailto:)/i.test(href)) {
    void api.openExternal(href).catch(() => undefined);
  }
}

const components: Components = {
  p: ({ children }: ComponentPropsWithoutRef<"p">) => <p className="my-2 leading-[19px]">{children}</p>,
  h1: ({ children }: ComponentPropsWithoutRef<"h1">) => (
    <h1 className="mb-1.5 mt-4 text-[15px] font-semibold leading-5">{children}</h1>
  ),
  h2: ({ children }: ComponentPropsWithoutRef<"h2">) => (
    <h2 className="mb-1.5 mt-4 text-[14px] font-semibold leading-5">{children}</h2>
  ),
  h3: ({ children }: ComponentPropsWithoutRef<"h3">) => (
    <h3 className="mb-1 mt-3 text-[13px] font-semibold leading-[18px]">{children}</h3>
  ),
  h4: ({ children }: ComponentPropsWithoutRef<"h4">) => (
    <h4 className="mb-1 mt-3 text-[13px] font-semibold leading-[18px] text-muted">{children}</h4>
  ),
  ul: ({ children, className }: ComponentPropsWithoutRef<"ul">) => (
    <ul
      className={cn(
        "my-2 list-disc space-y-1 pl-5 marker:text-subtle",
        className?.includes("contains-task-list") && "list-none pl-0",
        className,
      )}
    >
      {children}
    </ul>
  ),
  ol: ({ children, className }: ComponentPropsWithoutRef<"ol">) => (
    <ol className={cn("my-2 list-decimal space-y-1 pl-5 marker:text-subtle", className)}>{children}</ol>
  ),
  li: ({ children, className }: ComponentPropsWithoutRef<"li">) => (
    <li className={cn("leading-[18px]", className?.includes("task-list-item") && "list-none", className)}>
      {children}
    </li>
  ),
  input: ({ type, checked, disabled }: ComponentPropsWithoutRef<"input">) =>
    type === "checkbox" ? (
      <input
        type="checkbox"
        checked={checked}
        disabled={disabled ?? true}
        readOnly
        className="mr-1.5 size-3.5 translate-y-[2px] accent-[var(--accent-fill)]"
      />
    ) : null,
  strong: ({ children }: ComponentPropsWithoutRef<"strong">) => (
    <strong className="font-semibold text-foreground">{children}</strong>
  ),
  em: ({ children }: ComponentPropsWithoutRef<"em">) => <em>{children}</em>,
  a: ({ children, href }: ComponentPropsWithoutRef<"a">) => (
    <a
      href={href}
      onClick={(e) => openLink(href, e)}
      className="font-medium text-accent underline-offset-2 hover:underline"
    >
      {children}
    </a>
  ),
  code: ({ children, className }: ComponentPropsWithoutRef<"code">) => (
    <code
      className={cn(
        "rounded-[4px] bg-panel-raised px-1 py-px font-mono text-[12px] text-foreground",
        className,
      )}
    >
      {children}
    </code>
  ),
  pre: ({ children }: ComponentPropsWithoutRef<"pre">) => (
    <div className="relative my-2.5">
      <pre className="overflow-x-auto rounded-[10px] border border-border bg-panel-raised px-3.5 py-3 font-mono text-[12px] leading-[17px] [&_code]:rounded-none [&_code]:bg-transparent [&_code]:p-0">
        {children}
      </pre>
      <div className="absolute right-1.5 top-1.5">
        <CopyButton text={extractText(children)} />
      </div>
    </div>
  ),
  blockquote: ({ children }: ComponentPropsWithoutRef<"blockquote">) => (
    <blockquote className="my-2.5 border-l-2 border-accent/40 pl-3 text-muted">{children}</blockquote>
  ),
  table: ({ children }: ComponentPropsWithoutRef<"table">) => (
    <div className="my-2.5 overflow-x-auto">
      <table className="w-full border-collapse text-[12.5px] leading-4">{children}</table>
    </div>
  ),
  th: ({ children }: ComponentPropsWithoutRef<"th">) => (
    <th className="border-b border-border-strong px-2 py-1.5 text-left font-semibold text-muted">{children}</th>
  ),
  td: ({ children }: ComponentPropsWithoutRef<"td">) => (
    <td className="border-b border-border px-2 py-1.5 align-top">{children}</td>
  ),
  hr: () => <hr className="my-3 border-border" />,
  img: ({ alt }: ComponentPropsWithoutRef<"img">) =>
    alt ? <span className="text-muted">[Image: {alt}]</span> : null,
};

export function Markdown({ children }: { children: string }) {
  return (
    <div className="min-w-0 break-words text-[13px] text-foreground [&>*:first-child]:mt-0 [&>*:last-child]:mb-0">
      <ReactMarkdown remarkPlugins={[remarkGfm]} components={components}>
        {children}
      </ReactMarkdown>
    </div>
  );
}
