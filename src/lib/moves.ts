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

/**
 * Normalize a destination directory so the previewed target and
 * the value sent to the shell are the same string. A bare drive
 * root — "D:" or "D:\" — becomes the canonical "D:\": a bare
 * "D:" is a drive-RELATIVE path (whatever folder that drive last
 * used), a different location entirely. Other directories lose
 * their trailing separators.
 */
export function normalizeDestDir(destDir: string): string {
  const slashed = destDir.trim().replace(/\//g, "\\");
  const root = /^([A-Za-z]:)\\*$/.exec(slashed);
  if (root) return `${root[1].toUpperCase()}\\`;
  return slashed.replace(/\\+$/, "");
}

/** Where a folder lands when moved into `destDir`: the
    destination directory plus the folder's own name. */
export function relocationTarget(sourcePath: string, destDir: string): string {
  const dir = normalizeDestDir(destDir);
  const name = basename(sourcePath);
  /* A drive root has no folder name of its own — basename("C:\")
     is "C:" — so there is nothing to append: the preview must
     never invent "D:\C:". The destination itself is the honest
     preview (the shell refuses a root source regardless). */
  if (!name || /^[A-Za-z]:$/.test(name)) return dir;
  return /^[A-Za-z]:\\$/.test(dir) ? `${dir}${name}` : `${dir}\\${name}`;
}
