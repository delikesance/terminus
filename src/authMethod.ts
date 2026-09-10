/** Host SSH auth methods — fail-closed parse (unknown ≠ password). */

export type HostAuthMethod = "key" | "password" | "gssapi";

export type ParseHostAuthOk = { ok: true; method: HostAuthMethod };
export type ParseHostAuthErr = { ok: false; error: "unknown_method"; raw: string };
export type ParseHostAuthResult = ParseHostAuthOk | ParseHostAuthErr;

export function parseHostAuthMethod(raw: unknown): ParseHostAuthResult {
  const s = typeof raw === "string" ? raw.trim().toLowerCase() : "";
  if (s === "key" || s === "password" || s === "gssapi") {
    return { ok: true, method: s };
  }
  return { ok: false, error: "unknown_method", raw: String(raw ?? "") };
}

export function hostAuthUi(method: string): {
  segment: "key" | "password" | "gssapi";
  showKey: boolean;
  showPassword: boolean;
} {
  const parsed = parseHostAuthMethod(method);
  if (parsed.ok && parsed.method === "gssapi") {
    return { segment: "gssapi", showKey: false, showPassword: false };
  }
  if (parsed.ok && parsed.method === "password") {
    return { segment: "password", showKey: false, showPassword: true };
  }
  return { segment: "key", showKey: true, showPassword: false };
}
