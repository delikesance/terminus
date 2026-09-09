/**
 * Minimal local OS path helpers for the "local" pane of the bidirectional
 * file transfer. Works on Windows (`C:\Users\…`, UNC `\\…`) and POSIX (`/home/…`).
 * No sandboxing — these are the user's own paths.
 */

/** Join a directory and a name with the OS separator. */
export function joinLocalPath(dir: string, name: string): string {
  if (!dir) return name;
  if (dir.endsWith("/") || dir.endsWith("\\")) return dir + name;
  const sep = isWindowsPath(dir) ? "\\" : "/";
  return dir + sep + name;
}

/** Parent directory of a local path, or `null` when at a filesystem root. */
export function parentLocalPath(path: string): string | null {
  let p = path.replace(/[\\/]+$/, "");
  if (!p) return null;
  // Drive root like `C:` / `C:\` has no parent.
  if (/^[A-Za-z]:[\\/]?$/.test(p)) return null;
  if (p === "/" || p === "\\") return null;
  const i = Math.max(p.lastIndexOf("/"), p.lastIndexOf("\\"));
  if (i < 0) return null;
  let parent = p.slice(0, i);
  if (!parent) return "/";
  // `C:` is a drive root -> normalize to `C:\`.
  if (/^[A-Za-z]:$/.test(parent)) return parent + "\\";
  return parent;
}

/** File name of a local path. */
export function fileNameOfPath(path: string): string {
  const p = path.replace(/[\\/]+$/, "");
  const i = Math.max(p.lastIndexOf("/"), p.lastIndexOf("\\"));
  if (i < 0) return p;
  return p.slice(i + 1);
}

function isWindowsPath(p: string): boolean {
  return /^[A-Za-z]:[\\/]/.test(p) || p.startsWith("\\\\");
}
