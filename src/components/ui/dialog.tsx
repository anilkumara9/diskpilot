import {
  useCallback,
  useEffect,
  useRef,
  type ReactNode,
} from "react";
import { X } from "lucide-react";
import { cn } from "@/lib/utils";
import { Button } from "./button";

export interface DialogProps {
  open: boolean;
  onClose: () => void;
  title: string;
  description?: string;
  children: ReactNode;
  footer?: ReactNode;
  wide?: boolean;
  /** When false the dialog cannot be dismissed by backdrop click. Default true. */
  dismissible?: boolean;
}

export function Dialog({
  open,
  onClose,
  title,
  description,
  children,
  footer,
  wide,
  dismissible = true,
}: DialogProps) {
  const panelRef = useRef<HTMLDivElement>(null);
  const titleId = useRef(`dlg-${Math.random().toString(36).slice(2, 9)}`).current;

  const handleKey = useCallback(
    (event: KeyboardEvent) => {
      if (event.key === "Escape" && dismissible) onClose();
    },
    [onClose, dismissible],
  );

  useEffect(() => {
    if (!open) return;
    document.addEventListener("keydown", handleKey);
    const previous = document.activeElement as HTMLElement | null;
    const timer = window.setTimeout(() => {
      const focusable = panelRef.current?.querySelector<HTMLElement>(
        "button:not([disabled]), input, select, textarea, [tabindex]:not([tabindex='-1'])",
      );
      (focusable ?? panelRef.current)?.focus();
    }, 30);
    return () => {
      document.removeEventListener("keydown", handleKey);
      window.clearTimeout(timer);
      previous?.focus?.();
    };
  }, [open, handleKey]);

  if (!open) return null;

  return (
    <div className="fixed inset-0 z-50" role="presentation">
      <div
        className="absolute inset-0 bg-black/40"
        aria-hidden="true"
        onClick={() => dismissible && onClose()}
      />
      <div
        ref={panelRef}
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        tabIndex={-1}
        className={cn(
          "dialog-in absolute left-1/2 top-1/2 flex max-h-[86vh] w-full flex-col rounded-[10px] border border-border bg-panel shadow-[0_16px_48px_rgba(0,0,0,0.28)] outline-none",
          wide ? "max-w-[720px]" : "max-w-[520px]",
        )}
      >
        <div className="flex items-start gap-3 px-5 pb-2 pt-4">
          <div className="min-w-0 flex-1">
            <h2 id={titleId} className="text-[15px] font-semibold leading-5 text-foreground">
              {title}
            </h2>
            {description ? (
              <p className="mt-0.5 text-[12px] leading-4 text-muted">{description}</p>
            ) : null}
          </div>
          {dismissible ? (
            <Button variant="ghost" size="icon" onClick={onClose} aria-label="Close dialog">
              <X className="size-4" />
            </Button>
          ) : null}
        </div>
        <div className="min-h-0 flex-1 overflow-y-auto px-5 py-2">{children}</div>
        {footer ? (
          <div className="flex items-center justify-end gap-2 border-t border-border px-5 py-3">
            {footer}
          </div>
        ) : null}
      </div>
    </div>
  );
}
