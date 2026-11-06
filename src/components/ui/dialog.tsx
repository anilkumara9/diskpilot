import {
  useCallback,
  useEffect,
  useId,
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

const FOCUSABLE_SELECTOR =
  "button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), a[href], [tabindex]:not([tabindex='-1'])";

/**
 * Stack of currently open dialogs (their registry ids, in open
 * order). Keyboard handling — Escape and the Tab trap — belongs to
 * the TOPMOST dialog only: with two dialogs stacked, one Escape
 * closes one dialog, and Tab can never wander into the dialog (or
 * page) underneath.
 */
const openDialogs: string[] = [];

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
  const registryId = useId();
  const titleId = useRef(`dlg-${Math.random().toString(36).slice(2, 9)}`).current;

  const handleKey = useCallback(
    (event: KeyboardEvent) => {
      if (openDialogs[openDialogs.length - 1] !== registryId) return;
      if (event.key === "Escape") {
        if (dismissible) onClose();
        return;
      }
      if (event.key !== "Tab") return;
      /* Focus trap: Tab cycles within the panel. Focus that somehow
         sits outside the panel (or on the panel itself) is pulled
         to the first/last focusable element instead of escaping. */
      const panel = panelRef.current;
      if (!panel) return;
      const focusable = [...panel.querySelectorAll<HTMLElement>(FOCUSABLE_SELECTOR)].filter(
        (el) => el.offsetParent !== null || el === document.activeElement,
      );
      if (focusable.length === 0) {
        event.preventDefault();
        panel.focus();
        return;
      }
      const first = focusable[0];
      const last = focusable[focusable.length - 1];
      const active = document.activeElement as HTMLElement | null;
      const inside = active !== null && panel.contains(active);
      if (event.shiftKey) {
        if (!inside || active === first || active === panel) {
          event.preventDefault();
          last.focus();
        }
      } else if (!inside || active === last || active === panel) {
        event.preventDefault();
        first.focus();
      }
    },
    [onClose, dismissible, registryId],
  );

  useEffect(() => {
    if (!open) return;
    openDialogs.push(registryId);
    document.addEventListener("keydown", handleKey);
    const previous = document.activeElement as HTMLElement | null;
    const timer = window.setTimeout(() => {
      const focusable = panelRef.current?.querySelector<HTMLElement>(
        "button:not([disabled]), input, select, textarea, [tabindex]:not([tabindex='-1'])",
      );
      (focusable ?? panelRef.current)?.focus();
    }, 30);
    return () => {
      const index = openDialogs.indexOf(registryId);
      if (index >= 0) openDialogs.splice(index, 1);
      document.removeEventListener("keydown", handleKey);
      window.clearTimeout(timer);
      previous?.focus?.();
    };
  }, [open, handleKey, registryId]);

  if (!open) return null;

  return (
    <div className="fixed inset-0 z-50" role="presentation">
      <div
        className="absolute inset-0 bg-black/50 backdrop-blur-[2px]"
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
          "dialog-in absolute left-1/2 top-1/2 flex max-h-[86vh] w-full flex-col rounded-[10px] border border-border-strong bg-panel-raised shadow-[0_24px_70px_-12px_rgba(0,0,0,0.5),0_4px_16px_rgba(0,0,0,0.24)] outline-none",
          wide ? "max-w-[720px]" : "max-w-[520px]",
        )}
      >
        <div className="flex items-start gap-3 px-5 pb-3 pt-5">
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
        <div className="min-h-0 flex-1 overflow-y-auto px-5 py-3">{children}</div>
        {footer ? (
          <div className="flex items-center justify-end gap-2 border-t border-border px-5 py-3.5">
            {footer}
          </div>
        ) : null}
      </div>
    </div>
  );
}
