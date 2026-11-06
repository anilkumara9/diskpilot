import { useEffect, useRef, useState } from "react";
import { ArrowRight, Bot, Eye, SendHorizonal } from "lucide-react";
import * as api from "@/lib/api";
import { useAppStore } from "@/lib/store";
import type { AssistantMessage } from "@/lib/types";
import { PageHeader } from "@/components/layout/PageHeader";
import { Button } from "@/components/ui/button";
import { Textarea } from "@/components/ui/input";
import { EmptyState } from "@/components/shared/EmptyState";
import { cn } from "@/lib/utils";

const SUGGESTIONS = [
  "What is using the most space?",
  "What is safest to clean first?",
  "Which caches can I remove?",
  "Explain my largest folders.",
];

export function AssistantPage() {
  const navigate = useAppStore((s) => s.navigate);
  const setHighlighted = useAppStore((s) => s.setHighlighted);
  const [messages, setMessages] = useState<AssistantMessage[]>([]);
  const [draft, setDraft] = useState("");
  const [sending, setSending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const scrollRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    scrollRef.current?.scrollTo({ top: scrollRef.current.scrollHeight, behavior: "smooth" });
  }, [messages, sending]);

  const send = async (text: string) => {
    const content = text.trim();
    if (!content || sending) return;
    const history = messages.map((m) => ({ role: m.role, content: m.content }));
    const next: AssistantMessage[] = [...messages, { role: "user", content }];
    setMessages(next);
    setDraft("");
    setSending(true);
    setError(null);
    try {
      const reply = await api.assistantAsk(content, history);
      const assistantMessage: AssistantMessage = {
        role: "assistant",
        content: reply.content,
        highlightedRecommendationIds: reply.highlightedRecommendationIds,
      };
      setMessages([...next, assistantMessage]);
      if (reply.highlightedRecommendationIds.length > 0) {
        setHighlighted(reply.highlightedRecommendationIds);
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : "The assistant could not answer. Check the provider and API key in Settings.");
    } finally {
      setSending(false);
    }
  };

  const reviewHighlights = (ids: string[]) => {
    setHighlighted(ids);
    navigate("cleanup");
  };

  return (
    <div className="flex h-full flex-col px-6 py-5">
      <div className="mx-auto flex min-h-0 w-full max-w-[760px] flex-1 flex-col">
        <PageHeader
          title="Assistant"
          subtitle="Ask about this PC's scan results. The assistant is read-only: it can explain and point at recommendations, but it cannot delete, move, or run anything."
          showScanControl={false}
        />

        <div className="mb-3 flex items-start gap-2 rounded-[10px] border border-border bg-panel-raised px-3.5 py-2.5">
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
                    <Button key={suggestion} variant="secondary" size="sm" onClick={() => void send(suggestion)}>
                      {suggestion}
                    </Button>
                  ))}
                </div>
              }
            />
          ) : (
            <ul className="space-y-3 pb-2">
              {messages.map((message, i) => (
                <li key={i} className={cn("flex", message.role === "user" ? "justify-end" : "justify-start")}>
                  <div
                    className={cn(
                      "max-w-[86%] whitespace-pre-wrap rounded-[10px] px-3.5 py-2.5 text-[13px] leading-[18px]",
                      message.role === "user"
                        ? "bg-accent-fill text-accent-foreground"
                        : "border border-border bg-panel text-foreground",
                    )}
                  >
                    <p>{message.content}</p>
                    {message.highlightedRecommendationIds && message.highlightedRecommendationIds.length > 0 ? (
                      <Button
                        variant="secondary"
                        size="sm"
                        className="mt-2"
                        onClick={() => reviewHighlights(message.highlightedRecommendationIds ?? [])}
                      >
                        Review highlighted recommendations
                        <ArrowRight className="size-3.5" aria-hidden="true" />
                      </Button>
                    ) : null}
                  </div>
                </li>
              ))}
              {sending ? (
                <li className="flex justify-start">
                  <div className="rounded-[10px] border border-border bg-panel px-3.5 py-2.5 text-[12px] text-muted">
                    Thinking…
                  </div>
                </li>
              ) : null}
            </ul>
          )}
        </div>

        {error ? <p role="alert" className="py-2 text-[12px] text-critical">{error}</p> : null}

        <form
          className="flex items-end gap-2 border-t border-border pt-3"
          onSubmit={(e) => {
            e.preventDefault();
            void send(draft);
          }}
        >
          <Textarea
            value={draft}
            onChange={(e) => setDraft(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter" && !e.shiftKey) {
                e.preventDefault();
                void send(draft);
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
      </div>
    </div>
  );
}
