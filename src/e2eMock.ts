/**
 * Browser-only Tauri IPC mock for Playwright against vite preview.
 * Active only when VITE_E2E=1 and real Tauri is absent.
 * Normal release builds never enable this.
 */

import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";
import { isTauri } from "@tauri-apps/api/core";

type Host = {
  id: string;
  name: string;
  hostname: string;
  port: number;
  username: string;
  auth_method: string;
  password?: string | null;
  identity_id?: string | null;
  group_id?: string | null;
  tags: string[];
  notes: string;
  created_at: string;
  updated_at: string;
  deleted_at?: string | null;
};

type Group = {
  id: string;
  name: string;
  parent_id?: string | null;
  created_at: string;
  updated_at: string;
  deleted_at?: string | null;
};

type SessionInfo = {
  id: string;
  title: string;
  kind: string;
  host_id?: string | null;
};

type SyncStatus = {
  configured: boolean;
  url?: string | null;
  last_sync?: string | null;
  last_error?: string | null;
  state: string;
  sync_secrets?: boolean;
};

type Identity = {
  id: string;
  name: string;
  kind?: string | null;
  public_key?: string | null;
  private_key?: string | null;
  passphrase?: string | null;
  created_at?: string;
  updated_at?: string;
  deleted_at?: string | null;
};

type SftpEntry = {
  name: string;
  path: string;
  is_dir: boolean;
  size: number;
  mtime?: number | null;
};

type SftpNode = {
  name: string;
  is_dir: boolean;
  content: Uint8Array;
  mtime: number;
  children: Map<string, SftpNode>;
};

type Db = {
  hosts: Host[];
  groups: Group[];
  sessions: SessionInfo[];
  identities: Identity[];
  connections: Map<string, string>;
  appearance: Record<string, unknown>;
  sync: SyncStatus;
  /** Per-host virtual SFTP trees keyed by host id. */
  sftp: Map<string, SftpNode>;
  /** Force next sftp_* call to fail with typed IPC JSON. */
  sftpForceError: string | null;
};

const stamp = () => new Date().toISOString();

const GRAPHITE = {
  id: "graphite",
  name: "Graphite",
  background: "#2c2c2e",
  foreground: "#f5f5f7",
  cursor: "#0a84ff",
  selection_background: "#0a84ff55",
  black: "#48484a",
  red: "#ff6961",
  green: "#32d74b",
  yellow: "#ffd426",
  blue: "#64d2ff",
  magenta: "#bf5af2",
  cyan: "#70d7ff",
  white: "#e5e5ea",
  bright_black: "#8e8e93",
  bright_red: "#ff453a",
  bright_green: "#30d158",
  bright_yellow: "#ffd60a",
  bright_blue: "#0a84ff",
  bright_magenta: "#da8fff",
  bright_cyan: "#5ac8f5",
  bright_white: "#ffffff",
};

const DEFAULT_APPEARANCE = {
  font_family: "IBM Plex Mono",
  font_size: 14,
  line_height: 1.0,
  letter_spacing: 0,
  cursor_style: "block",
  cursor_blink: true,
  scrollback: 20000,
  renderer: "auto",
  padding: 10,
  opacity: 1,
  custom_css: "",
  theme_id: "graphite",
  ligatures: true,
};

function seedFixtureGroup(db: Db) {
  if (db.groups.some((g) => g.id === "test-group-id")) return;
  const ts = stamp();
  db.groups.push({
    id: "test-group-id",
    name: "Test Group",
    parent_id: null,
    created_at: ts,
    updated_at: ts,
    deleted_at: null,
  });
  for (let i = 0; i < 2; i++) {
    const id = `test-group-host-${i}`;
    db.hosts.push({
      id,
      name: `Grouped Host ${i}`,
      hostname: `grouped-${i}.example.com`,
      port: 22,
      username: "test",
      auth_method: "key",
      password: null,
      identity_id: null,
      group_id: "test-group-id",
      tags: [],
      notes: "",
      created_at: ts,
      updated_at: ts,
      deleted_at: null,
    });
    db.connections.set(id, "disconnected");
  }
}

