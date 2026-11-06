import { basename } from "./format";
import type { DriveInfo } from "./types";

/**
 * Pure path/drive logic for the Move folder flow, kept out of the
 * dialog so it is testable: which drive a path lives on, which
 * drives may receive it, and where exactly it will land.
 */

/** Normalize a Windows path for comparison: forward slashes
    become backslashes and trailing separators are dropped (except
    on a bare drive root). Case is folded by the callers that
    compare. */
function normalize(path: string): string {
  const slashed = path.replace(/\//g, "\\");
  if (/^[A-Za-z]:\\$/.test(slashed)) return slashed;
  return slashed.replace(/\\+$/, "");
}

/** The drive root a path lives on ("C:\" — letter uppercased to
    the canonical form), or null when the path has no drive
    letter (relative paths, UNC shares). */
export function driveRootOf(path: string): string | null {
  const match = /^([A-Za-z]:)[\\/]/.exec(path) ?? /^([A-Za-z]:)$/.exec(path);
  return match ? `${match[1].toUpperCase()}\\` : null;
}

/** True when two paths name the same location, ignoring slash
    style, trailing separators, and case (Windows semantics). */
export function sameLocation(a: string, b: string): boolean {
  return normalize(a).toLowerCase() === normalize(b).toLowerCase();
}

/**
 * The drives a folder may move TO: every drive except the one the
 * folder already lives on (moving within a drive frees nothing).
 * When the source's drive cannot be determined, all drives are
 * offered — the shell validates the move regardless.
 */
export function destinationDrives(drives: DriveInfo[], sourcePath: string): DriveInfo[] {
  const sourceRoot = driveRootOf(sourcePath);
  if (!sourceRoot) return [...drives];
  return drives.filter((drive) => !sameLocation(drive.rootPath, sourceRoot));
}

/** Where a folder lands when moved into `destDir`: the
    destination directory plus the folder's own name. */
export function relocationTarget(sourcePath: string, destDir: string): string {
  const name = basename(sourcePath);
  const dir = destDir.replace(/[\\/]+$/, "");
  return `${dir}\\${name}`;
}
