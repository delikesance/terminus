/**
 * Safe SFTP path helpers — block `..` escape outside the session root.
 * Mirrors crates/terminus-core/src/sftp_path.rs (fail closed).
 */

export type SftpPathError = {
  kind: "SftpPathTraversal";
  message: string;
  path: string;
};

export function normalizeSftpPath(path: string): string {
  if (!path) return ".";
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

export function resolveUnderRoot(root: string, path: string): string {
  const rootN = normalizeSftpPath(root);
  let candidate: string;
  if (path.startsWith("/")) {
    candidate = normalizeSftpPath(path);
  } else if (rootN === ".") {
    candidate = normalizeSftpPath(path);
  } else if (rootN === "/") {
    candidate = normalizeSftpPath(`/${path}`);
  } else {
    candidate = normalizeSftpPath(`${rootN.replace(/\/$/, "")}/${path.replace(/^\//, "")}`);
  }
  if (!isUnderRoot(rootN, candidate)) {
    const err: SftpPathError = {
      kind: "SftpPathTraversal",
      message: `path traversal blocked: ${path}`,
      path,
    };
    throw err;
  }
  return candidate;
}

/** Show the remote folder as an absolute path when the session cwd is known. */
export function sftpDisplayPath(cwd: string | null | undefined, logical: string): string {
  let norm: string;
  try {
    norm = normalizeSftpPath(logical || ".");
  } catch {
    norm = logical || ".";
  }
  const home = tidyAbs(cwd);
  if (!home) return norm;
  if (norm === "." || norm === "") return home;
  if (norm.startsWith("/")) return norm;
  return home === "/" ? `/${norm}` : `${home}/${norm}`;
}

/**
 * Map a path-bar value (absolute under cwd, or relative) back to a logical
 * session path that `resolveUnderRoot` can send over IPC.
 */
export function logicalFromDisplayPath(
  cwd: string | null | undefined,
  root: string,
  typed: string,
): string {
  const raw = typed.trim() || ".";
  const home = tidyAbs(cwd);
  if (home) {
    if (raw === home || raw === `${home}/`) return resolveUnderRoot(root, ".");
    const prefix = home === "/" ? "/" : `${home}/`;
    if (raw.startsWith(prefix)) {
      return resolveUnderRoot(root, raw.slice(prefix.length) || ".");
    }
    if (raw.startsWith("/")) {
      const err: SftpPathError = {
        kind: "SftpPathTraversal",
        message: `path traversal blocked: ${raw}`,
        path: raw,
      };
      throw err;
    }
  }
  return resolveUnderRoot(root, raw);
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

function isUnderRoot(root: string, path: string): boolean {
  if (root === "/") return path.startsWith("/");
  if (root === ".") return !path.startsWith("/");
  return path === root || path.startsWith(`${root}/`);
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
