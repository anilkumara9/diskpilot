import { cn } from "@/lib/utils";

const PALETTE = [
  "#5856D6", "#2A78D6", "#13906F", "#D24B84", "#E0601F",
  "#6A52D9", "#2D8A3A", "#B07F00", "#0E7C86", "#8A4FFF",
];

function colorFor(name: string): string {
  let hash = 0;
  for (let i = 0; i < name.length; i += 1) hash = (hash * 31 + name.charCodeAt(i)) >>> 0;
  return PALETTE[hash % PALETTE.length];
}

/** Initial-letter tile. No external logos are ever fetched or drawn. */
export function AppIcon({ name, size = 28, className }: { name: string; size?: number; className?: string }) {
  const letter = (name.trim().charAt(0) || "?").toUpperCase();
  return (
    <span
      aria-hidden="true"
      className={cn("inline-flex shrink-0 items-center justify-center rounded-md font-semibold text-white", className)}
      style={{
        width: size,
        height: size,
        background: colorFor(name),
        fontSize: Math.round(size * 0.46),
      }}
    >
      {letter}
    </span>
  );
}
