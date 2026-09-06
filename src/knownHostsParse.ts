/**
 * Fail-closed OpenSSH known_hosts parser for host import (no secrets).
 * Bad lines are skipped and counted; valid hostnames become connection stubs.
 */

export type KnownHostStub = {
  hostname: string;
  port: number;
};

export type KnownHostsParseResult = {
  hosts: KnownHostStub[];
  errors: number;
};

const KEY_TYPE_RE =
  /^(ssh-ed25519|ssh-rsa|ssh-dss|ecdsa-sha2-nistp(256|384|521)|sk-ssh-ed25519@openssh\.com|sk-ecdsa-sha2-nistp256@openssh\.com)$/;

/** Parse `[host]:port` or bare host; returns null if unusable (hashed / empty). */
export function parseHostPattern(token: string): KnownHostStub | null {
  const t = token.trim();
  if (!t) return null;
  if (t.startsWith("|")) return null; // hashed known_hosts — cannot recover hostname

  if (t.startsWith("[") && t.includes("]:")) {
    const end = t.indexOf("]:");
    const host = t.slice(1, end).trim();
    const portRaw = t.slice(end + 2).trim();
    const port = Number(portRaw);
    if (!host || !Number.isInteger(port) || port < 1 || port > 65535) return null;
    return { hostname: host, port };
  }

  // Bare hostname / IP (no port → 22). Reject tokens that look like key material.
  if (/\s/.test(t) || t.includes("@")) return null;
  return { hostname: t, port: 22 };
}

/**
 * Parse known_hosts file contents.
 * - Blank / `#` comments: ignored (not errors)
 * - `@cert-authority` / `@revoked` / hashed / malformed: skip + error
 * - Comma-separated host patterns: one stub per pattern
 * - Dedupes by hostname:port (first wins)
 */
export function parseKnownHosts(text: string): KnownHostsParseResult {
  const hosts: KnownHostStub[] = [];
  const seen = new Set<string>();
  let errors = 0;

  const lines = text.split(/\r?\n/);
  for (const raw of lines) {
    const line = raw.trim();
    if (!line || line.startsWith("#")) continue;

    // Markers we do not import as connection hosts
    if (line.startsWith("@")) {
      errors += 1;
      continue;
    }

    const parts = line.split(/\s+/);
    if (parts.length < 3) {
      errors += 1;
      continue;
    }

    const [hostField, keyType, keyData] = parts;
    if (!KEY_TYPE_RE.test(keyType) || !keyData || keyData.length < 8) {
      errors += 1;
      continue;
    }

    const patterns = hostField.split(",").filter(Boolean);
    if (!patterns.length) {
      errors += 1;
      continue;
    }

    let lineOk = false;
    for (const pattern of patterns) {
      const stub = parseHostPattern(pattern);
      if (!stub) {
        errors += 1;
        continue;
      }
      const key = `${stub.hostname.toLowerCase()}:${stub.port}`;
      if (seen.has(key)) {
        lineOk = true;
        continue;
      }
      seen.add(key);
      hosts.push(stub);
      lineOk = true;
    }
    if (!lineOk) {
      // all patterns on the line failed — already counted per pattern
    }
  }

  return { hosts, errors };
}
