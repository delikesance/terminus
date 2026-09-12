/**
 * Full-FS SFTP path helpers — the remote filesystem is browsed as an absolute
 * path tree (like Termius). `..` and `/` are allowed; the only remaining guard
 * is that a path cannot step above `/`. No session-root sandbox.
 */

export type SftpPathError = {
  kind: "SftpPathTraversal";
  message: string;
  path: string;
};

export function normalizeSftpPath(path: string): string {
  if (!path) return "/";
  const absolute = path.startsWith("/");
  const stack: string[] = [];
  for (const part of path.split("/")) {
    if (!part || part === ".") continue;
    if (part === "..") {
      if (stack.length === 0) {
        const err: SftpPathError = {
          kind: "SftpPathTraversal",
          message: `path traversal blocked: ${path}`,
          path,
        };
        throw err;
      }
      stack.pop();
      continue;
    }
    stack.push(part);
  }
  if (absolute) return stack.length ? `/${stack.join("/")}` : "/";
  return stack.length ? stack.join("/") : ".";
}

/** Resolve any path to an absolute one. Absolute paths pass through; relative paths are anchored to root (or `/`). */
export function resolveUnderRoot(root: string, path: string): string {
  const p = (path && path.trim()) || ".";
  if (p.startsWith("/")) return normalizeSftpPath(p);
  const base = (root && root !== "." ? root : "/").replace(/\/+$/, "");
  return normalizeSftpPath(`${base}/${p}`);
}

/** Show the remote path (absolute). Relative input is joined onto cwd. */
export function sftpDisplayPath(cwd: string | null | undefined, logical: string): string {
  let norm: string;
  try {
    norm = normalizeSftpPath(logical || "/");
  } catch {
    norm = logical || "/";
  }
  if (norm === "." || norm === "") return tidyAbs(cwd) || "/";
  if (norm.startsWith("/")) return norm === "/" ? "/" : norm.replace(/\/+$/, "");
  const home = tidyAbs(cwd);
  if (home) return home === "/" ? `/${norm}` : `${home}/${norm}`;
  return norm === "." ? "/" : `/${norm}`;
}

/**
 * Map a path-bar value back to an absolute remote path. Absolute values pass
 * through (`/`, `/etc`, `..`, …). Relative values are anchored onto cwd.
 */
export function logicalFromDisplayPath(
  cwd: string | null | undefined,
  _root: string,
  typed: string,
): string {
  const raw = typed.trim() || ".";
  if (raw.startsWith("/")) return normalizeSftpPath(raw);
  const home = tidyAbs(cwd);
  if (home && home !== "/") return normalizeSftpPath(`${home}/${raw}`.replace(/\/+/g, "/"));
  const joined = home === "/" ? `/${raw}` : `/${raw}`.replace(/\/+/g, "/");
  return normalizeSftpPath(joined);
}

function tidyAbs(path: string | null | undefined): string {
  if (!path) return "";
  if (path === "/") return "/";
  return path.replace(/\/+$/, "");
}

export function parentSftpPath(path: string): string | null {
  let norm: string;
  try {
    norm = normalizeSftpPath(path);
  } catch {
    return null;
  }
  if (norm === "/" || norm === ".") return null;
  const idx = norm.lastIndexOf("/");
  if (idx === 0) return "/";
  if (idx < 0) return ".";
  return norm.slice(0, idx);
}

/** Parse typed SFTP IPC errors (`{"kind":"Sftp…","message":…}`). */
export function parseSftpError(err: unknown): { kind: string; message: string } {
  const raw = typeof err === "string" ? err : err instanceof Error ? err.message : String(err);
  try {
    const parsed = JSON.parse(raw) as { kind?: string; message?: string };
    if (parsed && typeof parsed.kind === "string" && parsed.kind.startsWith("Sftp")) {
      return {
        kind: parsed.kind,
        message: typeof parsed.message === "string" ? parsed.message : raw,
      };
    }
  } catch {
    /* plain string */
  }
  if (typeof err === "object" && err && "kind" in err) {
    const o = err as { kind: string; message?: string };
    if (String(o.kind).startsWith("Sftp")) {
      return { kind: o.kind, message: o.message ?? String(err) };
    }
  }
  return { kind: "SftpIo", message: raw };
}