function createDb(): Db {
  const db: Db = {
    hosts: [],
    groups: [],
    sessions: [],
    identities: [],
    connections: new Map(),
    appearance: { ...DEFAULT_APPEARANCE },
    sync: {
      configured: false,
      url: null,
      last_sync: null,
      last_error: null,
      state: "unconfigured",
      sync_secrets: false,
    },
    sftp: new Map(),
    sftpForceError: null,
  };
  seedFixtureGroup(db);
  return db;
}

function ensureHost(db: Db, hostId: string): Host {
  const found = db.hosts.find((h) => h.id === hostId);
  if (found) return found;
  const ts = stamp();
  const host: Host = {
    id: hostId,
    name: hostId,
    hostname: `${hostId}.example.com`,
    port: 22,
    username: "test",
    auth_method: "key",
    password: null,
    identity_id: null,
    group_id: null,
    tags: [],
    notes: "",
    created_at: ts,
    updated_at: ts,
    deleted_at: null,
  };
  db.hosts.push(host);
  return host;
}

function runtimes(db: Db) {
  const counts = new Map<string, number>();
  for (const s of db.sessions) {
    if (!s.host_id) continue;
    counts.set(s.host_id, (counts.get(s.host_id) ?? 0) + 1);
  }
  return db.hosts
    .filter((h) => !h.deleted_at)
    .map((h) => ({
      host_id: h.id,
      connection: db.connections.get(h.id) ?? "disconnected",
      open_count: counts.get(h.id) ?? 0,
    }));
}

function argsOf(payload: unknown): Record<string, unknown> {
  if (!payload || typeof payload !== "object") return {};
  return payload as Record<string, unknown>;
}

function sftpNow(): number {
  return Math.floor(Date.now() / 1000);
}

function makeDir(name: string): SftpNode {
  return { name, is_dir: true, content: new Uint8Array(), mtime: sftpNow(), children: new Map() };
}

function makeFile(name: string, content: string | Uint8Array): SftpNode {
  const bytes = typeof content === "string" ? new TextEncoder().encode(content) : content;
  return { name, is_dir: false, content: bytes, mtime: sftpNow(), children: new Map() };
}

function ensureSftpRoot(db: Db, hostId: string): SftpNode {
  let root = db.sftp.get(hostId);
  if (!root) {
    root = makeDir(".");
    root.children.set("docs", makeDir("docs"));
    root.children.get("docs")!.children.set("readme.txt", makeFile("readme.txt", "hello sftp"));
    root.children.set("notes.txt", makeFile("notes.txt", "notes"));
    db.sftp.set(hostId, root);
  }
  return root;
}

/** Mirror backend normalize — throw typed IPC JSON on traversal. */
function mockNormalize(path: string): string {
  if (!path) return ".";
  const absolute = path.startsWith("/");
  const stack: string[] = [];
  for (const part of path.split("/")) {
    if (!part || part === ".") continue;
    if (part === "..") {
      if (!stack.length) {
        throw JSON.stringify({
          kind: "SftpPathTraversal",
          message: `path traversal blocked: ${path}`,
          path,
        });
      }
      stack.pop();
      continue;
    }
    stack.push(part);
  }
  if (absolute) return stack.length ? `/${stack.join("/")}` : "/";
  return stack.length ? stack.join("/") : ".";
}

function mockResolve(root: string, path: string): string {
  const rootN = mockNormalize(root);
  let candidate: string;
  if (path.startsWith("/")) candidate = mockNormalize(path);
  else if (rootN === ".") candidate = mockNormalize(path);
  else if (rootN === "/") candidate = mockNormalize(`/${path}`);
  else candidate = mockNormalize(`${rootN.replace(/\/$/, "")}/${path.replace(/^\//, "")}`);
  const under =
    rootN === "/"
      ? candidate.startsWith("/")
      : rootN === "."
        ? !candidate.startsWith("/")
        : candidate === rootN || candidate.startsWith(`${rootN}/`);
  if (!under) {
    throw JSON.stringify({
      kind: "SftpPathTraversal",
      message: `path traversal blocked: ${path}`,
      path,
    });
  }
  return candidate;
}

