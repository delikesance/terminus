/** Vault identity kinds — fail-closed typed parse (no AcceptAll). */

export type IdentityKind = "key" | "password" | "agent";

export type ParseIdentityKindOk = { ok: true; kind: IdentityKind };
export type ParseIdentityKindErr = {
  ok: false;
  error: "unknown_kind";
  raw: string;
};
export type ParseIdentityKindResult = ParseIdentityKindOk | ParseIdentityKindErr;

export type IdentityKeyParseErr = {
  kind: "IdentityKeyInvalid";
  reason: string;
};

export function parseIdentityKind(raw: unknown): ParseIdentityKindResult {
  const s = typeof raw === "string" ? raw.trim().toLowerCase() : "";
  if (s === "key" || s === "password" || s === "agent") {
    return { ok: true, kind: s };
  }
  return { ok: false, error: "unknown_kind", raw: String(raw ?? "") };
}

/** Infer display kind from stored fields when `kind` is missing (legacy rows). */
export function inferIdentityKind(identity: {
  kind?: string | null;
  private_key?: string | null;
  passphrase?: string | null;
}): IdentityKind {
  const parsed = parseIdentityKind(identity.kind);
  if (parsed.ok) return parsed.kind;
  if (identity.private_key && identity.private_key.trim()) return "key";
  if (identity.passphrase) return "password";
  return "key";
}

/** Parse structured IdentityKeyInvalid IPC without leaking secrets. */
export function parseIdentityKeyError(raw: unknown): IdentityKeyParseErr | null {
  if (typeof raw !== "string") return null;
  try {
    const parsed = JSON.parse(raw) as Partial<IdentityKeyParseErr>;
    if (parsed.kind === "IdentityKeyInvalid" && typeof parsed.reason === "string") {
      return { kind: "IdentityKeyInvalid", reason: parsed.reason };
    }
  } catch {
    /* plain string */
  }
  return null;
}
