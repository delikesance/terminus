/**
 * Browser-only Tauri IPC mock for Playwright against vite preview.
 * Active only when VITE_E2E=1 and real Tauri is absent.
 * Normal release builds never enable this.
 */

import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";
import { isTauri } from "@tauri-apps/api/core";
import { emit } from "@tauri-apps/api/event";

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
  os_id?: string | null;
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
  /** Local (user machine) virtual FS root + home for the local pane. */
  local: SftpNode | null;
  localHome: string;
  /** Transfer op counters for E2E assertions. */
  transferUploads: number;
  transferDownloads: number;
  /** Force next sftp_* call to fail with typed IPC JSON. */
  sftpForceError: string | null;
  /** Last file opened via sftp_open (E2E). */
  sftpLastOpen: { hostId: string; path: string; name: string } | null;
  /** Host ids that must pass TOFU before session_open_ssh succeeds. */
  tofuRequired: Set<string>;
  /** `${hostname}:${port}` keys accepted via ssh_host_key_trust. */
  tofuTrusted: Set<string>;
  /** Artificial handshake delay (ms) before session_open_ssh resolves. */
  slowConnectMs: Map<string, number>;
  /** Artificial delay (ms) before the next sftp_list for a host (#107). */
  slowSftpListMs: Map<string, number>;
  /** Host ids whose next session_open_ssh fails with a transport/auth error. */
  authFail: Set<string>;
  /** In-flight SSH connect attempts per host (Architect aggregation). */
  inflight: Map<string, number>;
  /** Saved local port forwards. */
  forwards: Array<{
    id: string;
    host_id: string;
    kind: string;
    name: string;
    bind_host: string;
    bind_port: number;
    dest_host?: string | null;
    dest_port?: number | null;
    created_at: string;
    updated_at: string;
    deleted_at?: string | null;
  }>;
  /** Running forward ids. */
  forwardsRunning: Set<string>;
  /** Force next forward_start to fail. */
  forwardStartError: string | null;
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
    local: null,
    localHome: "/home/local",
    transferUploads: 0,
    transferDownloads: 0,
    sftpForceError: null,
    sftpLastOpen: null,
    tofuRequired: new Set(),
    tofuTrusted: new Set(),
    slowConnectMs: new Map(),
    slowSftpListMs: new Map(),
    authFail: new Set(),
    inflight: new Map(),
    forwards: [],
    forwardsRunning: new Set(),
    forwardStartError: null,
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
  let localCount = 0;
  for (const s of db.sessions) {
    if (!s.host_id) {
      if (s.kind === "local") localCount += 1;
      continue;
    }
    counts.set(s.host_id, (counts.get(s.host_id) ?? 0) + 1);
  }
  const out: { host_id: string; connection: string; open_count: number }[] = [];
  if (localCount > 0) {
    out.push({ host_id: "local", connection: "local", open_count: localCount });
  }
  for (const h of db.hosts.filter((x) => !x.deleted_at)) {
    const open_count = counts.get(h.id) ?? 0;
    const inflight = db.inflight.get(h.id) ?? 0;
    const tracked = db.connections.get(h.id) ?? "disconnected";
    let connection: string;
    if (inflight > 0) {
      connection = "connecting";
    } else if (open_count > 0) {
      connection = "connected";
    } else if (tracked === "error") {
      connection = "error";
    } else if (tracked === "connecting") {
      connection = "connecting";
    } else if (tracked === "connected") {
      connection = "connected";
    } else {
      connection = "disconnected";
    }
    out.push({ host_id: h.id, connection, open_count });
  }
  for (const [host_id, open_count] of counts) {
    if (!host_id.startsWith("wsl:")) continue;
    out.push({ host_id, connection: "local", open_count });
  }
  return out;
}

async function emitHostRuntime(db: Db, hostId: string) {
  const rt = runtimes(db).find((r) => r.host_id === hostId);
  if (rt) await emit("hosts://runtime", rt);
}

function argsOf(payload: unknown): Record<string, unknown> {
  if (!payload || typeof payload !== "object") return {};
  return payload as Record<string, unknown>;
}

