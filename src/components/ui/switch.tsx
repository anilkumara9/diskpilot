import { cn } from "@/lib/utils";

/**
 * The house switch: a 32×18 pill track with a 14 px knob. Off is
 * the neutral `selected` grey, on is the accent fill; the knob is
 * a white disc with a soft shadow. Reduced motion collapses the
 * slide via the global rule. Used where a control IS a state
 * (Startup entries), never as a form checkbox substitute.
 */
export function Switch({
  checked,
  onChange,
  disabled = false,
  ariaLabel,
}: {
  checked: boolean;
  onChange: (checked: boolean) => void;
  disabled?: boolean;
  ariaLabel: string;
}) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={ariaLabel}
      disabled={disabled}
      onClick={() => onChange(!checked)}
      className={cn(
        "relative h-[18px] w-8 shrink-0 rounded-full transition-colors duration-120",
        checked ? "bg-[var(--accent-fill)]" : "bg-selected ring-1 ring-border",
        disabled ? "cursor-not-allowed opacity-50" : "cursor-pointer",
      )}
    >
      <span
        aria-hidden="true"
        className={cn(
          "absolute top-[2px] size-[14px] rounded-full bg-white shadow-[0_1px_3px_rgba(0,0,0,0.3)] transition-[left] duration-120",
          checked ? "left-[16px]" : "left-[2px]",
        )}
      />
    </button>
  );
}
