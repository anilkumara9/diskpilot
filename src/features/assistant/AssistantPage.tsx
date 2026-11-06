import { useEffect, useRef, useState } from "react";
import { AlertTriangle, ArrowRight, Bot, Eye, RotateCcw, SendHorizonal } from "lucide-react";
import * as api from "@/lib/api";
import { useAppStore } from "@/lib/store";
import type { AssistantMessage, Recommendation } from "@/lib/types";
import { formatBytes, formatTimeOfDay } from "@/lib/format";
import { resolveHighlights } from "@/lib/assistant";
import { PageHeader } from "@/components/layout/PageHeader";
import { Button } from "@/components/ui/button";
import { Textarea } from "@/components/ui/input";
import { EmptyState } from "@/components/shared/EmptyState";
import { CopyButton } from "@/components/shared/CopyButton";
import { TierBadge } from "@/components/shared/TierBadge";
import { Markdown } from "./Markdown";

const SUGGESTIONS = [
  "What is using the most space?",
  "What is safest to clean first?",
  "Which caches can I remove?",
  "Explain my largest folders.",
];

/** The assistant mark: the app's logo glyph in a small raised circle. */
function AssistantMark() {
  return (
    <span
      aria-hidden="true"
      className="mt-px flex size-6 shrink-0 items-center justify-center rounded-full border border-border bg-panel-raised"
    >
      <img src="/logo.svg" alt="" width={14} height={14} className="rounded-[4px]" />
    </span>
  );
}

/** The recommendations one reply highlighted, as compact rows inside a
    raised container. Rows (and the footer action) reuse the existing
    review flow: jump to Cleanup with exactly those ids selected. */
function HighlightList({
  recommendations,
  onReview,
}: {
  recommendations: Recommendation[];
  onReview: (ids: string[]) => void;
}) {
  return (
    <div className="mt-2.5 rounded-[10px] border border-border bg-panel-raised px-3 py-2.5">
      <p className="text-[12px] font-semibold leading-4">Worth a look</p>
      <ul className="mt-1 divide-y divide-border">
        {recommendations.map((rec) => (
          <li key={rec.id}>
            <button
              type="button"
              onClick={() => onReview([rec.id])}
              className="-mx-1 flex w-[calc(100%+8px)] items-center gap-2 rounded-[6px] px-1 py-1.5 text-left hover:bg-hover"
            >
              <TierBadge tier={rec.tier} />
              <span className="min-w-0 flex-1 truncate text-[12.5px] leading-4">{rec.title}</span>
              <span className="tabular shrink-0 text-[12px] leading-4 text-muted">{formatBytes(rec.bytes)}</span>
            </button>
          </li>
        ))}
      </ul>
      <div className="mt-2 flex justify-end">
        <Button variant="secondary" size="sm" onClick={() => onReview(recommendations.map((r) => r.id))}>
          Review in Cleanup
          <ArrowRight className="size-3.5" aria-hidden="true" />
        </Button>
      </div>
    </div>
  );
}