function tofuHostKey(hostname: string, port: unknown): string {
  return `${hostname}:${Number(port) || 22}`;
}

function trustHostFromArgs(args: Record<string, unknown>): { hostName: string; port: number } {
  const nested = args.host;
  if (nested && typeof nested === "object") {
    const o = nested as Record<string, unknown>;
    return {
      hostName: String(o.hostname ?? o.host ?? ""),
      port: Number(o.port ?? args.port ?? 22) || 22,
    };
  }
  return {
    hostName: String(args.host ?? args.hostname ?? args.hostName ?? ""),
    port: Number(args.port ?? 22) || 22,
  };
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

function mockHome(db: Db, hostId: string): string {
  const user = db.hosts.find((h) => h.id === hostId)?.username || "lab";
  return `/home/${user}`;
}

function mockRealpath(db: Db, hostId: string, path: string): string {
  const home = mockHome(db, hostId);
  const raw = path || ".";
  if (raw === "." || raw === "") return home;
  if (raw.startsWith("/")) return mockNormalize(raw);
  const rel = mockNormalize(raw);
  if (rel === ".") return home;
  return `${home}/${rel}`;
}

function ensureSftpRoot(db: Db, hostId: string): SftpNode {
  let root = db.sftp.get(hostId);
  if (!root) {
    // Full-FS tree rooted at "/" with the host's home at /home/<user>.
    const user = db.hosts.find((h) => h.id === hostId)?.username || "lab";
    root = makeDir("/");
    const home = makeDir("home");
    const userDir = makeDir(user);
    userDir.children.set("docs", makeDir("docs"));
    userDir.children.get("docs")!.children.set("readme.txt", makeFile("readme.txt", "hello sftp"));
    userDir.children.set("notes.txt", makeFile("notes.txt", "notes"));
    userDir.children.set(".cache", makeDir(".cache"));
    userDir.children.set("remote-only.txt", makeFile("remote-only.txt", "from remote"));
    home.children.set(user, userDir);
    root.children.set("home", home);
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

// ─── Local (user machine) virtual FS helpers ────────────────────────────────
function normLocal(p: string): string {
  return p.replace(/\\/g, "/").replace(/\/+/g, "/");
}

function ensureLocalRoot(db: Db): SftpNode {
  if (!db.local) {
    const root = makeDir(".");
    root.children.set("docs", makeDir("docs"));
    root.children.get("docs")!.children.set("readme.txt", makeFile("readme.txt", "hello local"));
    root.children.set("notes.txt", makeFile("notes.txt", "local notes"));
    root.children.set("subdir", makeDir("subdir"));
    root.children.get("subdir")!.children.set("deep.txt", makeFile("deep.txt", "deep"));
    root.children.set("local-upload.txt", makeFile("local-upload.txt", "from local"));
    root.children.set(".cache", makeDir(".cache"));
    db.local = root;
    db.localHome = "/home/local";
  }
  return db.local;
}

/** Relative segments of `path` under the local home. Throws typed IPC JSON. */
function localRel(db: Db, path: string): string[] {
  const home = db.localHome;
  const n = normLocal(path);
  if (n === home) return [];
  if (n.startsWith(home + "/")) return n.slice(home.length + 1).split("/").filter(Boolean);
  throw JSON.stringify({ kind: "SftpNotFound", message: `local: no such path: ${path}` });
}

function localNodeAt(db: Db, path: string): SftpNode | null {
  const root = ensureLocalRoot(db);
  let cur = root;
  for (const seg of localRel(db, path)) {
    const next = cur.children.get(seg);
    if (!next) return null;
    cur = next;
  }
  return cur;
}

/** Ensure every directory in the path exists, then return `{ parent, name }`. */
function localEnsureDirChain(db: Db, path: string): { parent: SftpNode; name: string } {
  const rel = localRel(db, path);
  const name = rel.pop()!;
  let cur = ensureLocalRoot(db);
  for (const seg of rel) {
    let next = cur.children.get(seg);
    if (!next) {
      next = makeDir(seg);
      cur.children.set(seg, next);
    }
    cur = next;
  }
  return { parent: cur, name };
}

function localListEntries(db: Db, path: string): SftpEntry[] {
  const node = localNodeAt(db, path);
  if (!node || !node.is_dir) {
    throw JSON.stringify({ kind: "SftpNotFound", message: `list: no such dir: ${path}` });
  }
  const base = normLocal(path);
  const home = db.localHome;
  const out: SftpEntry[] = [];
  for (const child of node.children.values()) {
    out.push({
      name: child.name,
      path: base === home ? `${home}/${child.name}` : `${base}/${child.name}`,
      is_dir: child.is_dir,
      size: child.is_dir ? 0 : child.content.byteLength,
      mtime: child.mtime,
    });
  }
  out.sort((a, b) => Number(b.is_dir) - Number(a.is_dir) || a.name.localeCompare(b.name));
  return out;
}

function localRemoveNode(db: Db, path: string): void {
  const rel = localRel(db, path);
  const name = rel.pop()!;
  let cur = ensureLocalRoot(db);
  for (const seg of rel) {
    const next = cur.children.get(seg);
    if (!next) throw JSON.stringify({ kind: "SftpNotFound", message: `remove: no such path: ${path}` });
    cur = next;
  }
  const node = cur.children.get(name);
  if (!node) throw JSON.stringify({ kind: "SftpNotFound", message: `remove: no such path: ${path}` });
  cur.children.delete(name);
}

/** Recursively delete a remote (SFTP) subtree under its tree root. */
function sftpRemoveRecursive(root: SftpNode, rel: string[]): void {
  const name = rel[rel.length - 1];
  const dirSegs = rel.slice(0, -1);
  let cur = root;
  for (const seg of dirSegs) {
    const next = cur.children.get(seg);
    if (!next) return;
    cur = next;
  }
  cur.children.delete(name);
}

function sftpEnsureDirChain(root: SftpNode, rel: string[]): { parent: SftpNode; name: string } {
  const name = rel[rel.length - 1];
  const dirSegs = rel.slice(0, -1);
  let cur = root;
  for (const seg of dirSegs) {
    let next = cur.children.get(seg);
    if (!next) {
      next = makeDir(seg);
      cur.children.set(seg, next);
    }
    cur = next;
  }
  return { parent: cur, name };
}


/** Install mock IPC. No-op outside VITE_E2E or when real Tauri is present. */
export function installE2eMock(): void {
  if (import.meta.env.VITE_E2E !== "1") return;
  if (typeof window === "undefined") return;
  if (isTauri()) return;

  const db = createDb();
  mockWindows("main");
  mockIPC(
    async (cmd, payload) => {
      const args = argsOf(payload);

      switch (cmd) {
        case "local_os_id":
          return "linux";
        case "themes_list":
          return [GRAPHITE];
        case "appearance_get":
          return db.appearance;
        case "appearance_set":
          db.appearance = { ...db.appearance, ...(args.appearance as object) };
          return null;
        case "keybindings_get":
          return {
            "ctrl+shift+t": "tab.new",
            "ctrl+shift+w": "tab.close",
            "ctrl+tab": "host.next",
            "ctrl+shift+tab": "host.prev",
            "cmd+tab": "host.next",
            "cmd+shift+tab": "host.prev",
            "ctrl+k": "palette.toggle",
            "ctrl+,": "settings.toggle",
            "ctrl+l": "terminal.clear",
            "ctrl+shift+c": "terminal.copy",
            "ctrl+shift+v": "terminal.paste",
            "ctrl+b": "sidebar.toggle",
            "cmd+b": "sidebar.toggle",
          };
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
          db.tofuRequired.delete(id);
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
          return [];
        case "forwards_list":
          return db.forwards.filter((f) => !f.deleted_at);
        case "forwards_upsert": {
          const forward = (args.forward ?? args) as (typeof db.forwards)[number];
          db.forwardsRunning.delete(forward.id);
          const idx = db.forwards.findIndex((f) => f.id === forward.id);
          if (idx >= 0) db.forwards[idx] = { ...forward };
          else db.forwards.push({ ...forward });
          return forward;
        }
        case "forwards_delete": {
          const id = String(args.id);
          db.forwardsRunning.delete(id);
          const row = db.forwards.find((f) => f.id === id);
          if (row) {
            row.deleted_at = stamp();
            row.updated_at = stamp();
          }
          return null;
        }
        case "forwards_running":
          return [...db.forwardsRunning];
        case "forward_start": {
          const id = String(args.id);
          if (db.forwardStartError) {
            const msg = db.forwardStartError;
            db.forwardStartError = null;
            throw msg;
          }
          if (db.forwardsRunning.has(id)) throw `forward ${id} is already running`;
          const fwd = db.forwards.find((f) => f.id === id && !f.deleted_at);
          if (!fwd) throw "forward not found";
          const host = db.hosts.find((h) => h.id === fwd.host_id && !h.deleted_at);
          if (!host) throw "host not found";
          if (fwd.dest_port == null) throw "destination port is required";
          db.forwardsRunning.add(id);
          return null;
        }
        case "forward_stop": {
          const id = String(args.id);
          if (!db.forwardsRunning.has(id)) throw "forward is not running";
          db.forwardsRunning.delete(id);
          return null;
        }
        case "test_forward_fail_next": {
          db.forwardStartError = String(args.message ?? args.error ?? "bind failed");
          return null;
        }
        case "sftp_list": {
          maybeSftpForce(db);
          const hostId = String(args.hostId ?? args.host_id ?? "");
          const path = String(args.path ?? ".");
          const root = String(args.root ?? (path.startsWith("/") ? "/" : "."));
          const safe = mockResolve(root, path);
          const slowList = db.slowSftpListMs.get(hostId) ?? 0;
          if (slowList > 0) {
            db.slowSftpListMs.delete(hostId);
            await new Promise((r) => setTimeout(r, slowList));
          }
          ensureHost(db, hostId);
          if (!db.connections.has(hostId) || db.connections.get(hostId) === "disconnected") {
            db.connections.set(hostId, "connected");
          }
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
          db.transferUploads += 1;
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
        case "sftp_realpath": {
          maybeSftpForce(db);
          const hostId = String(args.hostId ?? args.host_id ?? "");
          const path = String(args.path ?? ".");
          ensureHost(db, hostId);
          if (!db.connections.has(hostId) || db.connections.get(hostId) === "disconnected") {
            db.connections.set(hostId, "connected");
          }
          return mockRealpath(db, hostId, path);
        }
        case "sftp_open": {
          maybeSftpForce(db);
          const hostId = String(args.hostId ?? args.host_id ?? "");
          const path = String(args.path ?? "");
          const root = String(args.root ?? (path.startsWith("/") ? "/" : "."));
          const safe = mockResolve(root, path);
          const node = getNode(ensureSftpRoot(db, hostId), safe);
          if (!node || node.is_dir) {
            throw JSON.stringify({ kind: "SftpNotFound", message: `open: no such file: ${safe}` });
          }
          const name = safe.split("/").filter(Boolean).pop() || "file";
          db.sftpLastOpen = { hostId, path: safe, name };
          return { opened: true, localPath: `/tmp/terminus-sftp-open/${name}` };
        }
        case "test_sftp_last_open":
          return db.sftpLastOpen;
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
        case "local_home": {
          ensureLocalRoot(db);
          return db.localHome;
        }
        case "local_list": {
          const path = String(args.path ?? db.localHome);
          ensureLocalRoot(db);
          return localListEntries(db, path);
        }
        case "local_read": {
          const path = String(args.path ?? "");
          const node = localNodeAt(db, path);
          if (!node || node.is_dir) {
            throw JSON.stringify({ kind: "SftpNotFound", message: `read: no such file: ${path}` });
          }
          return Array.from(node.content);
        }
        case "local_write": {
          const path = String(args.path ?? "");
          const data = args.data as number[] | Uint8Array;
          const bytes = data instanceof Uint8Array ? data : Uint8Array.from(data ?? []);
          const { parent, name } = localEnsureDirChain(db, path);
          parent.children.set(name, makeFile(name, bytes));
          db.transferDownloads += 1;
          return null;
        }
        case "local_mkdir": {
          const path = String(args.path ?? "");
          const { parent, name } = localEnsureDirChain(db, path);
          parent.children.set(name, makeDir(name));
          return null;
        }
        case "local_remove": {
          localRemoveNode(db, String(args.path ?? ""));
          return null;
        }
        case "local_rename": {
          const from = String(args.from ?? "");
          const to = String(args.to ?? "");
          const src = localNodeAt(db, from);
          if (!src) throw JSON.stringify({ kind: "SftpNotFound", message: `rename: no such path: ${from}` });
          const fp = localEnsureDirChain(db, from);
          fp.parent.children.delete(fp.name);
          const { parent: tp, name: tn } = localEnsureDirChain(db, to);
          src.name = tn;
          tp.children.set(tn, src);
          return null;
        }
        case "sftp_mkdir": {
          maybeSftpForce(db);
          const hostId = String(args.hostId ?? args.host_id ?? "");
          const path = String(args.path ?? "");
          const root = String(args.root ?? (path.startsWith("/") ? "/" : "."));
          const safe = mockResolve(root, path);
          const rootNode = ensureSftpRoot(db, hostId);
          const { parent, name } = sftpEnsureDirChain(rootNode, splitRel(safe));
          parent.children.set(name, makeDir(name));
          return null;
        }
        case "sftp_rmtree": {
          maybeSftpForce(db);
          const hostId = String(args.hostId ?? args.host_id ?? "");
          const path = String(args.path ?? "");
          const root = String(args.root ?? (path.startsWith("/") ? "/" : "."));
          const safe = mockResolve(root, path);
          const rootNode = ensureSftpRoot(db, hostId);
          if (splitRel(safe).length === 0) return null;
          sftpRemoveRecursive(rootNode, splitRel(safe));
          return null;
        }
        case "test_transfer_ops":
          return { uploads: db.transferUploads, downloads: db.transferDownloads };
        case "test_local_reset": {
          db.local = null;
          db.localHome = args.path ? normLocal(String(args.path)) : "/home/local";
          ensureLocalRoot(db);
          return db.localHome;
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
          db.sftpLastOpen = null;
          return null;
        }
        case "test_sftp_bulk": {
          const hostId = String(args.hostId ?? args.host_id ?? "");
          const count = Math.max(0, Math.min(5000, Number(args.count ?? 500)));
          const root = ensureSftpRoot(db, hostId);
          const user = db.hosts.find((h) => h.id === hostId)?.username || "lab";
          const home = root.children.get("home");
          const userDir = home?.children.get(user);
          if (!userDir) throw JSON.stringify({ kind: "SftpNotFound", message: "bulk: no home" });
          for (let i = 0; i < count; i++) {
            const name = `bulk-${String(i).padStart(4, "0")}.dat`;
            if (!userDir.children.has(name)) {
              userDir.children.set(name, makeFile(name, `bulk-${i}`));
            }
          }
          return { count, path: `/home/${user}` };
        }
        case "test_require_tofu": {
          const hostId = String(args.hostId ?? args.host_id ?? "");
          if (hostId) db.tofuRequired.add(hostId);
          return null;
        }
        case "test_slow_connect": {
          const hostId = String(args.hostId ?? args.host_id ?? "");
          const ms = Number(args.ms ?? args.delayMs ?? 400);
          if (hostId) db.slowConnectMs.set(hostId, ms);
          return null;
        }
        case "test_sftp_slow_list": {
          const hostId = String(args.hostId ?? args.host_id ?? "");
          const ms = Number(args.ms ?? args.delayMs ?? 800);
          if (hostId) db.slowSftpListMs.set(hostId, ms);
          return null;
        }
        case "test_auth_fail": {
          const hostId = String(args.hostId ?? args.host_id ?? "");
          if (hostId) db.authFail.add(hostId);
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
        case "wsl_list_distros":
          return [
            { name: "Ubuntu", state: "Running", version: 2, is_default: true },
            { name: "Debian", state: "Stopped", version: 2, is_default: false },
          ];
        case "session_open_wsl": {
          const distro = String(args.distro ?? "Ubuntu");
          const info: SessionInfo = {
            id: `wsl-${crypto.randomUUID()}`,
            title: distro,
            kind: "wsl",
            host_id: `wsl:${distro}`,
          };
          db.sessions.push(info);
          return info;
        }
        case "session_open_ssh": {
          const hostId = String(args.hostId ?? args.host_id ?? "");
          ensureHost(db, hostId);
          const host = db.hosts.find((h) => h.id === hostId);

          // Mirror Rust begin_ssh_connect: inflight + connecting + emit.
          db.inflight.set(hostId, (db.inflight.get(hostId) ?? 0) + 1);
          db.connections.set(hostId, "connecting");
          await emitHostRuntime(db, hostId);

          const delay = db.slowConnectMs.get(hostId) ?? 0;
          if (delay > 0) {
            await new Promise((r) => setTimeout(r, delay));
            db.slowConnectMs.delete(hostId);
          }

          const decInflight = () => {
            const next = Math.max(0, (db.inflight.get(hostId) ?? 1) - 1);
            if (next === 0) db.inflight.delete(hostId);
            else db.inflight.set(hostId, next);
          };

          if (host && db.tofuRequired.has(hostId)) {
            const key = tofuHostKey(host.hostname, host.port);
            if (!db.tofuTrusted.has(key)) {
              decInflight();
              // AC4: HostKey → disconnected (not sticky error).
              db.connections.set(hostId, "disconnected");
              await emitHostRuntime(db, hostId);
              throw JSON.stringify({
                kind: "HostKeyUnknown",
                host: host.hostname,
                port: Number(host.port) || 22,
                public_key: "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIJdD7y3aLq454yWBdwLWbieU1ebz9/cu7/QEXn9OIeZJ",
                algo: "ssh-ed25519",
                fingerprint: "SHA256:e2e-mock-fingerprint",
              });
            }
          }

          if (db.authFail.has(hostId)) {
            db.authFail.delete(hostId);
            decInflight();
            db.connections.set(hostId, "error");
            await emitHostRuntime(db, hostId);
            throw "SSH authentication failed (e2e mock)";
          }

          decInflight();
          db.connections.set(hostId, "connected");
          await emitHostRuntime(db, hostId);
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
          const sid = String(args.id);
          const closing = db.sessions.find((s) => s.id === sid);
          db.sessions = db.sessions.filter((s) => s.id !== sid);
          if (closing?.host_id) {
            const still = db.sessions.some((s) => s.host_id === closing.host_id);
            if (!still) {
              const cur = db.connections.get(closing.host_id);
              if (cur === "connected" || cur === "connecting") {
                db.connections.set(closing.host_id, "disconnected");
              }
            }
          }
          return null;
        }
        case "session_list":
          return db.sessions;
        case "session_frame":
          return new Uint8Array();
        case "session_selection_text": {
          const { r0, c0, r1, c1 } = args as any;
          const text = `sel-${r0}-${c0}-${r1}-${c1}`;
          (window as any).__lastSelCmd = text;
          return text;
        }
        case "ssh_host_key_fingerprint":
          return { algo: "ssh-ed25519", sha256: "SHA256:e2e-mock" };
        case "ssh_host_key_trust": {
          const { hostName, port } = trustHostFromArgs(args);
          if (hostName) db.tofuTrusted.add(tofuHostKey(hostName, port));
          for (const id of [...db.tofuRequired]) {
            const h = db.hosts.find((x) => x.id === id);
            if (h) db.tofuTrusted.add(tofuHostKey(h.hostname, h.port));
          }
          db.tofuRequired.clear();
          return null;
        }
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
