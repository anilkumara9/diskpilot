/* Formatting helpers. Sizes use decimal units (1 GB = 10^9 bytes) to match
   how Windows reports disk capacity in File Explorer properties. */

const BYTE_UNITS = ["B", "KB", "MB", "GB", "TB", "PB"] as const;

export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes < 0) return "0 B";
  if (bytes === 0) return "0 B";
  let value = bytes;
  let unit = 0;
  while (value >= 1000 && unit < BYTE_UNITS.length - 1) {
    value /= 1000;
    unit += 1;
  }
  const decimals = value >= 100 ? 0 : value >= 10 ? 1 : 2;
  const text = value.toFixed(unit === 0 ? 0 : decimals).replace(/\.0+$|(\.\d)0$/, "$1");
  return `${text} ${BYTE_UNITS[unit]}`;
}

export function formatCount(n: number): string {
  return new Intl.NumberFormat("en-US").format(Math.trunc(n));
}

/** A byte delta with an explicit sign: "+212 MB" when space grew,
    "−1.2 GB" (minus sign) when it shrank, "0 B" when unchanged. Used
    by the scan-over-scan comparison, where the sign carries meaning. */
export function formatSignedBytes(delta: number): string {
  if (!Number.isFinite(delta) || delta === 0) return "0 B";
  return `${delta > 0 ? "+" : "−"}${formatBytes(Math.abs(delta))}`;
}

/** A count delta with an explicit sign: "+1,234" / "−56" / "0". */
export function formatSignedCount(delta: number): string {
  if (!Number.isFinite(delta) || delta === 0) return "0";
  return `${delta > 0 ? "+" : "−"}${formatCount(Math.abs(delta))}`;
}

export function formatPercent(fraction: number): string {
  if (!Number.isFinite(fraction)) return "0%";
  const pct = fraction * 100;
  if (pct > 0 && pct < 0.1) return "<0.1%";
  return `${pct.toFixed(pct >= 10 ? 0 : 1)}%`;
}

export function formatDuration(ms: number): string {
  if (!Number.isFinite(ms) || ms < 0) return "0s";
  const totalSeconds = Math.round(ms / 1000);
  if (totalSeconds < 60) return `${totalSeconds}s`;
  const minutes = Math.floor(totalSeconds / 60);
  const seconds = totalSeconds % 60;
  if (minutes < 60) return seconds ? `${minutes}m ${seconds}s` : `${minutes}m`;
  const hours = Math.floor(minutes / 60);
  const rest = minutes % 60;
  return rest ? `${hours}h ${rest}m` : `${hours}h`;
}

export function formatRelative(timestampMs: number, nowMs: number = Date.now()): string {
  const diff = nowMs - timestampMs;
  if (diff < 0) return "just now";
  const seconds = Math.floor(diff / 1000);
  if (seconds < 10) return "just now";
  if (seconds < 60) return `${seconds} seconds ago`;
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return minutes === 1 ? "1 minute ago" : `${minutes} minutes ago`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return hours === 1 ? "1 hour ago" : `${hours} hours ago`;
  const days = Math.floor(hours / 24);
  if (days < 30) return days === 1 ? "1 day ago" : `${days} days ago`;
  return formatDate(timestampMs);
}

export function formatDate(timestampMs: number): string {
  return new Intl.DateTimeFormat("en-US", {
    year: "numeric",
    month: "short",
    day: "numeric",
    hour: "numeric",
    minute: "2-digit",
  }).format(new Date(timestampMs));
}

export function formatDateShort(timestampMs: number): string {
  return new Intl.DateTimeFormat("en-US", {
    month: "short",
    day: "numeric",
    hour: "numeric",
    minute: "2-digit",
  }).format(new Date(timestampMs));
}

/** Replace the user profile prefix with ~ for display. Handles backslashes. */
export function displayPath(path: string, homeDir?: string | null): string {
  if (!path) return path;
  const normalized = path.replace(/\//g, "\\");
  if (homeDir) {
    const home = homeDir.replace(/\//g, "\\").replace(/\\+$/, "");
    if (normalized.toLowerCase() === home.toLowerCase()) return "~";
    if (normalized.toLowerCase().startsWith(home.toLowerCase() + "\\")) {
      return "~" + normalized.slice(home.length);
    }
  }
  return path;
}

/** Basename that understands both Windows and POSIX separators. */
export function basename(path: string): string {
  const trimmed = path.replace(/[\\/]+$/, "");
  const idx = Math.max(trimmed.lastIndexOf("\\"), trimmed.lastIndexOf("/"));
  return idx >= 0 ? trimmed.slice(idx + 1) : trimmed;
}

/** Parent path that understands both separators. */
export function parentPath(path: string): string {
  const trimmed = path.replace(/[\\/]+$/, "");
  const idx = Math.max(trimmed.lastIndexOf("\\"), trimmed.lastIndexOf("/"));
  if (idx < 0) return trimmed;
  if (idx === 2 && trimmed[1] === ":") return trimmed.slice(0, 3);
  return trimmed.slice(0, idx) || trimmed.slice(0, idx + 1);
}

export function joinDisplay(...parts: string[]): string {
  return parts
    .map((p, i) => (i === 0 ? p.replace(/[\\/]+$/, "") : p.replace(/^[\\/]+|[\\/]+$/g, "")))
    .filter(Boolean)
    .join("\\");
}

export function truncateMiddle(text: string, max = 72): string {
  if (text.length <= max) return text;
  const keep = Math.floor((max - 1) / 2);
  return `${text.slice(0, keep)}…${text.slice(text.length - keep)}`;
}