function splitRel(path: string): string[] {
  const n = mockNormalize(path);
  if (n === "." || n === "/") return [];
  return n.replace(/^\//, "").split("/").filter(Boolean);
}

function getNode(root: SftpNode, path: string): SftpNode | null {
  const parts = splitRel(path);
  let cur: SftpNode = root;
  for (const p of parts) {
    const next = cur.children.get(p);
    if (!next) return null;
    cur = next;
  }
  return cur;
}

function parentAndName(path: string): { parent: string; name: string } {
  const n = mockNormalize(path);
  if (n === "." || n === "/") return { parent: n, name: "" };
  const parts = splitRel(n);
  const name = parts.pop()!;
  const parent = n.startsWith("/")
    ? parts.length
      ? `/${parts.join("/")}`
      : "/"
    : parts.length
      ? parts.join("/")
      : ".";
  return { parent, name };
}

function listEntries(root: SftpNode, path: string): SftpEntry[] {
  const node = getNode(root, path);
  if (!node) {
    throw JSON.stringify({ kind: "SftpNotFound", message: `list: no such file: ${path}` });
  }
  if (!node.is_dir) {
    throw JSON.stringify({ kind: "SftpIo", message: `list: not a directory: ${path}` });
  }
  const base = mockNormalize(path);
  const out: SftpEntry[] = [];
  for (const child of node.children.values()) {
    const childPath =
      base === "/"
        ? `/${child.name}`
        : base === "."
          ? child.name
          : `${base}/${child.name}`;
    out.push({
      name: child.name,
      path: childPath,
      is_dir: child.is_dir,
      size: child.is_dir ? 0 : child.content.byteLength,
      mtime: child.mtime,
    });
  }
  out.sort((a, b) => Number(b.is_dir) - Number(a.is_dir) || a.name.localeCompare(b.name));
  return out;
}

function maybeSftpForce(db: Db): void {
  if (db.sftpForceError) {
    const err = db.sftpForceError;
    db.sftpForceError = null;
    throw err;
  }
}

/** Install mock IPC. No-op outside VITE_E2E or when real Tauri is present. */
export function installE2eMock(): void {
  if (import.meta.env.VITE_E2E !== "1") return;
  if (typeof window === "undefined") return;
  if (isTauri()) return;

  const db = createDb();
  mockWindows("main");
  mockIPC(
    (cmd, payload) => {
      const args = argsOf(payload);

      switch (cmd) {
        case "themes_list":
          return [GRAPHITE];
        case "appearance_get":
          return db.appearance;
        case "appearance_set":
          db.appearance = { ...db.appearance, ...(args.appearance as object) };
          return null;
        case "keybindings_get":
          return {};
        case "hosts_list":
          return db.hosts.filter((h) => !h.deleted_at);
        case "hosts_runtime":
          return runtimes(db);
        case "hosts_upsert": {
          const host = args.host as Host;
          const idx = db.hosts.findIndex((h) => h.id === host.id);
          if (idx >= 0) db.hosts[idx] = host;
          else db.hosts.push(host);
          return host;
        }
        case "hosts_delete": {
          const id = String(args.id);
          db.hosts = db.hosts.filter((h) => h.id !== id);
          db.connections.delete(id);
          db.sftp.delete(id);
          return null;
        }
        case "groups_list":
          return db.groups;
        case "groups_upsert": {
          const group = args.group as Group;
          const idx = db.groups.findIndex((g) => g.id === group.id);
          if (idx >= 0) db.groups[idx] = group;
          else db.groups.push(group);
          // Soft-delete: detach hosts when deleted_at set
          if (group.deleted_at) {
            for (const h of db.hosts.filter((h) => h.group_id === group.id)) {
              h.group_id = null;
              h.updated_at = stamp();
            }
          }
          return group;
        }
        case "groups_delete": {
          const id = String(args.id);
          const g = db.groups.find((x) => x.id === id);
          if (g) {
            g.deleted_at = stamp();
            g.updated_at = stamp();
          }
          for (const h of db.hosts.filter((h) => h.group_id === id)) {
            h.group_id = null;
            h.updated_at = stamp();
          }
          return null;
        }
        case "identities_list":
          return db.identities.filter((i) => !i.deleted_at);
        case "identities_upsert": {
          const identity = { ...(args.identity as Identity) };
          identity.kind = identity.kind || "key";
          const idx = db.identities.findIndex((i) => i.id === identity.id);
          if (idx >= 0) db.identities[idx] = identity;
          else db.identities.push(identity);
          return identity;
        }
        case "identities_delete": {
          const id = String(args.id);
          const row = db.identities.find((i) => i.id === id);
          if (row) {
            row.deleted_at = stamp();
            row.updated_at = stamp();
          }
          return null;
        }
        case "snippets_list":
        case "history_search":
        case "ssh_default_keys":
        case "forwards_list":
          return [];
        case "sftp_list": {
          maybeSftpForce(db);
          const hostId = String(args.hostId ?? args.host_id ?? "");
          const path = String(args.path ?? ".");
          const root = String(args.root ?? (path.startsWith("/") ? "/" : "."));
          const safe = mockResolve(root, path);
          ensureHost(db, hostId);
          return listEntries(ensureSftpRoot(db, hostId), safe);
        }
        case "sftp_read": {
          maybeSftpForce(db);
          const hostId = String(args.hostId ?? args.host_id ?? "");
          const path = String(args.path ?? "");
          const root = String(args.root ?? (path.startsWith("/") ? "/" : "."));
          const safe = mockResolve(root, path);
          const node = getNode(ensureSftpRoot(db, hostId), safe);
          if (!node || node.is_dir) {
            throw JSON.stringify({ kind: "SftpNotFound", message: `read: no such file: ${safe}` });
          }
          return Array.from(node.content);
        }
        case "sftp_write": {
          maybeSftpForce(db);
          const hostId = String(args.hostId ?? args.host_id ?? "");
          const path = String(args.path ?? "");
          const root = String(args.root ?? (path.startsWith("/") ? "/" : "."));
          const safe = mockResolve(root, path);
          const data = args.data as number[] | Uint8Array;
          const bytes = data instanceof Uint8Array ? data : Uint8Array.from(data ?? []);
          const { parent, name } = parentAndName(safe);
          if (!name) throw JSON.stringify({ kind: "SftpIo", message: "write: invalid path" });
          const rootNode = ensureSftpRoot(db, hostId);
          const parentNode = getNode(rootNode, parent);
          if (!parentNode || !parentNode.is_dir) {
            throw JSON.stringify({ kind: "SftpNotFound", message: `write: no such dir: ${parent}` });
          }
          parentNode.children.set(name, makeFile(name, bytes));
          return null;
        }
        case "sftp_rename": {
          maybeSftpForce(db);
          const hostId = String(args.hostId ?? args.host_id ?? "");
          const from = String(args.from ?? "");
          const to = String(args.to ?? "");
          const root = String(args.root ?? (from.startsWith("/") ? "/" : "."));
          const fromSafe = mockResolve(root, from);
          const toSafe = mockResolve(root, to);
          const rootNode = ensureSftpRoot(db, hostId);
          const { parent: fp, name: fn } = parentAndName(fromSafe);
          const { parent: tp, name: tn } = parentAndName(toSafe);
          const fromParent = getNode(rootNode, fp);
          const node = fromParent?.children.get(fn);
          if (!fromParent || !node) {
            throw JSON.stringify({ kind: "SftpNotFound", message: `rename: no such file: ${fromSafe}` });
          }
          const toParent = getNode(rootNode, tp);
          if (!toParent || !toParent.is_dir) {
            throw JSON.stringify({ kind: "SftpNotFound", message: `rename: no such dir: ${tp}` });
          }
          fromParent.children.delete(fn);
          node.name = tn;
          toParent.children.set(tn, node);
          return null;
        }
        case "sftp_remove": {
          maybeSftpForce(db);
          const hostId = String(args.hostId ?? args.host_id ?? "");
          const path = String(args.path ?? "");
          const root = String(args.root ?? (path.startsWith("/") ? "/" : "."));
          const safe = mockResolve(root, path);
          const { parent, name } = parentAndName(safe);
          const rootNode = ensureSftpRoot(db, hostId);
          const parentNode = getNode(rootNode, parent);
          const node = parentNode?.children.get(name);
          if (!parentNode || !node) {
            throw JSON.stringify({ kind: "SftpNotFound", message: `remove: no such file: ${safe}` });
          }
          if (node.is_dir && node.children.size > 0) {
            throw JSON.stringify({ kind: "SftpIo", message: `remove: directory not empty: ${safe}` });
          }
          parentNode.children.delete(name);
          return null;
        }
        case "test_sftp_force_error": {
          db.sftpForceError = String(args.error ?? args.message ?? "");
          return null;
        }
        case "test_sftp_reset": {
          const hostId = String(args.hostId ?? args.host_id ?? "");
          if (hostId) db.sftp.delete(hostId);
          else db.sftp.clear();
          db.sftpForceError = null;
          return null;
        }
        case "sync_status":
          return {
            ...db.sync,
            sync_secrets: db.sync.sync_secrets ?? false,
          };
        case "sync_configure": {
          const config = (args.config ?? args) as { url?: string; sync_secrets?: boolean };
          db.sync = {
            configured: true,
            url: config.url ?? "postgres://test",
            last_sync: null,
            last_error: null,
            state: "idle",
            sync_secrets: Boolean(config.sync_secrets),
          };
          return null;
        }
        case "sync_set_secrets": {
          const on = Boolean(
            args.syncSecrets ?? args.sync_secrets ?? false,
          );
          db.sync.sync_secrets = on;
          return null;
        }
        case "sync_now":
          db.sync.state = "idle";
          db.sync.last_sync = stamp();
          db.sync.last_error = null;
          return { pulled: 0, pushed: 0 };
        case "test_set_host_connection": {
          const hostId = String(args.hostId ?? args.host_id ?? "");
          const connectionState = String(
            args.connectionState ?? args.connection_state ?? "disconnected",
          );
          ensureHost(db, hostId);
          db.connections.set(hostId, connectionState);
          return null;
        }
        case "test_set_sync_status": {
          const status = (args.status ?? args) as Record<string, unknown>;
          const stateName = String(status.state ?? "unconfigured");
          db.sync = {
            configured: Boolean(
              status.configured ?? status.configured ?? stateName !== "unconfigured",
            ),
            url:
              (status.url as string | null | undefined) ??
              (stateName !== "unconfigured" ? "postgres://test" : null),
            last_sync:
              (status.last_sync as string | null | undefined) ??
              (stateName === "idle" ? stamp() : null),
            last_error: (status.last_error as string | null | undefined) ?? null,
            state: stateName,
            sync_secrets: Boolean(status.sync_secrets ?? db.sync.sync_secrets ?? false),
          };
          return null;
        }
        case "session_open_local": {
          const info: SessionInfo = {
            id: `local-${crypto.randomUUID()}`,
            title: "local",
            kind: "local",
            host_id: null,
          };
          db.sessions.push(info);
          return info;
        }
        case "session_open_ssh": {
          const hostId = String(args.hostId ?? args.host_id ?? "");
          ensureHost(db, hostId);
          if (!db.connections.has(hostId) || db.connections.get(hostId) === "disconnected") {
            db.connections.set(hostId, "connected");
          }
          const info: SessionInfo = {
            id: `ssh-${crypto.randomUUID()}`,
            title: hostId,
            kind: "ssh",
            host_id: hostId,
          };
          db.sessions.push(info);
          return info;
        }
        case "session_close": {
          db.sessions = db.sessions.filter((s) => s.id !== String(args.id));
          return null;
        }
        case "session_list":
          return db.sessions;
        case "session_frame":
          return new Uint8Array();
        case "ssh_host_key_fingerprint":
          return { algo: "ssh-ed25519", sha256: "SHA256:e2e-mock" };
        case "ssh_host_key_trust":
          return null;
        case "session_write":
        case "session_resize":
        case "identity_import_path":
          return null;
        default:
          if (cmd.startsWith("plugin:")) return null;
          console.warn(`[e2eMock] unhandled command: ${cmd}`);
          return null;
      }
    },
    { shouldMockEvents: true },
  );

  console.log("[e2eMock] Tauri IPC mocked for VITE_E2E preview");
}