export function AssistantPage() {
  const navigate = useAppStore((s) => s.navigate);
  const setHighlighted = useAppStore((s) => s.setHighlighted);
  const recommendations = useAppStore((s) => s.recommendations);
  const [messages, setMessages] = useState<AssistantMessage[]>([]);
  const [draft, setDraft] = useState("");
  const [sending, setSending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const scrollRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    scrollRef.current?.scrollTo({ top: scrollRef.current.scrollHeight, behavior: "smooth" });
  }, [messages, sending]);

  /** Ask for a reply to the last message in `base` (always the user's).
      The history sent to the core is everything before it — on a retry
      `base` is the unchanged message list, so the payload is identical
      and no bubble is duplicated. */
  const ask = async (base: AssistantMessage[]) => {
    const content = base[base.length - 1]?.content ?? "";
    const history = base.slice(0, -1).map((m) => ({ role: m.role, content: m.content }));
    setSending(true);
    setError(null);
    try {
      const reply = await api.assistantAsk(content, history);
      const assistantMessage: AssistantMessage = {
        role: "assistant",
        content: reply.content,
        highlightedRecommendationIds: reply.highlightedRecommendationIds,
        sentAtMs: Date.now(),
      };
      setMessages([...base, assistantMessage]);
      if (reply.highlightedRecommendationIds.length > 0) {
        setHighlighted(reply.highlightedRecommendationIds);
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : "The assistant could not answer. Check the provider and API key in Settings.");
    } finally {
      setSending(false);
    }
  };

  const send = (text: string) => {
    const content = text.trim();
    if (!content || sending) return;
    const userMessage: AssistantMessage = { role: "user", content, sentAtMs: Date.now() };
    const next = [...messages, userMessage];
    setMessages(next);
    setDraft("");
    void ask(next);
  };

  const retry = () => {
    const last = messages[messages.length - 1];
    if (!last || last.role !== "user" || sending) return;
    void ask(messages);
  };

  const reviewHighlights = (ids: string[]) => {
    setHighlighted(ids);
    navigate("cleanup");
  };

  return (
    <div className="flex h-full flex-col px-6 py-6">
      <div className="flex min-h-0 w-full max-w-[760px] flex-1 flex-col">
        <PageHeader
          title="Assistant"
          subtitle="Ask about this PC's scan results. The assistant is read-only: it can explain and point at recommendations, but it cannot delete, move, or run anything."
          showScanControl={false}
        />

        <div className="mb-3 flex items-start gap-2 rounded-[10px] border border-border bg-panel-raised px-4 py-3">
          <Eye className="mt-0.5 size-4 shrink-0 text-muted" aria-hidden="true" />
          <p className="text-[12px] leading-4 text-muted">
            Read-only by design. The assistant sees categories, largest items, recommendations, Docker usage,
            and developer tool summaries — paths are shown with your home folder as ~, private folders appear
            only as totals, and file contents are never read or sent. Requests go directly to your chosen
            provider over HTTPS.
          </p>
        </div>

        <div ref={scrollRef} className="min-h-0 flex-1 overflow-y-auto pr-1" aria-live="polite">
          {messages.length === 0 ? (
            <EmptyState
              icon={Bot}
              title="Ask about your storage"
              description="Try one of these, or ask your own question about the latest scan."
              action={
                <div className="flex flex-wrap justify-center gap-2">
                  {SUGGESTIONS.map((suggestion) => (
                    <Button key={suggestion} variant="secondary" size="sm" onClick={() => send(suggestion)}>
                      {suggestion}
                    </Button>
                  ))}
                </div>
              }
            />
          ) : (
            <ul className="space-y-5 pb-2">
              {messages.map((message, i) => {
                if (message.role === "user") {
                  return (
                    <li key={i} className="flex flex-col items-end">
                      <div className="max-w-[86%] whitespace-pre-wrap rounded-[10px] bg-accent-fill px-3.5 py-2.5 text-[13px] leading-[18px] text-accent-foreground">
                        <p>{message.content}</p>
                      </div>
                      {message.sentAtMs ? (
                        <span className="mt-1 pr-1 text-[11px] leading-[14px] text-subtle">
                          {formatTimeOfDay(message.sentAtMs)}
                        </span>
                      ) : null}
                    </li>
                  );
                }
                const highlights = resolveHighlights(message.highlightedRecommendationIds, recommendations);
                return (
                  <li key={i} className="group flex gap-2.5">
                    <AssistantMark />
                    <div className="min-w-0 flex-1">
                      <Markdown>{message.content}</Markdown>
                      {highlights.length > 0 ? (
                        <HighlightList recommendations={highlights} onReview={reviewHighlights} />
                      ) : null}
                      <div className="mt-1.5 flex items-center gap-1">
                        {message.sentAtMs ? (
                          <span className="text-[11px] leading-[14px] text-subtle">
                            {formatTimeOfDay(message.sentAtMs)}
                          </span>
                        ) : null}
                        <span className="opacity-0 transition-opacity duration-[120ms] focus-within:opacity-100 group-hover:opacity-100">
                          <CopyButton text={message.content} />
                        </span>
                      </div>
                    </div>
                  </li>
                );
              })}
              {sending ? (
                <li className="flex gap-2.5">
                  <AssistantMark />
                  <div className="flex items-center gap-2 pt-1">
                    <span className="flex gap-1" aria-hidden="true">
                      <span className="thinking-dot size-1.5 rounded-full bg-muted" />
                      <span className="thinking-dot size-1.5 rounded-full bg-muted" />
                      <span className="thinking-dot size-1.5 rounded-full bg-muted" />
                    </span>
                    <span className="text-[12px] leading-4 text-muted">Thinking…</span>
                  </div>
                </li>
              ) : null}
            </ul>
          )}
        </div>

        {error && !sending ? (
          <div
            role="alert"
            className="mb-3 mt-1 flex items-start gap-2 rounded-[10px] border border-critical/30 bg-critical/10 px-4 py-3"
          >
            <AlertTriangle className="mt-px size-4 shrink-0 text-critical" aria-hidden="true" />
            <p className="min-w-0 flex-1 text-[12px] leading-4 text-critical">{error}</p>
            <Button variant="secondary" size="sm" className="shrink-0" onClick={retry}>
              <RotateCcw className="size-3.5" aria-hidden="true" />
              Retry
            </Button>
          </div>
        ) : null}

        <div className="border-t border-border pt-3">
          <form
            className="flex items-end gap-2"
            onSubmit={(e) => {
              e.preventDefault();
              send(draft);
            }}
          >
            <Textarea
              value={draft}
              onChange={(e) => setDraft(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter" && !e.shiftKey) {
                  e.preventDefault();
                  send(draft);
                }
              }}
              placeholder="Ask about your storage…"
              aria-label="Message the assistant"
              className="min-h-[40px] flex-1 resize-none"
              rows={1}
            />
            <Button type="submit" variant="primary" disabled={!draft.trim() || sending}>
              <SendHorizonal className="size-4" aria-hidden="true" />
              Send
            </Button>
          </form>
          <p className="mt-1.5 text-[11px] leading-[14px] text-subtle">Enter to send · Shift+Enter for a new line</p>
        </div>
      </div>
    </div>
  );
}
