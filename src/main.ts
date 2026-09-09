import "./assets/font-logos/font-logos.css";
import "./styles.css";
import { icons, sftpKindIcon, hostOsIcon } from "./icons";
import { applyChrome, type Theme } from "./theme";
import { resolveMonoFont } from "./fonts";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { open } from "@tauri-apps/plugin-dialog";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { computeAffectedGroups, findOrphanedHosts, applySoftDelete, detachHost } from "./groupSoftDelete";
import { initTestBridge } from "./testBridge";
import { installE2eMock } from "./e2eMock";
import { joinLocalPath, parentLocalPath, fileNameOfPath } from "./localPath";
import {
  resolveUnderRoot,
  normalizeSftpPath,
  parentSftpPath,
  parseSftpError,
  sftpDisplayPath,
  logicalFromDisplayPath,
} from "./sftpPath";
import {
  SFTP_LOCAL_ID,
  canTransferBetween,
  createSftpBrowserState,
  enterSplit,
  exitSplit,
  isLocalEndpoint,
  openSingle,
  setPaneEndpoint,
  type SftpBrowserState,
  type SftpEndpointId,
  type SftpPaneSlot,
} from "./sftpLayout";
import {
  batchDownloadEnabled,
  batchUploadEnabled,
  filterFileEntries,
  shouldShowBatchBar,
  sftpSearchPlaceholder,
  type SftpListFilter,
} from "./sftpUx";
import { mountVirtualList, type VirtualListHandle } from "./sftpVirtualList";
import { filterFileEntriesAsync } from "./sftpFilterAsync";
import { pickRenderer } from "./perf";
import { createTrailingDebounce, FRAME_MIN_MS, SFTP_FILTER_DEBOUNCE_MS } from "./perfTiming";
import { parseKnownHosts } from "./knownHostsParse";
import {
  inferIdentityKind,
  parseIdentityKind,
  parseIdentityKeyError,
  type IdentityKind,
} from "./identityKind";
import {
  ACTIVITIES,
  clickActivity,
  createNavState,
  isPanelVisible,
  toggleSidebarOpen,
  TERMINAL_PANE_PADDING_X_PX,
  TERMINAL_PANE_PADDING_Y_PX,
  type ActivityId,
  type NavState,
} from "./activityBar";
import {
  applyToggleFailure,
  applyToggleSuccess,
  buildForwardRows,
  closeCreateForm,
  createForwardUiState,
  formatForwardSubtitle,
  forwardAddBtnMode,
  shouldCloseCreateFormOnEscape,
  shouldShowCreateForm,
  shouldShowHostFooterActions,
  toggleActionFromChecked,
  toggleCreateForm,
  validateForwardForm,
  type ForwardUiState,
} from "./forwardPanel";
import {
  connectionDotClassList,
  connectionDotColor,
  hostCardAriaStatus,
  hostCardClassList,
  sessionCountLabel,
  shouldShowSessionCount,
} from "./hostCard";
import { settingsHasUpdateBadge, shouldShowFloatingUpdateToast } from "./updateNotify";

const ONBOARD_KEY = "terminus.onboarded";

// Must run before any invoke/listen for Playwright vite-preview.
installE2eMock();

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

type HostRuntime = {
  host_id: string;
  connection: string;
  open_count: number;
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
type Snippet = { id: string; title: string; content: string; tags: string[]; shortcut?: string | null };
type HistoryEntry = { id: string; command: string; cwd?: string | null; session_kind: string; created_at: string };
type PortForward = {
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
};
type SessionInfo = { id: string; title: string; kind: string; host_id?: string | null };
type Group = { id: string; name: string; parent_id?: string | null; created_at: string; updated_at: string; deleted_at?: string | null };
type Appearance = {
  font_family: string;
  font_size: number;
  line_height: number;
  letter_spacing: number;
  cursor_style: "block" | "bar" | "underline";
  cursor_blink: boolean;
  scrollback: number;
  renderer: string;
  padding: number;
  opacity: number;
  custom_css: string;
  theme_id: string;
  ligatures: boolean;
};
type SyncStatus = {
  configured: boolean;
  url?: string | null;
  last_sync?: string | null;
  last_error?: string | null;
  state?: string;
  sync_secrets?: boolean;
};
type SftpEntry = {
  name: string;
  path: string;
  is_dir: boolean;
  size: number;
  mtime?: number | null;
};
type LocalEntry = {
  name: string;
  path: string;
  is_dir: boolean;
  size: number;
  modified?: number | null;
};

type TransferJob = {
  /** "upload" = local→remote, "download" = remote→local. */
  direction: "upload" | "download";
  total: number;
  done: number;
  failed: number;
  label: string;
  active: boolean;
  cancel: boolean;
};

type Pane = {
  id: string;
  session?: SessionInfo;
  pending?: { title: string; kind: string; hostId?: string };
  exited?: boolean;
  /** Set when the tab is closed while a connect is still in flight. */
  aborted?: boolean;
  canvas: HTMLCanvasElement;
  ctx: CanvasRenderingContext2D;
  el: HTMLDivElement;
  banner: HTMLDivElement;
  cellW: number;
  cellH: number;
  rasterScale: number;
  cols: number;
  rows: number;
  paintGen: number;
  selAnchor: { row: number; col: number } | null;
  selFocus: { row: number; col: number } | null;
  selLayer: HTMLDivElement;
  _selecting?: boolean;
};

const state = {
  hosts: [] as Host[],
  hostsRuntime: [] as HostRuntime[],
  groups: [] as Group[],
  identities: [] as Identity[],
  snippets: [] as Snippet[],
  history: [] as HistoryEntry[],
  forwards: [] as PortForward[],
  forwardsRunning: new Set<string>(),
  themes: [] as Theme[],
  appearance: null as Appearance | null,
  keybindings: {} as Record<string, string>,
  panes: [] as Pane[],
  activePane: null as string | null,
  sftpHostId: null as string | null,
  sftpRoot: "/" as string,
  sftpPath: "." as string,
  sftpCwd: "" as string,
  sftpCwdHostId: null as string | null,
  sftpMode: false,
  localCwd: "" as string,
  localRoot: null as string | null,
  localEntries: [] as LocalEntry[],
  sftpEntries: [] as SftpEntry[],
  localSelected: new Set<string>(),
  sftpSelected: new Set<string>(),
  transfer: null as TransferJob | null,
  sftpConn: "disconnected" as string,
  sftpShowHidden: localStorage.getItem("terminus-sftp-show-hidden") === "1",
  sftpCompact: localStorage.getItem("terminus-sftp-compact") !== "0",
  customCss: document.createElement("style"),
  expandedGroups: new Set<string>(JSON.parse(localStorage.getItem("terminus-expanded-groups") || "[]")),
  localOsId: null as string | null,
};

/** Activity Bar + contextual sidebar navigation (#27). */
let navState: NavState = createNavState();
let forwardUi: ForwardUiState = createForwardUiState();
/** SFTP single/split browser (#41). */
let sftpBrowser: SftpBrowserState = createSftpBrowserState();
const sftpFilterDebounce = createTrailingDebounce(SFTP_FILTER_DEBOUNCE_MS, () => {
  rerenderSftpListings();
});
let localVirtual: VirtualListHandle<LocalEntry> | null = null;
let remoteVirtual: VirtualListHandle<SftpEntry> | null = null;

document.head.appendChild(state.customCss);

const $ = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;
const $input = (id: string) => $(id) as HTMLInputElement | HTMLTextAreaElement | HTMLSelectElement;

function b64encode(data: string | Uint8Array): string {
  const bytes = typeof data === "string" ? new TextEncoder().encode(data) : data;
  let bin = "";
  for (let i = 0; i < bytes.length; i += 8192) {
    bin += String.fromCharCode(...bytes.subarray(i, i + 8192));
  }
  return btoa(bin);
}

async function boot() {
  bindUi();
  toggleSidebar(true);
  const [themes, appearance, keybindings, localOsId] = await Promise.all([
    invoke<Theme[]>("themes_list"),
    invoke<Appearance>("appearance_get"),
    invoke<Record<string, string>>("keybindings_get"),
    invoke<string>("local_os_id").catch(() => "linux"),
  ]);
  state.themes = themes;
  state.appearance = appearance;
  state.keybindings = keybindings;
  state.localOsId = localOsId || "linux";
  if (state.appearance) {
    // Resolve auto/webgl → concrete renderer; honor explicit canvas (#52).
    state.appearance.renderer = pickRenderer(state.appearance.renderer || "auto");
    if (import.meta.env.VITE_E2E === "1") {
      (window as any).__terminusActiveRenderer = state.appearance.renderer;
    }
    if (
      !localStorage.getItem("terminus-ux-v2") &&
      ["obsidian", "terminus", "mocha"].includes(state.appearance.theme_id)
    ) {
      state.appearance.theme_id = "graphite";
      localStorage.setItem("terminus-ux-v2", "1");
    }
    state.appearance.font_family = resolveMonoFont();
    state.appearance.letter_spacing = 0;
    state.appearance.line_height = 1.0;
    void invoke("appearance_set", { appearance: state.appearance });
  }
  applyAppearance();
  void Promise.all([refreshSide(), refreshSync()]).then(() => {
    maybeShowOnboarding();
  });
  window.setTimeout(() => void checkForAppUpdate(), 1500);
  await listen<{ id: string }>("session://output", (ev) => {
    scheduleFrame(ev.payload.id);
  });
  await listen<{ id: string }>("session://exit", (ev) => {
    markExited(ev.payload.id);
  });
  await listen("hosts://changed", () => {
    void refreshSide();
  });
  await listen<HostRuntime>("hosts://runtime", (ev) => {
    applyHostRuntime(ev.payload);
  });
  requestAnimationFrame(() => {
    void openLocal();
  });
  void document.fonts?.ready.then(() => {
    if (!state.appearance) return;
    state.appearance.font_family = resolveMonoFont();
    applyAppearance();
  });
}

function hideUpdateToast() {
  const el = $("update-toast");
  el.classList.add("hidden");
  el.innerHTML = "";
}

function showUpdateToast(html: string) {
  // Floating toast retired (#33). Keep helper for rare install progress fallback in sheet.
  if (shouldShowFloatingUpdateToast("progress")) {
    const el = $("update-toast");
    el.innerHTML = html;
    el.classList.remove("hidden");
  }
}

let updateBusy = false;
let pendingAppUpdate: Update | null = null;

function renderSettingsUpdateBadge() {
  const btn = $("btn-settings");
  const available = settingsHasUpdateBadge(!!pendingAppUpdate);
  btn.dataset.updateAvailable = available ? "true" : "false";
  const badge = available
    ? `<span data-testid="settings-update-badge" class="settings-update-badge" aria-hidden="true"></span>`
    : "";
  btn.innerHTML = `${icons.settings}${badge}`;
  if (pendingAppUpdate) {
    btn.title = `Settings — update ${pendingAppUpdate.version} available`;
  } else {
    btn.title = "Settings";
  }
  btn.setAttribute("aria-label", btn.title);
}

function clearPendingAppUpdate() {
  pendingAppUpdate = null;
  renderSettingsUpdateBadge();
}

async function checkForAppUpdate() {
  if (import.meta.env.VITE_E2E === "1") return;
  try {
    const update = await check({ timeout: 10_000 });
    if (!update) return;
    if (sessionStorage.getItem(`terminus.skip-update.${update.version}`)) return;
    pendingAppUpdate = update;
    renderSettingsUpdateBadge();
  } catch {
    /* offline, unsigned, or no endpoint */
  }
}

function promptAppUpdate(update: Update) {
  openSheet(`
    <h2>Update available</h2>
    <p class="lead">Terminus <strong>${escapeHtml(update.version)}</strong> is ready. Restart to install.</p>
    <div class="row">
      <button type="button" class="ghost" id="update-later" data-testid="update-later">Later</button>
      <button type="button" class="primary" id="update-now" data-testid="update-now">Restart</button>
    </div>`);
  $("update-later").onclick = () => {
    sessionStorage.setItem(`terminus.skip-update.${update.version}`, "1");
    clearPendingAppUpdate();
    $("modal").classList.add("hidden");
  };
  $("update-now").onclick = () => void installAppUpdate(update);
}

async function installAppUpdate(update: Update) {
  if (updateBusy) return;
  updateBusy = true;
  try {
    openSheet(`
      <h2>Downloading update</h2>
      <p class="lead">Terminus ${escapeHtml(update.version)}… <span id="update-progress"></span></p>`);
    let downloaded = 0;
    let total = 0;
    await update.downloadAndInstall((event) => {
      if (event.event === "Started") {
        downloaded = 0;
        total = event.data.contentLength ?? 0;
      }
      if (event.event === "Progress") {
        downloaded += event.data.chunkLength;
        const progress = document.getElementById("update-progress");
        if (progress && total) {
          progress.textContent = `${Math.round((downloaded / total) * 100)}%`;
        }
      }
      if (event.event === "Finished") {
        const progress = document.getElementById("update-progress");
        if (progress) progress.textContent = "Installing…";
      }
    });
    openSheet(`<h2>Restarting…</h2><p class="lead">Almost done.</p>`);
    await relaunch();
  } catch (err) {
    updateBusy = false;
    openSheet(`
      <h2>Couldn't install update</h2>
      <p class="form-error">${escapeHtml(ipcErrorText(err))}</p>
      <div class="row"><button type="button" class="primary" id="update-dismiss">Dismiss</button></div>`);
    $("update-dismiss").onclick = () => $("modal").classList.add("hidden");
  }
}

/** E2E / tests: mark an update as available without hitting the network. */
function setPendingAppUpdateForTest(version: string) {
  pendingAppUpdate = {
    version,
    downloadAndInstall: async () => undefined,
  } as unknown as Update;
  renderSettingsUpdateBadge();
}

const pendingFrames = new Set<string>();
const forcedFrames = new Set<string>();
let frameTick = 0;
let frameDeferTimer = 0;
let lastFlushAt = 0;
let layoutTick = 0;

function scheduleFrame(sessionId: string, force = false) {
  pendingFrames.add(sessionId);
  if (force) forcedFrames.add(sessionId);
  if (force) {
    if (frameDeferTimer) {
      clearTimeout(frameDeferTimer);
      frameDeferTimer = 0;
    }
    if (!frameTick) frameTick = requestAnimationFrame(flushFrames);
    return;
  }
  if (frameTick || frameDeferTimer) return;
  const wait = FRAME_MIN_MS - (performance.now() - lastFlushAt);
  if (wait <= 0) {
    frameTick = requestAnimationFrame(flushFrames);
  } else {
    frameDeferTimer = window.setTimeout(() => {
      frameDeferTimer = 0;
      if (!frameTick) frameTick = requestAnimationFrame(flushFrames);
    }, wait);
  }
}

async function flushFrames() {
  frameTick = 0;
  lastFlushAt = performance.now();
  const ids = [...pendingFrames];
  pendingFrames.clear();
  const forced = new Set(forcedFrames);
  forcedFrames.clear();
  await Promise.all(ids.map((id) => paintFrame(id, forced.has(id))));
}

function toBytes(data: unknown): Uint8Array {
  if (data instanceof Uint8Array) return data;
  if (data instanceof ArrayBuffer) return new Uint8Array(data);
  if (Array.isArray(data)) return Uint8Array.from(data as number[]);
  return new Uint8Array();
}

function termBgColor(): string {
  const a = state.appearance;
  const theme = (a && state.themes.find((t) => t.id === a.theme_id)) || state.themes[0];
  if (theme?.background) return theme.background;
  const fromCss =
    getComputedStyle(document.documentElement).getPropertyValue("--term-bg").trim() ||
    getComputedStyle(document.documentElement).getPropertyValue("--bg").trim();
  return fromCss || "#1c1c1e";
}

/** Size canvas to pane client × dpr and fill theme bg — never leave HTML 300×150 black. */
function clearPaneSurface(pane: Pane) {
  const workspace = $("workspace");
  const cssW = Math.max(1, pane.el.clientWidth || workspace.clientWidth || 1);
  const cssH = Math.max(1, pane.el.clientHeight || workspace.clientHeight || 1);
  const dpr = displayScale();
  const width = Math.max(1, Math.round(cssW * dpr));
  const height = Math.max(1, Math.round(cssH * dpr));
  pane.rasterScale = dpr;
  pane.paintGen += 1;
  pane.canvas.style.width = `${cssW}px`;
  pane.canvas.style.height = `${cssH}px`;
  if (pane.canvas.width !== width || pane.canvas.height !== height) {
    pane.canvas.width = width;
    pane.canvas.height = height;
  }
  pane.ctx.setTransform(1, 0, 0, 1, 0, 0);
  pane.ctx.imageSmoothingEnabled = false;
  pane.ctx.fillStyle = termBgColor();
  pane.ctx.fillRect(0, 0, width, height);
}

async function paintFrame(sessionId: string, force = false) {
  const pane = state.panes.find((p) => p.session?.id === sessionId);
  if (!pane || pane.exited) return;
  const raw = toBytes(await invoke("session_frame", { id: sessionId, force }).catch(() => new Uint8Array()));
  if (raw.byteLength < 16) {
    // No new screen state (e.g. a cursor-move/OSC escape that left the screen
    // unchanged). Keep the last frame — clearing the canvas here is what made
    // the terminal flash/blink during normal output.
    return;
  }
  const view = new DataView(raw.buffer, raw.byteOffset, raw.byteLength);
  const width = view.getUint32(0, true);
  const height = view.getUint32(4, true);
  const nextW = view.getUint32(8, true) || pane.cellW;
  const nextH = view.getUint32(12, true) || pane.cellH;
  const cellChanged = nextW !== pane.cellW || nextH !== pane.cellH;
  pane.cellW = nextW;
  pane.cellH = nextH;
  pane.rasterScale = displayScale();
  if (!width || !height) {
    clearPaneSurface(pane);
    return;
  }
  const pixels = raw.subarray(16);
  if (pixels.byteLength < width * height * 4) {
    clearPaneSurface(pane);
    return;
  }
  // Copy into ImageData then putImageData (sync) — avoids createImageBitmap overhead (#54).
  const copy = new Uint8ClampedArray(pixels.byteLength);
  copy.set(pixels);
  const image = new ImageData(copy, width, height);
  const dpr = pane.rasterScale;
  pane.canvas.style.width = `${width / dpr}px`;
  pane.canvas.style.height = `${height / dpr}px`;
  if (pane.canvas.width !== width || pane.canvas.height !== height) {
    pane.canvas.width = width;
    pane.canvas.height = height;
  }
  pane.paintGen += 1;
  pane.ctx.setTransform(1, 0, 0, 1, 0, 0);
  pane.ctx.imageSmoothingEnabled = false;
  pane.ctx.putImageData(image, 0, 0);
  if (cellChanged) scheduleLayout();
}

function applyAppearance() {
  const a = state.appearance;
  if (!a) return;
  const theme = state.themes.find((t) => t.id === a.theme_id) ?? state.themes[0];
  if (theme) {
    applyChrome(theme);
    document.documentElement.style.setProperty("--term-bg", theme.background);
  }
  document.documentElement.style.setProperty("--font-mono", a.font_family);
  $("status-theme").textContent = theme?.name ?? a.theme_id;
  state.customCss.textContent = a.custom_css;
  scheduleLayout();
  for (const pane of state.panes) {
    if (pane.session) scheduleFrame(pane.session.id, true);
    else clearPaneSurface(pane);
  }
}

async function refreshSide() {
  const [hosts, hostsRuntime, groups, identities, snippets, history, forwards, running] =
    await Promise.all([
      invoke<Host[]>("hosts_list"),
      invoke<HostRuntime[]>("hosts_runtime").catch(() => [] as HostRuntime[]),
      invoke<Group[]>("groups_list").catch(() => [] as Group[]),
      invoke<Identity[]>("identities_list"),
      invoke<Snippet[]>("snippets_list"),
      invoke<HistoryEntry[]>("history_search", { query: "", limit: 80 }),
      invoke<PortForward[]>("forwards_list").catch(() => [] as PortForward[]),
      invoke<string[]>("forwards_running").catch(() => [] as string[]),
    ]);
  state.hosts = hosts;
  state.hostsRuntime = hostsRuntime;
  state.groups = groups;
  state.identities = identities;
  state.snippets = snippets;
  state.history = history;
  state.forwards = forwards;
  state.forwardsRunning = new Set(running);
  renderHosts();
  renderSnippets();
  renderHistory();
  renderForwards();
  renderTabs();
}

function applyHostRuntime(rt: HostRuntime) {
  const idx = state.hostsRuntime.findIndex((r) => r.host_id === rt.host_id);
  if (idx >= 0) state.hostsRuntime[idx] = rt;
  else state.hostsRuntime.push(rt);
  renderHosts();
}

async function refreshHostsRuntime() {
  try {
    state.hostsRuntime = await invoke<HostRuntime[]>("hosts_runtime");
    renderHosts();
  } catch {
    /* ignore — sidebar stays on last known runtime */
  }
}

async function refreshSync() {
  const status = await invoke<SyncStatus>("sync_status");
  const syncState = status.state ?? (status.configured ? (status.last_error ? "error" : "idle") : "unconfigured");

  const stateConfig: Record<string, { label: string; icon: string; color: string }> = {
    unconfigured: { label: "Sync not configured", icon: icons.cloud, color: "var(--tertiary)" },
    idle: { label: "Up to date", icon: icons.cloud, color: "var(--green)" },
    syncing: { label: "Syncing...", icon: icons.cloud, color: "var(--blue)" },
    offline: { label: "Offline", icon: icons.cloud, color: "var(--yellow)" },
    error: { label: "Sync error", icon: icons.cloud, color: "var(--red)" },
  };

  const config = stateConfig[syncState] ?? stateConfig.idle;
  const el = $("status-sync");
  el.innerHTML = `<span class="sync-icon" style="color: ${config.color};">${config.icon}</span><span class="sync-label">${config.label}</span>`;
  el.setAttribute("data-testid", "sync-badge");
  el.setAttribute("data-state", syncState);
  el.setAttribute("role", "button");
  el.tabIndex = 0;
  el.style.color = config.color;
  el.style.cursor = "pointer";

  let detail = document.getElementById("sync-detail-error");
  if (!detail) {
    detail = document.createElement("div");
    detail.id = "sync-detail-error";
    detail.setAttribute("data-testid", "sync-detail-error");
    detail.className = "sync-detail-error";
    detail.hidden = true;
    el.insertAdjacentElement("afterend", detail);
  }
  const errText = status.last_error ?? "";
  detail.textContent = errText;
  detail.hidden = true;

  const toggle = () => {
    if (syncState === "error" && errText) {
      detail!.hidden = !detail!.hidden;
    } else {
      detail!.hidden = true;
    }
  };
  el.onclick = toggle;
  el.onkeydown = (ev) => {
    if (ev.key === "Enter" || ev.key === " ") {
      ev.preventDefault();
      toggle();
    }
  };
}

function hostPanes(hostId?: string | null) {
  return state.panes.filter((p) => (hostId ? p.session?.host_id === hostId || p.pending?.hostId === hostId : p.session?.kind === "local" || p.pending?.kind === "local"));
}

async function closeBackendSession(sessionId: string) {
  await invoke("session_close", { id: sessionId }).catch(() => undefined);
}

/** Kill backend sessions that no longer belong to any open tab. */
async function closeOrphanSessions(kind: "local" | "ssh", hostId?: string) {
  const live = await invoke<SessionInfo[]>("session_list").catch(() => [] as SessionInfo[]);
  const kept = new Set(
    state.panes.flatMap((p) => (p.session?.id && !p.aborted ? [p.session.id] : [])),
  );
  await Promise.all(
    live
      .filter((s) => {
        if (kept.has(s.id)) return false;
        if (kind === "local") return s.kind === "local" || !s.host_id;
        return !!hostId && s.host_id === hostId;
      })
      .map((s) => closeBackendSession(s.id)),
  );
}

function renderHosts() {
  const q = $input("host-filter").value.toLowerCase();
  const active = activePane();
  const localOpen = hostPanes().length;
  
  // Filter hosts and groups by search
  const filteredHosts = state.hosts.filter((h) =>
    `${h.name} ${h.hostname} ${h.username} ${h.notes} ${h.tags.join(" ")}`.toLowerCase().includes(q),
  );
  const matchingGroupIds = new Set(
    state.groups.filter((g) => g.name.toLowerCase().includes(q)).map((g) => g.id)
  );
  
  // Helper to expand all ancestors of a group
  const expandAncestors = (groupId: string) => {
    const group = state.groups.find((g) => g.id === groupId);
    if (group?.parent_id) {
      expandedBySearch.add(group.parent_id);
      expandAncestors(group.parent_id);
    }
  };
  
  // If a host matches, expand its group and all ancestor groups
  const expandedBySearch = new Set<string>();
  for (const host of filteredHosts) {
    if (host.group_id) {
      expandedBySearch.add(host.group_id);
      expandAncestors(host.group_id);
    }
  }
  
  // If a group matches, expand its ancestors
  for (const groupId of matchingGroupIds) {
    expandAncestors(groupId);
  }
  
  // Build group hierarchy
  const rootGroups: Group[] = [];
  const childGroups = new Map<string, Group[]>();
  
  for (const group of state.groups) {
    if (group.deleted_at) continue;
    if (!group.parent_id) {
      rootGroups.push(group);
    } else {
      const siblings = childGroups.get(group.parent_id) ?? [];
      siblings.push(group);
      childGroups.set(group.parent_id, siblings);
    }
  }
  
  // Helper to render a single host
  const hostRow = (h: Host) => {
    const runtime = state.hostsRuntime.find((r) => r.host_id === h.id);
    const connection = runtime?.connection ?? "disconnected";
    const openCount = Math.max(
      runtime?.open_count ?? 0,
      hostPanes(h.id).filter((p) => p.session && !p.exited).length,
    );
    const isActive = hostPanes(h.id).some((p) => p.id === state.activePane);
    const userAtHost = `${h.username}@${h.hostname}${h.port !== 22 ? `:${h.port}` : ""}`;
    const classes = hostCardClassList({ openCount, active: isActive, kind: "host" });
    const dotClass = connectionDotClassList(connection);
    const dotColor = connectionDotColor(connection);
    const aria = hostCardAriaStatus(connection);
    const countHtml = shouldShowSessionCount(openCount)
      ? `<span class="sess-count" data-focus="${h.id}" data-testid="open-count-pill" title="${openCount} sessions">${sessionCountLabel(openCount)}</span>`
      : "";
    const keyHtml = h.identity_id
      ? `<span class="host-key-badge" data-testid="host-identity-icon" title="SSH identity">${icons.key}</span>`
      : "";
    return `<div class="${classes}" data-host="${h.id}" data-testid="host-${h.id}" title="${escapeHtml(userAtHost)}" tabindex="0">
        ${hostLeading(h)}
        <div class="body">
          <strong class="host-title">${escapeHtml(h.name || h.hostname)}</strong>
          <small class="host-subtitle"><span class="host-user">${escapeHtml(h.username)}</span><span class="host-sep">@</span><span class="host-addr">${escapeHtml(h.hostname)}${h.port !== 22 ? `:${h.port}` : ""}</span>${keyHtml}</small>
        </div>
        <span class="host-actions">
          <button type="button" class="quick" data-new="${h.id}" data-testid="host-action-new" title="New session" aria-label="New session">${icons.plus}</button>
          <button type="button" class="more" data-more="${h.id}" data-testid="host-action-more" title="More actions" aria-label="More actions" aria-haspopup="menu">${icons.more}</button>
        </span>
        <span class="host-status">
          <span class="${dotClass}" style="background: ${dotColor};" data-testid="connection-dot" data-state="${connection}" role="status" aria-label="${escapeHtml(aria)}"></span>
          ${countHtml}
        </span>
      </div>`;
  };
  
  // Helper to render a group and its contents
  const renderGroup = (group: Group, depth = 0): string => {
    const groupHosts = filteredHosts.filter((h) => h.group_id === group.id);
    const children = childGroups.get(group.id) ?? [];
    const isExpanded = state.expandedGroups.has(group.id) || expandedBySearch.has(group.id) || matchingGroupIds.has(group.id);
    const totalHosts = groupHosts.length;
    
    // Skip empty groups unless they match search
    if (!q && totalHosts === 0 && children.length === 0) return "";
    
    let html = `<div class="group-row ${isExpanded ? "expanded" : ""}" data-group="${group.id}">
      <span class="chevron">${icons.chevronRight}</span>
      <span class="leading">${icons.folder}</span>
      <div class="body">
        <strong>${escapeHtml(group.name)}</strong>
      </div>
    </div>`;
    
    if (isExpanded) {
      html += `<div class="group-children">`;
      // Render hosts in this group
      for (const host of groupHosts) {
        html += hostRow(host);
      }
      // Render child groups
      for (const child of children) {
        html += renderGroup(child, depth + 1);
      }
      html += `</div>`;
    }
    
    return html;
  };
  
  // Build the panel HTML
  const localActive = active?.session?.kind === "local" || active?.pending?.kind === "local";
  const localClasses = hostCardClassList({ openCount: localOpen, active: !!localActive, kind: "local" });
  const localDotClass = connectionDotClassList("local");
  const localDotColor = connectionDotColor("local");
  const localAria = hostCardAriaStatus("local");
  const localCountHtml = shouldShowSessionCount(localOpen)
    ? `<span class="sess-count" data-testid="open-count-pill" title="${localOpen} sessions">${sessionCountLabel(localOpen)}</span>`
    : "";
  let panelHtml = `<div class="${localClasses}" data-local="1" data-testid="host-local" tabindex="0">
      ${localLeading()}
      <div class="body">
        <strong class="host-title">This computer</strong>
        <small class="host-subtitle">${localOpen ? `${localOpen} open shell${localOpen > 1 ? "s" : ""}` : "Local shell"}</small>
      </div>
      <span class="host-actions">
        <button type="button" class="quick" data-new-local="1" data-testid="host-action-new" title="New session" aria-label="New session">${icons.plus}</button>
      </span>
      <span class="host-status">
        <span class="${localDotClass}" style="background: ${localDotColor};" data-testid="connection-dot" data-state="local" role="status" aria-label="${escapeHtml(localAria)}"></span>
        ${localCountHtml}
      </span>
    </div>`;
  
  const ungrouped = filteredHosts.filter((h) => !h.deleted_at && !h.group_id);
  for (const host of ungrouped) {
    panelHtml += hostRow(host);
  }

  for (const group of rootGroups) {
    panelHtml += renderGroup(group);
  }
  
  $("panel-hosts").innerHTML = panelHtml;

  // Empty only for 0 real hosts (list already excludes soft-deleted). Search ≠ zero-host empty.
  const noFilter = !q.trim();
  if (!state.hosts.length && noFilter) {
    $("panel-hosts").insertAdjacentHTML(
      "beforeend",
      `<div class="empty" data-testid="empty-hosts">
        ${icons.server}
        <span class="empty-title">No remote hosts yet</span>
        <div class="empty-actions">
          <button type="button" class="primary" id="empty-add-host" data-testid="empty-add-host">Add host</button>
          <button type="button" class="ghost" id="empty-import-hosts" data-testid="empty-import-hosts">Import known_hosts…</button>
        </div>
      </div>`,
    );
    $("empty-add-host").onclick = () => editHost();
    $("empty-import-hosts").onclick = () => void importKnownHosts();
  } else if (!filteredHosts.length) {
    $("panel-hosts").insertAdjacentHTML(
      "beforeend",
      `<div class="empty" data-testid="empty-hosts-match">${icons.search}<span class="empty-title">No hosts match</span></div>`,
    );
  }

  // Bind event handlers
  $("panel-hosts").querySelector<HTMLElement>("[data-local]")!.onclick = () => focusOrOpenLocal();
  $("panel-hosts").querySelectorAll<HTMLButtonElement>("[data-new-local]").forEach((btn) => {
    btn.onclick = (ev) => {
      ev.stopPropagation();
      void openLocal();
    };
  });

  // Group toggle handlers
  $("panel-hosts").querySelectorAll<HTMLElement>(".group-row").forEach((el) => {
    el.onclick = () => {
      const groupId = el.dataset.group!;
      if (state.expandedGroups.has(groupId)) {
        state.expandedGroups.delete(groupId);
      } else {
        state.expandedGroups.add(groupId);
      }
      localStorage.setItem("terminus-expanded-groups", JSON.stringify([...state.expandedGroups]));
      renderHosts();
    };
  });

  // Host handlers
  $("panel-hosts").querySelectorAll<HTMLElement>("[data-host]").forEach((el) => {
    el.onclick = () => focusOrOpenSsh(el.dataset.host!);
    el.oncontextmenu = (ev) => {
      ev.preventDefault();
      const host = state.hosts.find((h) => h.id === el.dataset.host);
      if (!host) return;
      showMenu(ev.clientX, ev.clientY, [
        { label: "Connect", run: () => void openSsh(host.id) },
        { label: "Focus session", run: () => focusHost(host.id), hidden: !hostPanes(host.id).length },
        { label: "SFTP", run: () => openSftpFor(host.id) },
        { label: "Edit", run: () => editHost(host) },
        { danger: true, label: "Delete", run: () => void deleteHost(host) },
      ]);
    };
  });

  $("panel-hosts").querySelectorAll<HTMLElement>("[data-focus]").forEach((el) => {
    el.onclick = (ev) => {
      ev.stopPropagation();
      focusHost(el.dataset.focus!);
    };
  });

  $("panel-hosts").querySelectorAll<HTMLButtonElement>("[data-new]").forEach((btn) => {
    btn.onclick = (ev) => {
      ev.stopPropagation();
      void openSsh(btn.dataset.new!);
    };
  });

  $("panel-hosts").querySelectorAll<HTMLButtonElement>("[data-more]").forEach((btn) => {
    btn.onclick = (ev) => {
      ev.stopPropagation();
      const host = state.hosts.find((h) => h.id === btn.dataset.more);
      if (!host) return;
      const rect = btn.getBoundingClientRect();
      showMenu(rect.left, rect.bottom + 4, [
        { label: "Connect", run: () => void openSsh(host.id) },
        { label: "Focus session", run: () => focusHost(host.id), hidden: !hostPanes(host.id).length },
        { label: "SFTP", run: () => openSftpFor(host.id) },
        { label: "Edit", run: () => editHost(host) },
        { danger: true, label: "Delete", run: () => void deleteHost(host) },
      ]);
    };
  });
}

function syncHostHighlights() {
  const panel = $("panel-hosts");
  if (!panel.childElementCount) return;
  const localOpen = hostPanes();
  const localEl = panel.querySelector<HTMLElement>("[data-local]");
  if (localEl) {
    localEl.classList.toggle("open", localOpen.length > 0);
    localEl.classList.toggle("active-host", localOpen.some((p) => p.id === state.activePane));
    const small = localEl.querySelector("small");
    if (small) small.textContent = localOpen.length ? `${localOpen.length} open shell${localOpen.length > 1 ? "s" : ""}` : "Local shell";
  }
  panel.querySelectorAll<HTMLElement>("[data-host]").forEach((el) => {
    const open = hostPanes(el.dataset.host);
    el.classList.toggle("open", open.length > 0);
    el.classList.toggle("active-host", open.some((p) => p.id === state.activePane));
  });
}

function renderSnippets() {
  if (!state.snippets.length) {
    $("panel-snippets").innerHTML = `<div class="empty" data-testid="empty-snippets">
      ${icons.snippet}
      <span class="empty-title">No snippets yet</span>
      <span class="empty-hint">Save reusable commands to paste into a session.</span>
      <div class="empty-actions">
        <button type="button" class="primary" id="new-snippet" data-testid="empty-add-snippet">New snippet</button>
      </div>
    </div>`;
    $("new-snippet").onclick = () => editSnippet();
    return;
  }
  $("panel-snippets").innerHTML =
    `<div class="item" id="new-snippet"><span class="leading">${icons.plus}</span><div class="body"><strong>New snippet</strong><small>Insert text into the terminal</small></div></div>` +
    state.snippets
      .map(
        (s) => `<div class="item" data-snip="${s.id}"><span class="leading">${icons.snippet}</span><div class="body"><strong>${escapeHtml(s.title)}</strong><small>${escapeHtml(s.content)}</small></div></div>`,
      )
      .join("");
  $("new-snippet").onclick = () => editSnippet();
  $("panel-snippets").querySelectorAll<HTMLElement>("[data-snip]").forEach((el) => {
    el.onclick = () => sendText(state.snippets.find((s) => s.id === el.dataset.snip)?.content ?? "");
  });
}

function renderHistory() {
  if (!state.history.length) {
    $("panel-history").innerHTML = `<div class="empty" data-testid="empty-history">
      ${icons.clock}
      <span class="empty-title">No history yet</span>
      <span class="empty-hint">Commands you run will appear here.</span>
    </div>`;
    return;
  }
  $("panel-history").innerHTML = state.history
    .map((h) => `<div class="item" data-hist="${h.id}"><span class="leading">${icons.clock}</span><div class="body"><strong>${escapeHtml(h.command)}</strong><small>${h.session_kind} · ${h.created_at.slice(11, 19)}</small></div></div>`)
    .join("");
  $("panel-history").querySelectorAll<HTMLElement>("[data-hist]").forEach((el) => {
    el.onclick = () => sendText((state.history.find((h) => h.id === el.dataset.hist)?.command ?? "") + "\r");
  });
}

function forwardFormHtml(): string {
  if (!state.hosts.length || !shouldShowCreateForm(forwardUi)) return "";
  const hostOpts = state.hosts
    .map((h) => `<option value="${escapeHtml(h.id)}">${escapeHtml(h.name || h.hostname)}</option>`)
    .join("");
  return `<form class="forward-form forward-form-card" data-testid="forward-form" id="forward-form" autocomplete="off">
    <div class="fwd-route-row" data-testid="fwd-route-row">
      <input id="fwd-local-port" data-testid="fwd-local-port" class="fwd-inline-input fwd-port" type="number" min="1" max="65535" placeholder="Local" aria-label="Local port" required />
      <span class="fwd-route-arrow" aria-hidden="true">→</span>
      <input id="fwd-remote-host" data-testid="fwd-remote-host" class="fwd-inline-input fwd-host-ip" value="127.0.0.1" placeholder="127.0.0.1" aria-label="Remote host" required />
      <span class="fwd-route-sep" aria-hidden="true">:</span>
      <input id="fwd-remote-port" data-testid="fwd-remote-port" class="fwd-inline-input fwd-port" type="number" min="1" max="65535" placeholder="Port" aria-label="Remote port" required />
    </div>
    <p class="form-error hidden" id="fwd-form-error" data-testid="fwd-form-error"></p>
    <div class="fwd-actions-row" data-testid="fwd-actions-row">
      <label class="fwd-via">
        <span class="fwd-via-label" data-testid="fwd-via-label">via</span>
        <select id="fwd-host" data-testid="fwd-ssh-host" class="fwd-inline-select fwd-ssh-select" aria-label="SSH host">${hostOpts}</select>
      </label>
      <div class="forward-form-actions">
        <button type="button" class="ghost" data-testid="fwd-form-cancel" id="fwd-form-cancel">Cancel</button>
        <button type="submit" class="primary" data-testid="fwd-form-submit">Add forward</button>
      </div>
    </div>
  </form>`;
}

function bindForwardForm() {
  const form = document.getElementById("forward-form") as HTMLFormElement | null;
  if (!form) return;
  const cancel = document.getElementById("fwd-form-cancel");
  if (cancel) {
    cancel.onclick = () => {
      forwardUi = closeCreateForm(forwardUi);
      renderForwards();
      syncForwardAddBtn();
    };
  }
  form.onsubmit = async (ev) => {
    ev.preventDefault();
    const result = validateForwardForm({
      hostId: ($("fwd-host") as HTMLSelectElement).value,
      localPort: ($("fwd-local-port") as HTMLInputElement).value,
      remoteHost: ($("fwd-remote-host") as HTMLInputElement).value,
      remotePort: ($("fwd-remote-port") as HTMLInputElement).value,
    });
    const errEl = $("fwd-form-error");
    if (!result.ok) {
      errEl.textContent = result.error;
      errEl.classList.remove("hidden");
      return;
    }
    errEl.classList.add("hidden");
    const now = new Date().toISOString();
    const payload: PortForward = {
      id: crypto.randomUUID(),
      host_id: result.hostId,
      kind: "local",
      name: result.name,
      bind_host: result.bindHost,
      bind_port: result.bindPort,
      dest_host: result.destHost,
      dest_port: result.destPort,
      created_at: now,
      updated_at: now,
      deleted_at: null,
    };
    await invoke("forwards_upsert", { forward: payload });
    forwardUi = closeCreateForm(forwardUi);
    await refreshSide();
    syncForwardAddBtn();
  };
}

function syncForwardAddBtn() {
  const btn = document.getElementById("btn-forward-add") as HTMLButtonElement | null;
  if (!btn) return;
  const show = navState.active === "forwards" && navState.sidebarOpen && state.hosts.length > 0;
  const open = shouldShowCreateForm(forwardUi);
  const mode = forwardAddBtnMode(open);
  btn.classList.toggle("hidden", !show);
  btn.setAttribute("aria-expanded", open ? "true" : "false");
  btn.setAttribute("data-mode", mode);
  btn.setAttribute("aria-label", mode === "close" ? "Close create forward" : "Add forward");
  btn.title = mode === "close" ? "Close" : "Add forward";
  btn.classList.toggle("active", open);
  btn.innerHTML = mode === "close" ? icons.close : icons.plus;
}

function renderForwards() {
  const panel = $("panel-forwards");
  const q = $input("host-filter").value.toLowerCase().trim();
  if (!state.forwards.length && !state.hosts.length) {
    panel.innerHTML = `<div class="empty" data-testid="empty-forwards">
      ${icons.tunnel}
      <span class="empty-title">No port forwards yet</span>
      <span class="empty-hint">Tunnel a local port through an SSH host to a remote destination.</span>
      <div class="empty-actions">
        <button type="button" class="primary" id="new-forward" data-testid="empty-add-forward">New forward</button>
      </div>
    </div>`;
    $("new-forward").onclick = () => editForward();
    syncForwardAddBtn();
    return;
  }

  const filtered = state.forwards.filter((f) => {
    if (!q) return true;
    const host = state.hosts.find((h) => h.id === f.host_id);
    const hay = `${f.name} ${f.bind_host} ${f.bind_port} ${f.dest_host} ${f.dest_port} ${host?.name ?? ""} ${host?.hostname ?? ""}`.toLowerCase();
    return hay.includes(q);
  });

  const rows = buildForwardRows(filtered, state.forwardsRunning);
  const listHtml = rows.length
    ? rows
        .map((row) => {
          const host = state.hosts.find((h) => h.id === row.hostId);
          const hostLabel = host ? host.name || host.hostname : "missing host";
          const subtitle = formatForwardSubtitle({
            bindHost: row.bindHost,
            bindPort: row.bindPort,
            destHost: row.destHost,
            destPort: row.destPort,
            sshLabel: hostLabel,
          });
          return `<div class="item forward-item" data-forward="${escapeHtml(row.id)}" data-testid="forward-${escapeHtml(row.id)}">
          <span class="leading">${icons.tunnel}</span>
          <div class="body">
            <strong class="host-title">${escapeHtml(row.name)}</strong>
            <small class="host-subtitle">${escapeHtml(subtitle)}</small>
          </div>
          <span class="forward-state" data-testid="forward-state-${escapeHtml(row.id)}" data-state="${row.state}">${row.state}</span>
          <label class="forward-switch" title="${row.active ? "Stop" : "Start"}">
            <input type="checkbox" role="switch" data-action="toggle" data-id="${escapeHtml(row.id)}" data-testid="forward-toggle-${escapeHtml(row.id)}" ${row.active ? "checked" : ""} />
            <span class="forward-switch-ui" aria-hidden="true"></span>
          </label>
          <button type="button" class="ghost" data-action="edit" data-id="${escapeHtml(row.id)}" data-testid="forward-edit-${escapeHtml(row.id)}">Edit</button>
        </div>`;
        })
        .join("")
    : `<div class="empty compact" data-testid="empty-forwards-list">
        <span class="empty-hint">${q ? "No forwards match." : "No forwards yet — press + to create one."}</span>
      </div>`;

  panel.innerHTML = forwardFormHtml() + `<div class="forward-list">${listHtml}</div>`;
  bindForwardForm();
  syncForwardAddBtn();
  if (shouldShowCreateForm(forwardUi)) {
    const localPort = document.getElementById("fwd-local-port") as HTMLInputElement | null;
    localPort?.focus();
  }
  panel.querySelectorAll<HTMLInputElement>('input[data-action="toggle"]').forEach((input) => {
    input.onchange = () => {
      const id = input.dataset.id ?? "";
      const action = toggleActionFromChecked(input.checked);
      if (action === "start") void startForward(id);
      else void stopForward(id);
    };
  });
  panel.querySelectorAll<HTMLButtonElement>('button[data-action="edit"]').forEach((btn) => {
    btn.onclick = (ev) => {
      ev.stopPropagation();
      const id = btn.dataset.id ?? "";
      const fwd = state.forwards.find((f) => f.id === id);
      if (fwd) editForward(fwd);
    };
  });
}

async function startForward(id: string) {
  try {
    await invoke("forward_start", { id });
    state.forwardsRunning = applyToggleSuccess(state.forwardsRunning, id, "start");
    renderForwards();
  } catch (err) {
    state.forwardsRunning = applyToggleFailure(state.forwardsRunning, id, "start");
    openSheet(
      `<h2>Forward failed</h2><p class="form-error">${escapeHtml(ipcErrorText(err))}</p><div class="row"><button class="primary" id="fwd-err-ok">Close</button></div>`,
    );
    $("fwd-err-ok").onclick = () => {
      $("modal").classList.add("hidden");
      void refreshSide();
    };
  }
}

async function stopForward(id: string) {
  try {
    await invoke("forward_stop", { id });
  } catch {
    /* already stopped */
  }
  state.forwardsRunning = applyToggleSuccess(state.forwardsRunning, id, "stop");
  renderForwards();
}

function editForward(existing?: PortForward) {
  if (!state.hosts.length) {
    openSheet(
      `<h2>No hosts</h2><p class="lead">Add an SSH host before creating a port forward.</p><div class="row"><button class="primary" id="fwd-nohost-ok">Close</button></div>`,
    );
    $("fwd-nohost-ok").onclick = () => $("modal").classList.add("hidden");
    return;
  }
  const fwd = existing ?? {
    id: crypto.randomUUID(),
    host_id: state.hosts[0]!.id,
    kind: "local",
    name: "",
    bind_host: "127.0.0.1",
    bind_port: 8080,
    dest_host: "127.0.0.1",
    dest_port: 80,
    created_at: new Date().toISOString(),
    updated_at: new Date().toISOString(),
    deleted_at: null,
  };
  const hostOpts = state.hosts
    .map(
      (h) =>
        `<option value="${escapeHtml(h.id)}" ${h.id === fwd.host_id ? "selected" : ""}>${escapeHtml(h.name || h.hostname)}</option>`,
    )
    .join("");
  openSheet(`
    <h2>${existing ? "Edit forward" : "New forward"}</h2>
    <p class="lead">Local bind → SSH host → remote destination (local forward only).</p>
    <div class="group-card">
      <label class="cell stack"><span>Name</span><input id="f-name" value="${escapeHtml(fwd.name)}" placeholder="Postgres tunnel" /></label>
      <label class="cell stack"><span>SSH host</span><select id="f-host">${hostOpts}</select></label>
      <label class="cell stack"><span>Bind host</span><input id="f-bind-host" value="${escapeHtml(fwd.bind_host)}" /></label>
      <label class="cell stack"><span>Bind port</span><input id="f-bind-port" type="number" min="1" max="65535" value="${fwd.bind_port}" /></label>
      <label class="cell stack"><span>Destination host</span><input id="f-dest-host" value="${escapeHtml(fwd.dest_host || "127.0.0.1")}" /></label>
      <label class="cell stack"><span>Destination port</span><input id="f-dest-port" type="number" min="1" max="65535" value="${fwd.dest_port ?? 80}" /></label>
    </div>
    <p class="form-error hidden" id="f-error"></p>
    <div class="row">
      ${existing ? `<button id="f-del" class="danger" data-testid="forward-delete">Delete</button>` : ""}
      <button class="primary" id="f-save" data-testid="forward-save">Save</button>
    </div>`);
  const showErr = (msg: string) => {
    const el = $("f-error");
    el.textContent = msg;
    el.classList.toggle("hidden", !msg);
  };
  $("f-save").onclick = async () => {
    const name = ($("f-name") as HTMLInputElement).value.trim();
    const bindHost = ($("f-bind-host") as HTMLInputElement).value.trim() || "127.0.0.1";
    const bindPort = Number(($("f-bind-port") as HTMLInputElement).value);
    const destHost = ($("f-dest-host") as HTMLInputElement).value.trim() || "127.0.0.1";
    const destPort = Number(($("f-dest-port") as HTMLInputElement).value);
    const hostId = ($("f-host") as HTMLSelectElement).value;
    if (!name) return showErr("Name is required.");
    if (!Number.isInteger(bindPort) || bindPort < 1 || bindPort > 65535) {
      return showErr("Bind port must be 1–65535.");
    }
    if (!Number.isInteger(destPort) || destPort < 1 || destPort > 65535) {
      return showErr("Destination port must be 1–65535.");
    }
    const payload: PortForward = {
      ...fwd,
      name,
      host_id: hostId,
      kind: "local",
      bind_host: bindHost,
      bind_port: bindPort,
      dest_host: destHost,
      dest_port: destPort,
      updated_at: new Date().toISOString(),
      deleted_at: null,
    };
    await invoke("forwards_upsert", { forward: payload });
    $("modal").classList.add("hidden");
    await refreshSide();
  };
  if (existing) {
    $("f-del").onclick = async () => {
      await invoke("forwards_delete", { id: existing.id });
      $("modal").classList.add("hidden");
      await refreshSide();
    };
  }
}

function hostLeading(h: Host): string {
  const { icon, os } = hostOsIcon(h.os_id);
  const data = os ? ` data-os="${escapeHtml(os)}"` : "";
  const title = os ? ` title="${escapeHtml(os)}"` : "";
  return `<span class="leading"${data}${title}>${icon}</span>`;
}

function localLeading(): string {
  const { icon, os } = hostOsIcon(state.localOsId);
  const data = os ? ` data-os="${escapeHtml(os)}"` : "";
  const title = os ? ` title="${escapeHtml(os)}"` : ' title="This computer"';
  return `<span class="leading"${data}${title}>${icon}</span>`;
}

function escapeHtml(value: string) {
  return value.replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c]!));
}

function applyNavState() {
  const open = navState.sidebarOpen;
  $("app").classList.toggle("sidebar-open", open);
  $("btn-sidebar").setAttribute("aria-expanded", open ? "true" : "false");
  for (const id of ACTIVITIES) {
    const btn = document.querySelector<HTMLButtonElement>(`[data-activity="${id}"]`);
    if (btn) {
      btn.classList.toggle("active", navState.active === id);
      btn.setAttribute("aria-pressed", navState.active === id ? "true" : "false");
    }
    const panel = document.getElementById(`panel-${id}`);
    if (panel) panel.classList.toggle("hidden", !isPanelVisible(navState, id));
  }
  const placeholders: Record<ActivityId, string> = {
    hosts: "Search hosts...",
    snippets: "Search snippets...",
    history: "Search history...",
    forwards: "Search forwards...",
    sftp: sftpSearchPlaceholder("pane A"),
  };
  ($("host-filter") as HTMLInputElement).placeholder = placeholders[navState.active] ?? "Search...";
  if (navState.active === "sftp") syncSftpSearchScopeHint();
  const showHostFooter = shouldShowHostFooterActions(navState.active);
  $("btn-new-host").classList.toggle("hidden", !showHostFooter);
  $("btn-new-group").classList.toggle("hidden", !showHostFooter);
  syncForwardAddBtn();
  scheduleLayout();
}

function setActivity(id: ActivityId) {
  navState = { active: id, sidebarOpen: true };
  applyNavState();
}

function bindUi() {
  $("btn-sidebar").innerHTML = icons.sidebar;
  $("btn-new-local").innerHTML = icons.plus;
  $("btn-palette").innerHTML = icons.search;
  renderSettingsUpdateBadge();
  $("search-ico").innerHTML = icons.search;
  $("palette-ico").innerHTML = icons.search;
  $("btn-new-host").innerHTML = `${icons.plus}<span>New host</span>`;
  $("btn-new-group").innerHTML = `${icons.folder}<span>New group</span>`;
  $("tabs-prev").innerHTML = icons.chevronLeft;
  $("tabs-next").innerHTML = icons.chevronRight;
  const activityIcons: Record<ActivityId, string> = {
    hosts: icons.server,
    snippets: icons.snippet,
    history: icons.clock,
    forwards: icons.tunnel,
    sftp: icons.folder,
  };
  document.querySelectorAll<HTMLButtonElement>("#activity-bar [data-activity]").forEach((btn) => {
    const id = (btn.dataset.activity ?? "") as ActivityId;
    btn.innerHTML = activityIcons[id] ?? "";
    btn.onclick = () => {
      const prev = navState.active;
      navState = clickActivity(navState, id);
      applyNavState();
      if (navState.active === "sftp" && navState.sidebarOpen) enterSftpMode();
      else if (prev === "sftp" || navState.active !== "sftp") exitSftpMode();
    };
  });
  applyNavState();
  $("host-filter").oninput = () => {
    renderHosts();
    if (navState.active === "forwards") renderForwards();
    if (navState.active === "sftp") sftpFilterDebounce.schedule();
  };
  $("btn-forward-add").onclick = () => {
    if (!state.hosts.length) {
      editForward();
      return;
    }
    forwardUi = toggleCreateForm(forwardUi);
    renderForwards();
    syncForwardAddBtn();
  };
  syncForwardAddBtn();
  $("btn-new-host").onclick = () => editHost();
  $("btn-new-group").onclick = () => editGroup();
  $("btn-new-local").onclick = () => openLocal();
  $("btn-settings").onclick = () => {
    if (pendingAppUpdate) promptAppUpdate(pendingAppUpdate);
    else openSettings();
  };
  $("btn-palette").onclick = () => togglePalette();
  $("btn-sidebar").onclick = () => toggleSidebar();
  $("btn-sidebar").setAttribute("aria-expanded", "true");
  $("sidebar").addEventListener("transitionend", (ev) => {
    if (ev.propertyName === "width" || ev.propertyName === "flex-basis") fitWorkspace();
  });
  $("tabs-prev").onclick = () => $("tabs").scrollBy({ left: -180, behavior: "smooth" });
  $("tabs-next").onclick = () => $("tabs").scrollBy({ left: 180, behavior: "smooth" });
  $("tabs").addEventListener("scroll", () => updateTabOverflow(), { passive: true });
  $("tabs").addEventListener(
    "wheel",
    (ev) => {
      if (Math.abs(ev.deltaY) <= Math.abs(ev.deltaX)) return;
      ev.preventDefault();
      $("tabs").scrollLeft += ev.deltaY;
    },
    { passive: false },
  );
  window.addEventListener("click", () => hideMenu());
  window.addEventListener("blur", () => hideMenu());
  new ResizeObserver(() => scheduleLayout()).observe($("workspace"));
  try {
    const win = getCurrentWindow();
    $("win-close").onclick = () => void win.close();
    $("win-min").onclick = () => void win.minimize();
    $("win-max").onclick = () => void win.toggleMaximize();
    $("titlebar").ondblclick = (ev) => {
      if ((ev.target as HTMLElement).closest("button")) return;
      void win.toggleMaximize();
    };
    const dragChrome = (ev: MouseEvent) => {
      if (ev.button !== 0) return;
      const t = ev.target as HTMLElement;
      if (t.closest("button, input, textarea, select, a, [role='tab'], [data-close], [data-tab]")) return;
      void win.startDragging();
    };
    $("titlebar").addEventListener("mousedown", dragChrome);
  } catch {
    /* vite preview has no window API */
  }
  $("modal").onclick = (ev) => {
    if (ev.target !== $("modal")) return;
    if (cancelTofuIfOpen()) return;
    hideModalSheet();
  };
  window.addEventListener("resize", () => scheduleLayout());
  window.visualViewport?.addEventListener("resize", () => scheduleLayout());
  window.addEventListener("keydown", onGlobalKey, true);
}

function closeOverlays() {
  if (cancelTofuIfOpen()) return;
  hideModalSheet();
  $("palette").classList.add("hidden");
  hideMenu();
}

function toggleSidebar(force?: boolean) {
  navState = toggleSidebarOpen(navState, force);
  applyNavState();
}

function fitWorkspace() {
  scheduleLayout();
}

function onGlobalKey(ev: KeyboardEvent) {
  if (shouldCloseCreateFormOnEscape(ev.key, forwardUi.createOpen)) {
    ev.preventDefault();
    forwardUi = closeCreateForm(forwardUi);
    renderForwards();
    syncForwardAddBtn();
    return;
  }
  if (
    state.sftpMode &&
    navState.active === "sftp" &&
    (ev.metaKey || ev.ctrlKey) &&
    ev.shiftKey &&
    ev.key === "."
  ) {
    ev.preventDefault();
    state.sftpShowHidden = !state.sftpShowHidden;
    localStorage.setItem("terminus-sftp-show-hidden", state.sftpShowHidden ? "1" : "0");
    renderSftpSidebar(state.sftpHostId);
    rerenderSftpListings();
    return;
  }
  if (ev.key === "Escape") {
    closeOverlays();
    hideMenu();
    return;
  }
  if ((ev.metaKey || ev.ctrlKey) && !ev.shiftKey && !ev.altKey && ev.key.toLowerCase() === "b") {
    ev.preventDefault();
    ev.stopPropagation();
    toggleSidebar();
    return;
  }
  const combo = [
    ev.metaKey ? "cmd" : ev.ctrlKey ? "ctrl" : "",
    ev.shiftKey ? "shift" : "",
    ev.altKey ? "alt" : "",
    ev.key.length === 1 ? ev.key.toLowerCase() : ev.key.toLowerCase(),
  ]
    .filter(Boolean)
    .join("+");
  const tabPick = combo.match(/^(?:ctrl|cmd)\+([1-9])$/);
  if (tabPick) {
    const pane = state.panes[Number(tabPick[1]) - 1];
    if (pane) {
      ev.preventDefault();
      selectPane(pane.id);
    }
    return;
  }
  const action =
    state.keybindings[combo] ??
    (combo === "ctrl+b" || combo === "cmd+b" ? "sidebar.toggle" : "");
  if (!action) return;
  ev.preventDefault();
  runAction(action);
}

function runAction(action: string) {
  switch (action) {
    case "tab.new":
      openLocal();
      break;
    case "tab.close":
      closeActive();
      break;
    case "tab.next":
      cycleTab(1);
      break;
    case "tab.prev":
      cycleTab(-1);
      break;
    case "palette.toggle":
    case "command.palette":
      togglePalette();
      break;
    case "sidebar.toggle":
      toggleSidebar();
      break;
    case "settings.toggle":
      openSettings();
      break;
    case "terminal.clear":
      sendText("\x0c");
      break;
    case "font.increase":
      bumpFont(1);
      break;
    case "font.decrease":
      bumpFont(-1);
      break;
    case "font.reset":
      if (state.appearance) {
        state.appearance.font_size = 14;
        applyAppearance();
      }
      break;
    default:
      break;
  }
}

async function bumpFont(delta: number) {
  if (!state.appearance) return;
  state.appearance.font_size = Math.max(8, Math.min(32, state.appearance.font_size + delta));
  await invoke("appearance_set", { appearance: state.appearance });
  applyAppearance();
}

function activePane() {
  return state.panes.find((p) => p.id === state.activePane);
}

function focusOrOpenLocal() {
  const open = hostPanes();
  if (open.length) selectPane(open[open.length - 1]!.id);
  else void openLocal();
}

function focusOrOpenSsh(hostId: string) {
  const open = hostPanes(hostId);
  if (open.length) selectPane(open[open.length - 1]!.id);
  else void openSsh(hostId);
}

function focusHost(hostId: string) {
  const open = hostPanes(hostId);
  if (!open.length) {
    void openSsh(hostId);
    return;
  }
  const idx = open.findIndex((p) => p.id === state.activePane);
  selectPane(open[(idx + 1) % open.length]!.id);
}

async function openLocal(reuse?: Pane) {
  if (!reuse) await closeOrphanSessions("local");
  const pane = reuse ?? createPendingPane("This computer", "local");
  try {
    const size = paneSize(pane);
    const info = await invoke<SessionInfo>("session_open_local", {
      cols: size.cols,
      rows: size.rows,
      scale: displayScale(),
    });
    if (pane.aborted || !state.panes.some((p) => p.id === pane.id)) {
      await closeBackendSession(info.id);
      return;
    }
    attachSession(info, pane);
  } catch (err) {
    if (!pane.aborted) failPane(pane, "Couldn't open a local shell", String(err));
  }
}

type HostKeyError = {
  kind: "HostKeyUnknown" | "HostKeyMismatch";
  host: string;
  port: number;
  line?: number;
  public_key: string;
  algo: string;
  fingerprint: string;
};

function ipcErrorText(err: unknown): string {
  if (typeof err === "string") return err;
  if (err instanceof Error) return err.message;
  if (err && typeof err === "object") {
    const o = err as Record<string, unknown>;
    if (typeof o.message === "string") return o.message;
    if (typeof o.kind === "string") return JSON.stringify(err);
  }
  return String(err);
}

function parseHostKeyError(err: unknown): HostKeyError | null {
  const candidates: unknown[] = [err];
  const raw = ipcErrorText(err);
  candidates.push(raw);
  const brace = raw.indexOf("{");
  if (brace > 0) candidates.push(raw.slice(brace));
  for (const candidate of candidates) {
    let parsed: Partial<HostKeyError> | null = null;
    if (candidate && typeof candidate === "object") {
      parsed = candidate as Partial<HostKeyError>;
    } else if (typeof candidate === "string") {
      try {
        parsed = JSON.parse(candidate) as Partial<HostKeyError>;
      } catch {
        continue;
      }
    }
    if (
      parsed &&
      (parsed.kind === "HostKeyUnknown" || parsed.kind === "HostKeyMismatch") &&
      typeof parsed.host === "string" &&
      typeof parsed.port === "number" &&
      typeof parsed.public_key === "string" &&
      typeof parsed.algo === "string" &&
      typeof parsed.fingerprint === "string"
    ) {
      return parsed as HostKeyError;
    }
  }
  return null;
}

let tofuSession: {
  decide: (choice: "trust" | "cancel") => void;
  onKeyDown: (ev: KeyboardEvent) => void;
} | null = null;

function modalSheetEl(): HTMLElement {
  return (
    document.getElementById("sheet-vault") ??
    document.getElementById("sheet-tofu") ??
    document.getElementById("modal-sheet")
  ) as HTMLElement;
}

function hideModalSheet() {
  const sheet = modalSheetEl();
  sheet.id = "modal-sheet";
  sheet.innerHTML = "";
  $("modal").classList.add("hidden");
}

function dismissTofuSheet() {
  if (tofuSession) {
    document.removeEventListener("keydown", tofuSession.onKeyDown, true);
    tofuSession = null;
  }
  hideModalSheet();
}

function focusableIn(root: HTMLElement): HTMLElement[] {
  return [...root.querySelectorAll<HTMLElement>(
    'button:not([disabled]), [href], input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])',
  )].filter((el) => el.offsetParent !== null);
}

function setTofuPending(pending: boolean) {
  const sheet = modalSheetEl();
  const primary = sheet.querySelector<HTMLButtonElement>("#tofu-primary");
  const cancel = sheet.querySelector<HTMLButtonElement>("#tofu-cancel");
  const close = sheet.querySelector<HTMLButtonElement>("#sheet-close");
  if (primary) {
    primary.disabled = pending;
    primary.classList.toggle("is-pending", pending);
    const label = primary.dataset.label ?? primary.textContent?.trim() ?? "";
    if (!primary.dataset.label) primary.dataset.label = label;
    primary.innerHTML = pending
      ? `<span class="btn-spinner" aria-hidden="true"></span><span>${escapeHtml(label)}</span>`
      : escapeHtml(label);
  }
  if (cancel) cancel.disabled = pending;
  if (close) close.disabled = pending;
}

function cancelTofuIfOpen(): boolean {
  if (!tofuSession) return false;
  const sheet = modalSheetEl();
  if (sheet.querySelector("#tofu-primary")?.hasAttribute("disabled")) return true;
  const decide = tofuSession.decide;
  decide("cancel");
  return true;
}

async function showTofuSheet(
  hk: HostKeyError,
  userAtHost: string,
): Promise<"trust" | "cancel"> {
  const mismatch = hk.kind === "HostKeyMismatch";
  const title = mismatch ? "Host key changed" : "Unknown host key";
  const lead = mismatch
    ? "This server’s host key does not match the key recorded in known_hosts. Someone could be intercepting the connection (MITM). Trust only if you intentionally rotated the key."
    : "Trust only if you recognize this server.";
  const primaryLabel = mismatch ? "Remove & Trust new" : "Trust & Connect";
  const warn = mismatch
    ? `<p class="tofu-warn" role="alert">WARNING: Host key mismatch. Connecting without verifying the new key risks a man-in-the-middle attack.</p>`
    : "";

  const sheet = modalSheetEl();
  sheet.id = "sheet-tofu";
  sheet.innerHTML = `
    <button type="button" class="sheet-close" id="sheet-close" title="Close">${icons.close}</button>
    <h2>${escapeHtml(title)}</h2>
    <p class="lead">${escapeHtml(lead)}</p>
    ${warn}
    <div class="group-card tofu-meta">
      <div class="cell"><span>Host</span><code class="tofu-host">${escapeHtml(userAtHost)}</code></div>
      <div class="cell stack"><span>Fingerprint (SHA256)</span><code class="tofu-fp" id="tofu-fp">${escapeHtml(hk.fingerprint)}</code></div>
      <div class="cell"><span>Algorithm</span><code class="tofu-algo">${escapeHtml(hk.algo)}</code></div>
    </div>
    <div class="row">
      <button type="button" class="ghost" id="tofu-cancel" data-testid="tofu-cancel">Cancel</button>
      <button type="button" class="${mismatch ? "danger" : "primary"}" id="tofu-primary" data-testid="tofu-primary" data-label="${escapeHtml(primaryLabel)}">${escapeHtml(primaryLabel)}</button>
    </div>`;
  $("modal").classList.remove("hidden");

  return new Promise((resolve) => {
    let decided = false;
    const cleanupKeys = () => {
      if (tofuSession) {
        document.removeEventListener("keydown", tofuSession.onKeyDown, true);
        tofuSession = null;
      }
    };

    const settle = (choice: "trust" | "cancel") => {
      if (decided) return;
      decided = true;
      if (choice === "cancel") {
        cleanupKeys();
        dismissTofuSheet();
      }
      // trust: keep sheet + trap until dismissTofuSheet after write+retry
      resolve(choice);
    };

    const onKeyDown = (ev: KeyboardEvent) => {
      if (sheet.id !== "sheet-tofu" || $("modal").classList.contains("hidden")) return;
      const pending = !!sheet.querySelector("#tofu-primary")?.hasAttribute("disabled");
      if (ev.key === "Escape") {
        ev.preventDefault();
        ev.stopImmediatePropagation();
        if (!pending && !decided) settle("cancel");
        return;
      }
      if (ev.key === "Enter" && !ev.isComposing) {
        const target = ev.target as HTMLElement | null;
        if (target && (target.tagName === "TEXTAREA" || target.isContentEditable)) return;
        ev.preventDefault();
        ev.stopImmediatePropagation();
        if (pending || decided) return;
        const primary = sheet.querySelector<HTMLButtonElement>("#tofu-primary");
        if (primary && !primary.disabled) primary.click();
        return;
      }
      if (ev.key === "Tab") {
        const nodes = focusableIn(sheet);
        if (nodes.length < 2) return;
        const first = nodes[0]!;
        const last = nodes[nodes.length - 1]!;
        if (ev.shiftKey && document.activeElement === first) {
          ev.preventDefault();
          last.focus();
        } else if (!ev.shiftKey && document.activeElement === last) {
          ev.preventDefault();
          first.focus();
        }
      }
    };

    tofuSession = {
      decide: (choice) => {
        if (choice === "cancel") settle("cancel");
      },
      onKeyDown,
    };
    document.addEventListener("keydown", onKeyDown, true);

    $("sheet-close").onclick = () => settle("cancel");
    $("tofu-cancel").onclick = () => settle("cancel");
    $("tofu-primary").onclick = () => settle("trust");

    requestAnimationFrame(() => {
      $("tofu-primary").focus();
    });
  });
}

async function openSsh(hostId: string, reuse?: Pane, afterTrust = false): Promise<boolean> {
  const host = state.hosts.find((h) => h.id === hostId);
  if (!reuse) await closeOrphanSessions("ssh", hostId);
  const pane = reuse ?? createPendingPane(host?.name || host?.hostname || "SSH", "ssh", hostId);
  try {
    const size = paneSize(pane);
    const info = await invoke<SessionInfo>("session_open_ssh", {
      hostId,
      cols: size.cols,
      rows: size.rows,
      scale: displayScale(),
    });
    if (pane.aborted || !state.panes.some((p) => p.id === pane.id)) {
      await closeBackendSession(info.id);
      return false;
    }
    attachSession(info, pane);
    return true;
  } catch (err) {
    const hk = parseHostKeyError(err);
    if (hk && host) {
      if (afterTrust) {
        dismissTofuSheet();
        const detail = ipcErrorText(err);
        failPane(pane, `Couldn't reach ${host.name || host.hostname}`, detail);
        openSheet(`<h2>SSH failed</h2><p class="form-error">Host key was saved, but the server is still untrusted. ${escapeHtml(detail)}</p><div class="row"><button class="primary" id="ssh-fail-ok">Close</button></div>`);
        $("ssh-fail-ok").onclick = () => $("modal").classList.add("hidden");
        return false;
      }
      const userAtHost = `${host.username}@${hk.host}:${hk.port}`;
      const choice = await showTofuSheet(hk, userAtHost);
      if (choice === "cancel") {
        await closePane(pane.id);
        return false;
      }
      setTofuPending(true);
      try {
        await invoke("ssh_host_key_trust", {
          host: hk.host,
          port: hk.port,
          publicKey: hk.public_key,
          public_key: hk.public_key,
          replaceLine: hk.kind === "HostKeyMismatch" ? hk.line ?? null : null,
          replace_line: hk.kind === "HostKeyMismatch" ? hk.line ?? null : null,
        });
        showBanner(pane, `Connecting to ${host.name || host.hostname}…`);
        const connected = await openSsh(hostId, pane, true);
        if (connected) dismissTofuSheet();
      } catch (trustErr) {
        setTofuPending(false);
        dismissTofuSheet();
        failPane(pane, `Couldn't trust host key`, ipcErrorText(trustErr));
        openSheet(`<h2>SSH failed</h2><p class="form-error">${escapeHtml(ipcErrorText(trustErr))}</p><div class="row"><button class="primary" id="ssh-fail-ok">Close</button></div>`);
        $("ssh-fail-ok").onclick = () => $("modal").classList.add("hidden");
      }
      return Boolean(pane.session);
    }
    failPane(pane, `Couldn't reach ${host?.name || host?.hostname || "host"}`, ipcErrorText(err));
    openSheet(`<h2>SSH failed</h2><p class="form-error">${escapeHtml(ipcErrorText(err))}</p><div class="row"><button class="primary" id="ssh-fail-ok">Close</button></div>`);
    $("ssh-fail-ok").onclick = () => $("modal").classList.add("hidden");
    return false;
  } finally {
    await refreshHostsRuntime();
  }
}

function attachSession(info: SessionInfo, pane = createPane()) {
  if (pane.aborted || !state.panes.some((p) => p.id === pane.id)) {
    void closeBackendSession(info.id);
    return;
  }
  pane.session = info;
  pane.pending = undefined;
  pane.exited = false;
  pane.el.tabIndex = 0;
  hideBanner(pane);
  pane.el.onkeydown = (ev) => {
    if (!$("modal").classList.contains("hidden") || !$("palette").classList.contains("hidden")) return;
    if (handleTerminalCopy(ev, pane)) return;
    const bytes = encodeTermKey(ev);
    if (!bytes) return;
    ev.preventDefault();
    sendText(bytes);
  };
  pane.el.onpaste = (ev) => {
    const text = ev.clipboardData?.getData("text") ?? "";
    if (!text) return;
    ev.preventDefault();
    sendText(text);
  };
  selectPane(pane.id);
  layoutPane(pane);
  scheduleFrame(info.id, true);
  renderHosts();
  void refreshSide();
}

function encodeTermKey(ev: KeyboardEvent): string | null {
    if (ev.ctrlKey && ev.key.toLowerCase() === "v") return null;
    if (ev.ctrlKey && ev.key.length === 1) {
      const code = ev.key.toLowerCase().charCodeAt(0);
      if (code >= 97 && code <= 122) return String.fromCharCode(code - 96);
    }
  switch (ev.key) {
    case "Enter":
      return "\r";
    case "Backspace":
      return "\x7f";
    case "Tab":
      return "\t";
    case "Escape":
      return "\x1b";
    case "ArrowUp":
      return "\x1b[A";
    case "ArrowDown":
      return "\x1b[B";
    case "ArrowRight":
      return "\x1b[C";
    case "ArrowLeft":
      return "\x1b[D";
    case "Home":
      return "\x1b[H";
    case "End":
      return "\x1b[F";
    case "Delete":
      return "\x1b[3~";
    case "PageUp":
      return "\x1b[5~";
    case "PageDown":
      return "\x1b[6~";
    default:
      return ev.key.length === 1 && !ev.ctrlKey && !ev.altKey && !ev.metaKey ? ev.key : null;
  }
}

function createPane(pending?: Pane["pending"]): Pane {
  const el = document.createElement("div");
  el.className = "pane";
  const canvas = document.createElement("canvas");
  canvas.className = "term-canvas";
  const selLayer = document.createElement("div");
  selLayer.className = "term-selection";
  selLayer.style.display = "none";
  const banner = document.createElement("div");
  banner.className = "pane-banner hidden";
  el.append(canvas, selLayer, banner);
  $("workspace").appendChild(el);
  const ctx = canvas.getContext("2d", { alpha: false })!;
  const pane: Pane = {
    id: crypto.randomUUID(),
    canvas,
    ctx,
    el,
    banner,
    cellW: 9,
    cellH: 23,
    rasterScale: 1,
    cols: 0,
    rows: 0,
    paintGen: 0,
    selAnchor: null,
    selFocus: null,
    selLayer,
  };
  if (pending) pane.pending = pending;
  state.panes.push(pane);
  clearPaneSurface(pane);
  el.onclick = () => {
    selectPane(pane.id);
    el.focus();
  };
  el.onmousedown = (ev) => {
    if (ev.button !== 0) return;
    selectPane(pane.id);
    el.focus();
    const cell = selCellFromEvent(pane, ev);
    if (!cell) return;
    pane.selAnchor = cell;
    pane.selFocus = cell;
    renderSelection(pane);
    pane._selecting = true;
    ev.preventDefault();
  };
  window.addEventListener("mousemove", (ev) => {
    if (!pane._selecting) return;
    const cell = selCellFromEvent(pane, ev);
    if (!cell) return;
    pane.selFocus = cell;
    renderSelection(pane);
  });
  window.addEventListener("mouseup", () => {
    pane._selecting = false;
  });
  renderTabs();
  return pane;
}

function createPendingPane(title: string, kind: string, hostId?: string): Pane {
  const pane = createPane({ title, kind, hostId });
  showBanner(pane, kind === "ssh" ? `Connecting to ${title}…` : "Opening local shell…");
  selectPane(pane.id);
  renderHosts();
  return pane;
}

function selectPane(id: string) {
  if (state.sftpMode) {
    exitSftpMode();
    setActivity("hosts");
  }
  state.activePane = id;
  for (const pane of state.panes) pane.el.classList.toggle("active", pane.id === id);
  const pane = activePane();
  $("status-session").textContent = paneTitle(pane) || "idle";
  renderTabs();
  syncHostHighlights();
  requestAnimationFrame(() => {
    if (!pane || state.activePane !== pane.id) return;
    layoutPane(pane);
    if (pane.session && !pane.exited) scheduleFrame(pane.session.id, true);
    pane.el.focus();
  });
}

function cycleTab(delta: number) {
  if (!state.panes.length) return;
  const idx = state.panes.findIndex((p) => p.id === state.activePane);
  const next = state.panes[(idx + delta + state.panes.length) % state.panes.length];
  if (next) selectPane(next.id);
}

function selCellFromEvent(pane: Pane, ev: MouseEvent): { row: number; col: number } | null {
  const rect = pane.canvas.getBoundingClientRect();
  const cw = pane.cellW / pane.rasterScale;
  const ch = pane.cellH / pane.rasterScale;
  if (cw <= 0 || ch <= 0 || !pane.cols || !pane.rows) return null;
  const col = Math.max(0, Math.min(pane.cols - 1, Math.floor((ev.clientX - rect.left) / cw)));
  const row = Math.max(0, Math.min(pane.rows - 1, Math.floor((ev.clientY - rect.top) / ch)));
  return { row, col };
}

function selectionRect(pane: Pane): { r0: number; c0: number; r1: number; c1: number } | null {
  if (!pane.selAnchor || !pane.selFocus) return null;
  return {
    r0: Math.min(pane.selAnchor.row, pane.selFocus.row),
    c0: Math.min(pane.selAnchor.col, pane.selFocus.col),
    r1: Math.max(pane.selAnchor.row, pane.selFocus.row),
    c1: Math.max(pane.selAnchor.col, pane.selFocus.col),
  };
}

function renderSelection(pane: Pane): void {
  const s = selectionRect(pane);
  const layer = pane.selLayer;
  if (!s) {
    layer.style.display = "none";
    return;
  }
  const cw = pane.cellW / pane.rasterScale;
  const ch = pane.cellH / pane.rasterScale;
  layer.style.left = `${s.c0 * cw}px`;
  layer.style.top = `${s.r0 * ch}px`;
  layer.style.width = `${(s.c1 - s.c0 + 1) * cw}px`;
  layer.style.height = `${(s.r1 - s.r0 + 1) * ch}px`;
  layer.style.display = "block";
}

function clearSelection(pane: Pane): void {
  pane.selAnchor = null;
  pane.selFocus = null;
  renderSelection(pane);
}

async function copyTerminalSelection(pane: Pane, s: { r0: number; c0: number; r1: number; c1: number }) {
  try {
    const text = await invoke<string>("session_selection_text", {
      id: pane.session!.id,
      r0: s.r0,
      c0: s.c0,
      r1: s.r1,
      c1: s.c1,
    });
    await writeClipboard(text);
    if (import.meta.env.VITE_E2E === "1") (window as any).__copyLast = text;
    clearSelection(pane);
  } catch (err) {
    console.error("copy failed", err);
  }
}

function handleTerminalCopy(ev: KeyboardEvent, pane: Pane): boolean {
  if (!(ev.ctrlKey || ev.metaKey) || ev.key.toLowerCase() !== "c") return false;
  const s = selectionRect(pane);
  if (!s) return false; // no selection → let encodeTermKey send Ctrl+C (SIGINT)
  ev.preventDefault();
  void copyTerminalSelection(pane, s);
  return true;
}

async function writeClipboard(text: string): Promise<void> {
  if (navigator.clipboard?.writeText) {
    try {
      await navigator.clipboard.writeText(text);
      return;
    } catch {
      // fall through to the textarea fallback
    }
  }
  const ta = document.createElement("textarea");
  ta.value = text;
  ta.style.position = "fixed";
  ta.style.opacity = "0";
  document.body.appendChild(ta);
  ta.select();
  document.execCommand("copy");
  document.body.removeChild(ta);
}

function displayScale() {
  const dpr = window.devicePixelRatio || 1;
  return Math.min(4, Math.max(1, dpr));
}

function logicalCell(pane: Pane) {
  const scale = pane.rasterScale || 1;
  return {
    w: Math.max(1, pane.cellW / scale),
    h: Math.max(1, pane.cellH / scale),
  };
}

function paneSize(pane: Pane) {
  const padX = TERMINAL_PANE_PADDING_X_PX;
  const padY = TERMINAL_PANE_PADDING_Y_PX;
  const workspace = $("workspace");
  const width = pane.el.clientWidth || workspace.clientWidth;
  const height = pane.el.clientHeight || workspace.clientHeight;
  const innerW = Math.max(0, width - padX * 2);
  const innerH = Math.max(0, height - padY * 2);
  const cell = logicalCell(pane);
  return {
    cols: Math.max(20, Math.floor(innerW / cell.w)),
    rows: Math.max(8, Math.floor(innerH / cell.h)),
    tooSmall: innerW < 16 || innerH < 16,
  };
}

function scheduleLayout() {
  if (layoutTick) return;
  layoutTick = requestAnimationFrame(() => {
    layoutTick = 0;
    layoutPanes();
  });
}

function layoutPanes() {
  for (const pane of state.panes) layoutPane(pane);
}

function layoutPane(pane: Pane) {
  pane.el.style.padding = `${TERMINAL_PANE_PADDING_Y_PX}px ${TERMINAL_PANE_PADDING_X_PX}px`;
  pane.el.style.opacity = String(state.appearance?.opacity ?? 1);
  if (!pane.session || pane.exited) {
    clearPaneSurface(pane);
    return;
  }
  const size = paneSize(pane);
  if (size.tooSmall) {
    clearPaneSurface(pane);
    return;
  }
  const scale = displayScale();
  if (pane.cols === size.cols && pane.rows === size.rows && Math.abs(pane.rasterScale - scale) < 0.001) {
    return;
  }
  pane.cols = size.cols;
  pane.rows = size.rows;
  pane.rasterScale = scale;
  clearPaneSurface(pane);
  void invoke("session_resize", {
    id: pane.session.id,
    cols: size.cols,
    rows: size.rows,
    scale,
  }).then(() => {
    scheduleFrame(pane.session!.id, true);
  });
}

function paneTitle(pane?: Pane | null) {
  if (!pane) return "";
  const base =
    pane.session?.kind === "local"
      ? "This computer"
      : pane.session?.title || pane.pending?.title || "Shell";
  const same = state.panes.filter((p) => {
    const title =
      p.session?.kind === "local" ? "This computer" : p.session?.title || p.pending?.title || "Shell";
    return title === base;
  });
  if (same.length < 2) return base;
  return `${base} · ${same.indexOf(pane) + 1}`;
}

function paneTabIcon(pane: Pane): { icon: string; os: string } {
  const kind = pane.session?.kind ?? pane.pending?.kind;
  if (kind === "local") return hostOsIcon(state.localOsId);
  if (kind !== "ssh") return { icon: icons.laptop, os: "" };
  const hostId = pane.session?.host_id ?? pane.pending?.hostId;
  const host = hostId ? state.hosts.find((h) => h.id === hostId) : undefined;
  return host ? hostOsIcon(host.os_id) : { icon: icons.server, os: "" };
}

function renderTabs() {
  const root = $("tabs");
  const ids = state.panes.map((p) => p.id);
  const existing = [...root.querySelectorAll<HTMLElement>("[data-tab]")];
  const sameOrder = existing.length === ids.length && existing.every((el, i) => el.dataset.tab === ids[i]);
  if (!sameOrder) {
    root.innerHTML = state.panes
      .map((p) => {
        const osIco = paneTabIcon(p);
        const osAttr = osIco.os ? ` data-os="${escapeHtml(osIco.os)}"` : "";
        return `<button type="button" role="tab" data-tab="${p.id}"><span class="tab-ico"${osAttr}>${osIco.icon}</span><span class="live-dot"></span><span class="label"></span><span class="x" data-close="${p.id}">${icons.close}</span></button>`;
      })
      .join("");
    root.querySelectorAll<HTMLElement>("[data-tab]").forEach((el) => {
      el.draggable = true;
      el.onclick = (ev) => {
        const close = (ev.target as HTMLElement).closest("[data-close]") as HTMLElement | null;
        if (close) {
          ev.stopPropagation();
          void closePane(close.dataset.close!);
        } else {
          selectPane(el.dataset.tab!);
        }
      };
      el.onauxclick = (ev) => {
        if (ev.button !== 1) return;
        ev.preventDefault();
        void closePane(el.dataset.tab!);
      };
      el.ondragstart = (ev) => {
        ev.dataTransfer?.setData("text/plain", el.dataset.tab ?? "");
        el.classList.add("dragging");
      };
      el.ondragend = () => {
        root.querySelectorAll(".dragging, .drop-before").forEach((n) => n.classList.remove("dragging", "drop-before"));
      };
      el.ondragover = (ev) => {
        ev.preventDefault();
        root.querySelectorAll(".drop-before").forEach((n) => n.classList.remove("drop-before"));
        el.classList.add("drop-before");
      };
      el.ondrop = (ev) => {
        ev.preventDefault();
        const id = ev.dataTransfer?.getData("text/plain");
        if (id && id !== el.dataset.tab) movePane(id, el.dataset.tab ?? null);
      };
      el.oncontextmenu = (ev) => {
        ev.preventDefault();
        const pane = state.panes.find((p) => p.id === el.dataset.tab);
        if (!pane) return;
        showMenu(ev.clientX, ev.clientY, [
          { label: "Close", run: () => void closePane(pane.id) },
          { label: "Close others", run: () => closeOtherPanes(pane.id), hidden: state.panes.length < 2 },
          { label: "Close all", run: () => closeAllPanes(), hidden: !state.panes.length },
          { label: pane.exited ? "Reconnect" : "New session", run: () => duplicatePane(pane) },
        ]);
      };
    });
  }
  for (const pane of state.panes) {
    const btn = root.querySelector<HTMLElement>(`[data-tab="${pane.id}"]`);
    if (!btn) continue;
    btn.classList.toggle("active", pane.id === state.activePane);
    btn.classList.toggle("pending", !!pane.pending && !pane.session);
    btn.classList.toggle("exited", !!pane.exited);
    const { icon, os } = paneTabIcon(pane);
    const icoEl = btn.querySelector(".tab-ico");
    if (icoEl) {
      if (icoEl.innerHTML !== icon) icoEl.innerHTML = icon;
      if (os) icoEl.setAttribute("data-os", os);
      else icoEl.removeAttribute("data-os");
    }
    const label = btn.querySelector(".label");
    if (label) label.textContent = paneTitle(pane);
    btn.title = paneTitle(pane);
  }
  $("workspace-empty").classList.toggle("hidden", state.panes.length > 0);
  requestAnimationFrame(() => {
    const active = root.querySelector<HTMLElement>(".active");
    if (active) {
      const left = active.offsetLeft - 12;
      const right = active.offsetLeft + active.offsetWidth + 12;
      if (left < root.scrollLeft) root.scrollLeft = left;
      else if (right > root.scrollLeft + root.clientWidth) root.scrollLeft = right - root.clientWidth;
    }
    updateTabOverflow();
  });
}

function movePane(id: string, beforeId: string | null) {
  const from = state.panes.findIndex((p) => p.id === id);
  if (from < 0) return;
  const [pane] = state.panes.splice(from, 1);
  if (!pane) return;
  if (!beforeId) state.panes.push(pane);
  else {
    const to = state.panes.findIndex((p) => p.id === beforeId);
    state.panes.splice(to < 0 ? state.panes.length : to, 0, pane);
  }
  renderTabs();
}

function updateTabOverflow() {
  const tabs = $("tabs");
  const strip = $("tabstrip");
  const overflowLeft = tabs.scrollLeft > 4;
  const overflowRight = tabs.scrollLeft + tabs.clientWidth < tabs.scrollWidth - 4;
  const overflow = tabs.scrollWidth > tabs.clientWidth + 2;
  strip.classList.toggle("overflow", overflowLeft);
  strip.classList.toggle("overflow-end", overflowRight);
  strip.classList.toggle("has-nav", overflow);
  $("tabs-prev").classList.toggle("hidden", !overflow);
  $("tabs-next").classList.toggle("hidden", !overflow);
}

async function closePane(id: string) {
  const pane = state.panes.find((p) => p.id === id);
  if (!pane) return;
  pane.aborted = true;
  const sessionId = pane.session?.id;
  const hostId = pane.session?.host_id ?? pane.pending?.hostId;
  const local = (pane.session?.kind ?? pane.pending?.kind) === "local";
  if (sessionId) {
    pane.session = undefined;
    await closeBackendSession(sessionId);
  }
  pane.el.remove();
  const idx = state.panes.findIndex((p) => p.id === id);
  state.panes = state.panes.filter((p) => p.id !== id);
  if (state.activePane === id) {
    const next = state.panes[idx] ?? state.panes[idx - 1] ?? state.panes[0];
    state.activePane = next?.id ?? null;
    if (next) selectPane(next.id);
  }
  if (local) await closeOrphanSessions("local");
  else if (hostId) await closeOrphanSessions("ssh", hostId);
  renderTabs();
  await refreshSide();
  scheduleLayout();
}

async function closeOtherPanes(keepId: string) {
  for (const pane of [...state.panes]) {
    if (pane.id !== keepId) await closePane(pane.id);
  }
}

async function closeAllPanes() {
  for (const pane of [...state.panes]) await closePane(pane.id);
}

function duplicatePane(pane: Pane) {
  if (pane.exited) {
    void reconnectPane(pane);
    return;
  }
  if (pane.session?.kind === "ssh" && pane.session.host_id) void openSsh(pane.session.host_id);
  else if (pane.pending?.kind === "ssh" && pane.pending.hostId) void openSsh(pane.pending.hostId);
  else void openLocal();
}

function closeActive() {
  if (state.activePane) void closePane(state.activePane);
}

function showBanner(pane: Pane, title: string, action?: { label: string; run: () => void }, detail?: string) {
  pane.banner.classList.remove("hidden");
  pane.banner.innerHTML = `<div><p>${escapeHtml(title)}</p>${detail ? `<p class="muted">${escapeHtml(detail)}</p>` : ""}</div>${
    action ? `<button type="button">${icons.reconnect}<span>${escapeHtml(action.label)}</span></button>` : ""
  }`;
  const btn = pane.banner.querySelector("button");
  if (btn && action) btn.onclick = (ev) => {
    ev.stopPropagation();
    action.run();
  };
}

function hideBanner(pane: Pane) {
  pane.banner.classList.add("hidden");
  pane.banner.innerHTML = "";
}

function failPane(pane: Pane, title: string, detail: string) {
  pane.exited = true;
  pane.pending = pane.pending ?? { title, kind: pane.session?.kind ?? "ssh", hostId: pane.session?.host_id ?? undefined };
  showBanner(pane, title, { label: "Retry", run: () => void reconnectPane(pane) }, detail);
  renderTabs();
  renderHosts();
}

function markExited(sessionId: string) {
  const pane = state.panes.find((p) => p.session?.id === sessionId);
  if (!pane) return;
  pane.exited = true;
  const name = paneTitle(pane);
  showBanner(pane, `${name} disconnected`, { label: "Reconnect", run: () => void reconnectPane(pane) });
  renderTabs();
  renderHosts();
}

async function reconnectPane(pane: Pane) {
  const hostId = pane.session?.host_id ?? pane.pending?.hostId;
  const local = (pane.session?.kind ?? pane.pending?.kind) === "local";
  if (pane.session) await closeBackendSession(pane.session.id);
  pane.session = undefined;
  pane.exited = false;
  pane.aborted = false;
  pane.cols = 0;
  pane.rows = 0;
  pane.pending = {
    title: local ? "This computer" : state.hosts.find((h) => h.id === hostId)?.name || pane.pending?.title || "SSH",
    kind: local ? "local" : "ssh",
    hostId,
  };
  showBanner(pane, local ? "Opening local shell…" : `Connecting to ${pane.pending.title}…`);
  renderTabs();
  if (local) await openLocal(pane);
  else if (hostId) await openSsh(hostId, pane);
}

type MenuItem = { label: string; run: () => void; danger?: boolean; hidden?: boolean };

function showMenu(x: number, y: number, items: MenuItem[]) {
  const menu = $("ctx-menu");
  const visible = items.filter((i) => !i.hidden);
  if (!visible.length) return;
  menu.innerHTML = visible
    .map((item, idx) => `<button type="button" data-i="${idx}" class="${item.danger ? "danger" : ""}">${escapeHtml(item.label)}</button>`)
    .join("");
  menu.querySelectorAll<HTMLButtonElement>("button").forEach((btn) => {
    btn.onclick = (ev) => {
      ev.stopPropagation();
      hideMenu();
      visible[Number(btn.dataset.i)]?.run();
    };
  });
  menu.classList.remove("hidden");
  const pad = 8;
  const left = Math.min(x, window.innerWidth - menu.offsetWidth - pad);
  const top = Math.min(y, window.innerHeight - menu.offsetHeight - pad);
  menu.style.left = `${Math.max(pad, left)}px`;
  menu.style.top = `${Math.max(pad, top)}px`;
}

function hideMenu() {
  $("ctx-menu").classList.add("hidden");
  $("ctx-menu").innerHTML = "";
}

async function deleteHost(host: Host) {
  await invoke("hosts_delete", { id: host.id });
  await refreshSide();
}

function sendText(text: string) {
  const pane = activePane();
  if (!pane?.session) return;
  invoke("session_write", { id: pane.session.id, data: b64encode(text) });
}

function togglePalette() {
  const el = $("palette");
  el.classList.toggle("hidden");
  if (!el.classList.contains("hidden")) {
    $input("palette-input").value = "";
    renderPalette("");
    $input("palette-input").focus();
  }
}

function renderPalette(query: string) {
  const q = query.toLowerCase();
  const items: { label: string; hint: string; run: () => void }[] = [
    { label: "New local shell", hint: "session", run: () => openLocal() },
    { label: "Identities", hint: "vault", run: () => void openVault() },
    { label: "Settings", hint: "app", run: () => openSettings() },
    { label: "Sync now", hint: "cloud", run: () => invoke("sync_now").then(refreshSync) },
    ...state.hosts.map((h) => ({
      label: `SSH ${h.name}`,
      hint: `${h.username}@${h.hostname}`,
      run: () => openSsh(h.id),
    })),
    ...state.snippets.map((s) => ({
      label: `Snippet ${s.title}`,
      hint: s.content,
      run: () => sendText(s.content),
    })),
  ].filter((i) => `${i.label} ${i.hint}`.toLowerCase().includes(q));
  $("palette-results").innerHTML = items
    .slice(0, 20)
    .map((i, idx) => {
      const ico =
        i.hint === "session"
          ? icons.laptop
          : i.hint === "vault"
            ? icons.key
            : i.hint === "app"
              ? icons.settings
              : i.hint === "cloud"
                ? icons.cloud
                : i.label.startsWith("Snippet")
                  ? icons.snippet
                  : icons.server;
      return `<li class="${idx === 0 ? "active" : ""}" data-i="${idx}"><span class="leading">${ico}</span><span class="grow">${escapeHtml(i.label)}<small>${escapeHtml(i.hint)}</small></span></li>`;
    })
    .join("");
  $("palette-results").querySelectorAll<HTMLElement>("li").forEach((li) => {
    li.onclick = () => {
      items[Number(li.dataset.i)]?.run();
      $("palette").classList.add("hidden");
    };
  });
}

$input("palette-input").addEventListener("input", () => renderPalette($input("palette-input").value));
$input("palette-input").addEventListener("keydown", (ev: Event) => {
  const key = (ev as KeyboardEvent).key;
  if (key === "Escape") $("palette").classList.add("hidden");
  if (key === "Enter") {
    const first = $("palette-results").querySelector("li") as HTMLElement | null;
    first?.click();
  }
});

async function editHost(existing?: Host) {
  const host = existing ?? {
    id: crypto.randomUUID(),
    name: "",
    hostname: "",
    port: 22,
    username: "",
    auth_method: "key",
    password: "",
    identity_id: null,
    group_id: null,
    tags: [],
    notes: "",
    os_id: null,
    created_at: new Date().toISOString(),
    updated_at: new Date().toISOString(),
  };
  const defaultKeys = await invoke<string[]>("ssh_default_keys").catch(() => [] as string[]);
  const identOpts = [`<option value="">Default keys in ~/.ssh</option>`]
    .concat(
      state.identities.map(
        (i) =>
          `<option value="${escapeHtml(i.id)}" ${host.identity_id === i.id ? "selected" : ""}>${escapeHtml(i.name)}</option>`,
      ),
    )
    .join("");
  const groupOpts = [`<option value="">None</option>`]
    .concat(
      state.groups.map(
        (g) =>
          `<option value="${escapeHtml(g.id)}" ${host.group_id === g.id ? "selected" : ""}>${escapeHtml(g.name)}</option>`,
      ),
    )
    .join("");
  openSheet(`
    <h2>${existing ? "Edit host" : "New host"}</h2>
    <p class="lead">Saved connections open in one click from the sidebar.</p>
    <div class="group-title">Connection</div>
    <div class="group-card">
      <label class="cell"><span>Name</span><input id="f-name" value="${escapeHtml(host.name)}" placeholder="Production" /></label>
      <label class="cell"><span>Host</span><input id="f-host" value="${escapeHtml(host.hostname)}" placeholder="192.168.1.10" /></label>
      <label class="cell"><span>Port</span><input id="f-port" type="number" value="${host.port}" /></label>
      <label class="cell"><span>Username</span><input id="f-user" value="${escapeHtml(host.username)}" placeholder="ubuntu" /></label>
      <label class="cell"><span>Group</span><select id="f-group">${groupOpts}</select></label>
    </div>
    <div class="group-title">Authentication</div>
    <div class="group-card">
      <div class="cell">
        <span>Method</span>
        <div class="seg" id="f-auth">
          <button type="button" data-auth="key" class="${host.auth_method !== "password" ? "on" : ""}">Key</button>
          <button type="button" data-auth="password" class="${host.auth_method === "password" ? "on" : ""}">Password</button>
        </div>
      </div>
      <label class="cell" id="pass-row"><span>Password</span><input id="f-pass" type="password" value="${escapeHtml(host.password ?? "")}" /></label>
    </div>
    <div id="key-row">
      <div class="group-title">SSH key</div>
      <div class="group-card">
        <label class="cell"><span>Saved key</span><select id="f-ident">${identOpts}</select></label>
        <label class="cell stack"><span>Key file</span><input id="f-keypath" placeholder="~/.ssh/id_ed25519" list="f-key-suggestions" />
          <datalist id="f-key-suggestions">${defaultKeys.map((p) => `<option value="${escapeHtml(p)}"></option>`).join("")}</datalist>
        </label>
        <label class="cell stack"><span>Or paste private key</span><textarea id="f-keypem" rows="4" placeholder="-----BEGIN OPENSSH PRIVATE KEY-----"></textarea></label>
        <label class="cell"><span>Passphrase</span><input id="f-keypass" type="password" placeholder="Optional" /></label>
      </div>
      <p class="meta" style="margin:8px 4px 0">${defaultKeys.length ? `Found on this computer: ${defaultKeys.map(escapeHtml).join(" · ")}` : "No default keys found in ~/.ssh yet."}</p>
    </div>
    <div class="group-title">Notes</div>
    <div class="group-card">
      <label class="cell stack"><textarea id="f-notes" rows="3" placeholder="Optional">${escapeHtml(host.notes)}</textarea></label>
    </div>
    <div class="row">
      ${existing ? `<button id="f-del" class="danger">Delete</button>` : ""}
      <button class="primary" id="f-save">Save</button>
    </div>`);
  const authValue = () =>
    ($("f-auth").querySelector<HTMLButtonElement>(".on")?.dataset.auth ?? "key");
  const syncAuth = () => {
    const key = authValue() === "key";
    $("key-row").classList.toggle("hidden", !key);
    $("pass-row").classList.toggle("hidden", key);
  };
  $("f-auth").querySelectorAll<HTMLButtonElement>("button").forEach((btn) => {
    btn.onclick = () => {
      $("f-auth").querySelectorAll("button").forEach((b) => b.classList.remove("on"));
      btn.classList.add("on");
      syncAuth();
    };
  });
  syncAuth();
  $("f-save").onclick = async () => {
    const prevHostname = host.hostname;
    const prevPort = host.port;
    host.name = ($("f-name") as HTMLInputElement).value;
    host.hostname = ($("f-host") as HTMLInputElement).value;
    host.port = Number(($("f-port") as HTMLInputElement).value);
    host.username = ($("f-user") as HTMLInputElement).value;
    host.auth_method = authValue();
    host.password = ($("f-pass") as HTMLInputElement).value;
    host.notes = ($("f-notes") as HTMLTextAreaElement).value;
    host.group_id = ($("f-group") as HTMLSelectElement).value || null;
    host.updated_at = new Date().toISOString();
    if (existing && (host.hostname !== prevHostname || host.port !== prevPort)) host.os_id = null;
    if (host.auth_method === "key") {
      const selected = ($("f-ident") as HTMLSelectElement).value;
      const path = ($("f-keypath") as HTMLInputElement).value.trim();
      const pem = ($("f-keypem") as HTMLTextAreaElement).value.trim();
      const pass = ($("f-keypass") as HTMLInputElement).value;
      if (pem) {
        const identity = await invoke<Identity>("identities_upsert", {
          identity: {
            id: crypto.randomUUID(),
            name: host.name || host.hostname || "imported-key",
            kind: "key",
            private_key: pem,
            passphrase: pass || null,
            created_at: new Date().toISOString(),
            updated_at: new Date().toISOString(),
          },
        });
        host.identity_id = identity.id;
      } else if (path) {
        const identity = await invoke<Identity>("identity_import_path", {
          name: host.name || path,
          path,
          passphrase: pass || null,
        });
        host.identity_id = identity.id;
      } else if (selected) {
        host.identity_id = selected;
      } else {
        host.identity_id = null;
      }
    }
    await invoke("hosts_upsert", { host });
    $("modal").classList.add("hidden");
    await refreshSide();
  };
  const del = document.getElementById("f-del");
  if (del) {
    del.onclick = async () => {
      await invoke("hosts_delete", { id: host.id });
      $("modal").classList.add("hidden");
      await refreshSide();
    };
  }
}

function editSnippet() {
  openSheet(`
    <h2>New snippet</h2>
    <p class="lead">Click a snippet later to paste it into the active session.</p>
    <div class="group-card">
      <label class="cell stack"><span>Title</span><input id="s-title" placeholder="Restart nginx" /></label>
      <label class="cell stack"><span>Content</span><textarea id="s-content" rows="6"></textarea></label>
    </div>
    <div class="row"><button class="primary" id="s-save">Save</button></div>`);
  $("s-save").onclick = async () => {
    await invoke("snippets_upsert", {
      snippet: {
        id: crypto.randomUUID(),
        title: ($("s-title") as HTMLInputElement).value,
        content: ($("s-content") as HTMLTextAreaElement).value,
        tags: [],
        created_at: new Date().toISOString(),
        updated_at: new Date().toISOString(),
      },
    });
    $("modal").classList.add("hidden");
    await refreshSide();
  };
}

function editGroup(existing?: Group) {
  const group = existing ?? {
    id: crypto.randomUUID(),
    name: "",
    parent_id: null,
    created_at: new Date().toISOString(),
    updated_at: new Date().toISOString(),
  };
  
  const parentOpts = [`<option value="">None (top-level)</option>`].concat(
    state.groups
      .filter((g) => g.id !== group.id && !g.parent_id)
      .map((g) => `<option value="${escapeHtml(g.id)}" ${group.parent_id === g.id ? "selected" : ""}>${escapeHtml(g.name)}</option>`)
  ).join("");
  
  openSheet(`
    <h2>${existing ? "Edit group" : "New group"}</h2>
    <p class="lead">Organize your hosts into collapsible groups in the sidebar.</p>
    <div class="group-card">
      <label class="cell stack"><span>Name</span><input id="g-name" value="${escapeHtml(group.name)}" placeholder="Production servers" /></label>
      <label class="cell stack"><span>Parent group</span><select id="g-parent">${parentOpts}</select></label>
    </div>
    <div class="row">
      ${existing ? `<button id="g-del" class="danger">Delete</button>` : ""}
      <button class="primary" id="g-save">Save</button>
    </div>`);
  
  $("g-save").onclick = async () => {
    group.name = ($("g-name") as HTMLInputElement).value.trim();
    group.parent_id = ($("g-parent") as HTMLSelectElement).value || null;
    group.updated_at = new Date().toISOString();
    
    if (!group.name) return;
    
    await invoke("groups_upsert", { group });
    $("modal").classList.add("hidden");
    await refreshSide();
  };
  
  const del = document.getElementById("g-del");
  if (del) {
    del.onclick = async () => {
      // Use extracted soft-delete logic
      const affectedGroupIds = computeAffectedGroups(group.id, state.groups);
      const now = new Date().toISOString();
      
      // Soft-delete the group itself and all descendant groups
      for (const groupId of affectedGroupIds) {
        const targetGroup = state.groups.find((g) => g.id === groupId);
        if (targetGroup) {
          const deletedGroup = applySoftDelete(targetGroup, now);
          await invoke("groups_upsert", { group: deletedGroup });
        }
      }
      
      // Clear group_id on all hosts that belong to affected groups
      const orphanedHosts = findOrphanedHosts(affectedGroupIds, state.hosts);
      for (const host of orphanedHosts) {
        const detachedHostData = detachHost(host, now);
        await invoke("hosts_upsert", { host: detachedHostData });
      }
      
      $("modal").classList.add("hidden");
      await refreshSide();
    };
  }
}

async function openVault() {
  clearVaultReveal();
  await refreshSide();
  renderVaultSheet();
}

let vaultRevealTimer: ReturnType<typeof setTimeout> | null = null;
let vaultRevealId: string | null = null;

function clearVaultReveal() {
  if (vaultRevealTimer) {
    clearTimeout(vaultRevealTimer);
    vaultRevealTimer = null;
  }
  vaultRevealId = null;
}

function identityTypeLabel(kind: IdentityKind): string {
  if (kind === "password") return "password";
  if (kind === "agent") return "agent";
  return "key";
}

function identitySecret(identity: Identity): string | null {
  const kind = inferIdentityKind(identity);
  if (kind === "agent") return null;
  if (kind === "password") return identity.passphrase ?? null;
  return identity.private_key ?? identity.passphrase ?? null;
}

function attachedHostCount(identityId: string): number {
  return state.hosts.filter((h) => h.identity_id === identityId).length;
}

function renderVaultSheet() {
  const statusPromise = invoke<SyncStatus>("sync_status").catch(
    () => ({ configured: false, sync_secrets: false }) as SyncStatus,
  );
  void statusPromise.then((status) => {
    const syncSecrets = Boolean(status.sync_secrets);
    const items = state.identities
      .map((ident) => {
        const kind = inferIdentityKind(ident);
        const hosts = attachedHostCount(ident.id);
        const revealing = vaultRevealId === ident.id;
        const secret = revealing ? identitySecret(ident) : null;
        const canReveal = kind !== "agent" && Boolean(identitySecret(ident));
        const ico = kind === "password" ? icons.password : kind === "agent" ? icons.cloud : icons.key;
        return `<div class="item" data-identity="${escapeHtml(ident.id)}" data-testid="vault-identity-${escapeHtml(ident.id)}">
          <span class="leading">${ico}</span>
          <div class="body">
            <strong>${escapeHtml(ident.name || "Unnamed")}</strong>
            <small>${escapeHtml(identityTypeLabel(kind))} · ${hosts} host${hosts === 1 ? "" : "s"}</small>
            ${revealing && secret != null ? `<pre class="vault-reveal" data-testid="vault-reveal">${escapeHtml(secret)}</pre>` : ""}
          </div>
          <span class="trail">
            ${canReveal ? `<button type="button" class="ghost vault-reveal-btn" data-reveal="${escapeHtml(ident.id)}" data-testid="vault-reveal-btn">${revealing ? "Hide" : "Reveal"}</button>` : ""}
            <button type="button" class="ghost" data-edit="${escapeHtml(ident.id)}" data-testid="vault-edit-btn">Edit</button>
          </span>
        </div>`;
      })
      .join("");

    openSheet(
      `
    <h2>Identities</h2>
    <p class="lead">Keys, passwords, and agents stay on this device unless you opt in.</p>
    <div class="group-card">
      <label class="cell"><span>Sync secrets<small class="hint">Secrets stay local</small></span>
        <span class="toggle"><input id="vault-sync-secrets" type="checkbox" ${syncSecrets ? "checked" : ""} data-testid="vault-sync-secrets" /><span class="track"></span></span>
      </label>
    </div>
    <div class="vault-list" id="vault-list" data-testid="vault-list">
      ${items || `<div class="empty" data-testid="vault-empty">${icons.key}<span class="empty-title">No identities yet</span></div>`}
    </div>
    <div class="row">
      <button type="button" id="vault-add" data-testid="vault-add">Add identity</button>
    </div>`,
      "sheet-vault",
    );

    $("vault-add").onclick = () => editIdentity();
    ($("vault-sync-secrets") as HTMLInputElement).onchange = async (ev) => {
      const on = (ev.target as HTMLInputElement).checked;
      try {
        await invoke("sync_set_secrets", { syncSecrets: on });
      } catch (err) {
        (ev.target as HTMLInputElement).checked = !on;
        const msg = document.getElementById("vault-sync-msg");
        if (msg) msg.textContent = String(err);
      }
    };
    $("vault-list").querySelectorAll<HTMLButtonElement>("[data-reveal]").forEach((btn) => {
      btn.onclick = (ev) => {
        ev.stopPropagation();
        const id = btn.dataset.reveal!;
        if (vaultRevealId === id) {
          clearVaultReveal();
          renderVaultSheet();
          return;
        }
        clearVaultReveal();
        vaultRevealId = id;
        vaultRevealTimer = setTimeout(() => {
          clearVaultReveal();
          if (modalSheetEl().id === "sheet-vault") renderVaultSheet();
        }, 15_000);
        renderVaultSheet();
      };
    });
    $("vault-list").querySelectorAll<HTMLButtonElement>("[data-edit]").forEach((btn) => {
      btn.onclick = (ev) => {
        ev.stopPropagation();
        const ident = state.identities.find((i) => i.id === btn.dataset.edit);
        if (ident) editIdentity(ident);
      };
    });
  });
}

function editIdentity(existing?: Identity) {
  clearVaultReveal();
  const identity: Identity = existing
    ? { ...existing }
    : {
        id: crypto.randomUUID(),
        name: "",
        kind: "key",
        private_key: "",
        passphrase: "",
        created_at: new Date().toISOString(),
        updated_at: new Date().toISOString(),
      };
  const kind = inferIdentityKind(identity);
  openSheet(
    `
    <h2>${existing ? "Edit identity" : "Add identity"}</h2>
    <p class="lead">Secrets are never written to the console or sync events.</p>
    <div class="group-card">
      <label class="cell stack"><span>Name</span><input id="v-name" value="${escapeHtml(identity.name)}" placeholder="deploy-key" data-testid="vault-name" /></label>
      <div class="cell">
        <span>Type</span>
        <div class="seg" id="v-kind" data-testid="vault-kind">
          <button type="button" data-kind="key" class="${kind === "key" ? "on" : ""}">Key</button>
          <button type="button" data-kind="password" class="${kind === "password" ? "on" : ""}">Password</button>
          <button type="button" data-kind="agent" class="${kind === "agent" ? "on" : ""}">Agent</button>
        </div>
      </div>
      <label class="cell stack" id="v-key-row"><span>Private key<small class="hint">PEM or path like ~/.ssh/id_ed25519</small></span>
        <textarea id="v-key" rows="5" placeholder="-----BEGIN OPENSSH PRIVATE KEY-----" data-testid="vault-key">${escapeHtml(identity.private_key ?? "")}</textarea>
      </label>
      <label class="cell" id="v-pass-row"><span id="v-pass-label">Passphrase</span>
        <input id="v-pass" type="password" value="${escapeHtml(identity.passphrase ?? "")}" autocomplete="off" data-testid="vault-pass" />
      </label>
    </div>
    <p class="form-error hidden" id="v-err" data-testid="vault-error"></p>
    <div class="row">
      ${existing ? `<button type="button" id="v-attach">Attach to host</button>` : ""}
      ${existing ? `<button type="button" class="danger" id="v-del" data-testid="vault-delete">Delete</button>` : ""}
      <button type="button" class="primary" id="v-save" data-testid="vault-save">Save</button>
    </div>`,
    "sheet-vault",
  );

  const kindValue = () =>
    ($("v-kind").querySelector<HTMLButtonElement>(".on")?.dataset.kind ?? "key") as IdentityKind;
  const syncKindUi = () => {
    const k = kindValue();
    const keyRow = $("v-key-row");
    const passRow = $("v-pass-row");
    keyRow.classList.toggle("hidden", k !== "key");
    passRow.classList.toggle("hidden", k === "agent");
    $("v-pass-label").textContent = k === "password" ? "Password" : "Passphrase";
  };
  $("v-kind").querySelectorAll<HTMLButtonElement>("button").forEach((btn) => {
    btn.onclick = () => {
      $("v-kind").querySelectorAll("button").forEach((b) => b.classList.remove("on"));
      btn.classList.add("on");
      syncKindUi();
    };
  });
  syncKindUi();

  $("v-save").onclick = async () => {
    const errEl = $("v-err");
    errEl.classList.add("hidden");
    errEl.textContent = "";
    const parsed = parseIdentityKind(kindValue());
    if (!parsed.ok) {
      errEl.textContent = `Unknown identity type`;
      errEl.classList.remove("hidden");
      return;
    }
    identity.name = ($("v-name") as HTMLInputElement).value.trim();
    if (!identity.name) {
      errEl.textContent = "Name is required";
      errEl.classList.remove("hidden");
      return;
    }
    identity.kind = parsed.kind;
    identity.updated_at = new Date().toISOString();
    if (!identity.created_at) identity.created_at = identity.updated_at;
    if (parsed.kind === "key") {
      identity.private_key = ($("v-key") as HTMLTextAreaElement).value;
      identity.passphrase = ($("v-pass") as HTMLInputElement).value || null;
    } else if (parsed.kind === "password") {
      identity.private_key = null;
      identity.passphrase = ($("v-pass") as HTMLInputElement).value || null;
    } else {
      identity.private_key = null;
      identity.passphrase = null;
    }
    try {
      await invoke("identities_upsert", { identity });
      await refreshSide();
      renderVaultSheet();
    } catch (err) {
      const typed = parseIdentityKeyError(String(err));
      errEl.textContent = typed ? typed.reason : String(err);
      errEl.classList.remove("hidden");
    }
  };

  const del = document.getElementById("v-del");
  if (del) {
    del.onclick = () => {
      openSheet(
        `
      <h2>Delete identity?</h2>
      <p class="lead">Remove <strong>${escapeHtml(identity.name)}</strong> from the vault. Attached hosts keep their link until you change them.</p>
      <div class="row">
        <button type="button" id="v-del-cancel">Cancel</button>
        <button type="button" class="danger" id="v-del-confirm" data-testid="vault-delete-confirm">Delete</button>
      </div>`,
        "sheet-vault",
      );
      $("v-del-cancel").onclick = () => editIdentity(identity);
      $("v-del-confirm").onclick = async () => {
        await invoke("identities_delete", { id: identity.id });
        for (const host of state.hosts.filter((h) => h.identity_id === identity.id)) {
          host.identity_id = null;
          host.updated_at = new Date().toISOString();
          await invoke("hosts_upsert", { host });
        }
        await refreshSide();
        renderVaultSheet();
      };
    };
  }

  const attach = document.getElementById("v-attach");
  if (attach) {
    attach.onclick = () => attachIdentityToHost(identity);
  }
}

function attachIdentityToHost(identity: Identity) {
  const kind = inferIdentityKind(identity);
  const opts = state.hosts
    .map(
      (h) =>
        `<label class="cell"><span>${escapeHtml(h.name || h.hostname)}</span>
          <input type="checkbox" data-host="${escapeHtml(h.id)}" ${h.identity_id === identity.id ? "checked" : ""} /></label>`,
    )
    .join("");
  openSheet(
    `
    <h2>Attach to hosts</h2>
    <p class="lead">Link <strong>${escapeHtml(identity.name)}</strong> (${escapeHtml(identityTypeLabel(kind))}) to one or more hosts.</p>
    <div class="group-card" id="v-attach-list" data-testid="vault-attach-list">
      ${opts || `<div class="meta">No hosts yet — add a host first.</div>`}
    </div>
    <div class="row">
      <button type="button" id="v-attach-cancel">Back</button>
      <button type="button" class="primary" id="v-attach-save" data-testid="vault-attach-save">Save</button>
    </div>`,
    "sheet-vault",
  );
  $("v-attach-cancel").onclick = () => editIdentity(identity);
  $("v-attach-save").onclick = async () => {
    const boxes = [...document.querySelectorAll<HTMLInputElement>("#v-attach-list input[data-host]")];
    for (const box of boxes) {
      const host = state.hosts.find((h) => h.id === box.dataset.host);
      if (!host) continue;
      const want = box.checked;
      const has = host.identity_id === identity.id;
      if (want === has) continue;
      if (want) {
        host.identity_id = identity.id;
        host.auth_method = kind === "password" ? "password" : kind === "agent" ? "agent" : "key";
        if (kind === "password") {
          host.password = identity.passphrase ?? host.password;
        }
      } else {
        host.identity_id = null;
      }
      host.updated_at = new Date().toISOString();
      await invoke("hosts_upsert", { host });
    }
    await refreshSide();
    renderVaultSheet();
  };
}

async function openSettings() {
  const appearance = state.appearance!;
  const status = await invoke<SyncStatus>("sync_status");
  openSheet(`
    <h2>Settings</h2>
    <p class="lead">A theme restyles the window, sidebar, and terminal together.</p>
    <div class="group-title">Appearance</div>
    <div class="group-card">
      <div class="cell">
        <span>Theme<small class="hint" id="a-theme-name">${escapeHtml(state.themes.find((t) => t.id === appearance.theme_id)?.name ?? appearance.theme_id)}</small></span>
        <div class="swatches" id="a-theme">${state.themes
          .map(
            (t) =>
              `<button type="button" class="swatch ${t.id === appearance.theme_id ? "on" : ""}" data-theme="${t.id}" title="${escapeHtml(t.name)}" style="background:${t.background}"></button>`,
          )
          .join("")}</div>
      </div>
      <div class="cell"><span>Renderer<small class="hint">Rust VT + CPU glyph raster, blit to canvas</small></span><span class="meta">native</span></div>
      <label class="cell"><span>Font</span><input id="a-font" value="${escapeHtml(appearance.font_family)}" /></label>
      <label class="cell"><span>Size</span><input id="a-size" type="number" value="${appearance.font_size}" /></label>
      <label class="cell"><span>Line height</span><input id="a-lh" type="number" step="0.05" value="${appearance.line_height}" /></label>
      <label class="cell"><span>Scrollback</span><input id="a-scroll" type="number" value="${appearance.scrollback}" /></label>
      <label class="cell"><span>Padding</span><input id="a-pad" type="number" value="${appearance.padding}" /></label>
      <label class="cell"><span>Opacity</span><input id="a-op" type="number" step="0.05" min="0.3" max="1" value="${appearance.opacity}" /></label>
    </div>
    <div class="group-title">Advanced</div>
    <div class="group-card">
      <label class="cell stack"><span>Custom CSS</span><textarea id="a-css" rows="4">${escapeHtml(appearance.custom_css)}</textarea></label>
    </div>
    <div class="group-title">SQL sync</div>
    <div class="group-card">
      <label class="cell stack"><span>Database URL<small class="hint">PostgreSQL or any sqlx-compatible URL</small></span>
        <input id="sync-url" placeholder="postgres://user:pass@host:5432/terminus" value="${escapeHtml(status.url ?? "")}" />
      </label>
      <label class="cell"><span>Sync secrets<small class="hint">Secrets stay local</small></span>
        <span class="toggle"><input id="sync-secrets" type="checkbox" ${status.sync_secrets ? "checked" : ""} /><span class="track"></span></span>
      </label>
    </div>
    <div class="row">
      <button id="sync-now">Sync now</button>
      <button type="button" id="open-vault">Identities</button>
      <button class="primary" id="a-save">Save</button>
    </div>
    <div class="meta" id="sync-msg">${status.last_error ?? ""}</div>`);
  $("open-vault").onclick = () => void openVault();
  $("a-theme").querySelectorAll<HTMLButtonElement>(".swatch").forEach((btn) => {
    btn.onclick = () => {
      $("a-theme").querySelectorAll(".swatch").forEach((b) => b.classList.remove("on"));
      btn.classList.add("on");
      const theme = state.themes.find((t) => t.id === btn.dataset.theme);
      $("a-theme-name").textContent = theme?.name ?? btn.dataset.theme ?? "";
      appearance.theme_id = btn.dataset.theme ?? appearance.theme_id;
      applyAppearance();
    };
  });
  $("a-save").onclick = async () => {
    appearance.theme_id =
      $("a-theme").querySelector<HTMLButtonElement>(".swatch.on")?.dataset.theme ?? appearance.theme_id;
    appearance.renderer = pickRenderer(appearance.renderer || "auto");
    appearance.font_family = ($("a-font") as HTMLInputElement).value;
    appearance.font_size = Number(($("a-size") as HTMLInputElement).value);
    appearance.line_height = Number(($("a-lh") as HTMLInputElement).value);
    appearance.scrollback = Number(($("a-scroll") as HTMLInputElement).value);
    appearance.padding = Number(($("a-pad") as HTMLInputElement).value);
    appearance.opacity = Number(($("a-op") as HTMLInputElement).value);
    appearance.custom_css = ($("a-css") as HTMLTextAreaElement).value;
    await invoke("appearance_set", { appearance });
    const url = ($("sync-url") as HTMLInputElement).value.trim();
    const syncSecrets = ($("sync-secrets") as HTMLInputElement).checked;
    if (url) {
      try {
        await invoke("sync_configure", {
          config: { url, sync_secrets: syncSecrets },
        });
      } catch (err) {
        $("sync-msg").textContent = String(err);
        return;
      }
    } else {
      try {
        await invoke("sync_set_secrets", { syncSecrets });
      } catch (err) {
        $("sync-msg").textContent = String(err);
        return;
      }
    }
    applyAppearance();
    await refreshSync();
    $("modal").classList.add("hidden");
  };
  $("sync-now").onclick = async () => {
    try {
      const stats = await invoke("sync_now");
      $("sync-msg").textContent = JSON.stringify(stats);
      await refreshSide();
      await refreshSync();
    } catch (err) {
      $("sync-msg").textContent = String(err);
    }
  };
}

function openSheet(html: string, sheetId = "modal-sheet") {
  const sheet = modalSheetEl();
  sheet.id = sheetId;
  sheet.innerHTML = `<button type="button" class="sheet-close" id="sheet-close" title="Close">${icons.close}</button>${html}`;
  $("modal").classList.remove("hidden");
  $("sheet-close").onclick = () => {
    clearVaultReveal();
    hideModalSheet();
  };
}

function markOnboarded() {
  localStorage.setItem(ONBOARD_KEY, "1");
}

function maybeShowOnboarding() {
  // Default E2E suites must not be blocked by the modal; opt in via localStorage for C7 onboard smoke.
  if (import.meta.env.VITE_E2E === "1" && localStorage.getItem("terminus.e2e.showOnboard") !== "1") return;
  if (localStorage.getItem(ONBOARD_KEY) === "1") return;
  openSheet(`
    <h2>Get started</h2>
    <p class="lead">A short checklist for your first SSH session.</p>
    <ol class="onboard-steps" data-testid="onboard-steps">
      <li><span class="step-num">1</span><span>Add a host</span></li>
      <li><span class="step-num">2</span><span>Connect<small>You'll confirm the server fingerprint on first connect</small></span></li>
      <li><span class="step-num">3</span><span>Optional: configure sync</span></li>
    </ol>
    <div class="row">
      <button type="button" class="ghost" id="onboard-skip" data-testid="onboard-skip">Skip</button>
      <button type="button" class="primary" id="onboard-go" data-testid="onboard-go">Get started</button>
    </div>`);
  const finish = (start: boolean) => {
    markOnboarded();
    localStorage.removeItem("terminus.e2e.showOnboard");
    $("modal").classList.add("hidden");
    if (start) void editHost();
  };
  $("onboard-skip").onclick = () => finish(false);
  $("onboard-go").onclick = () => finish(true);
  $("sheet-close").onclick = () => {
    markOnboarded();
    localStorage.removeItem("terminus.e2e.showOnboard");
    $("modal").classList.add("hidden");
  };
}

/** Native file picker → fail-closed known_hosts parse → hosts without secrets. */
async function importKnownHosts() {
  const input = document.createElement("input");
  input.type = "file";
  input.accept = ".known_hosts,known_hosts,text/plain,.txt";
  input.style.display = "none";
  document.body.appendChild(input);

  const file = await new Promise<File | null>((resolve) => {
    input.onchange = () => resolve(input.files?.[0] ?? null);
    input.oncancel = () => resolve(null);
    input.click();
  });
  input.remove();
  if (!file) return;

  let text = "";
  try {
    text = await file.text();
  } catch (err) {
    openSheet(`<h2>Import failed</h2><p class="lead">${escapeHtml(String(err))}</p>
      <div class="row"><button type="button" class="primary" id="import-ok">OK</button></div>`);
    $("import-ok").onclick = () => $("modal").classList.add("hidden");
    return;
  }

  const { hosts, errors } = parseKnownHosts(text);
  const now = new Date().toISOString();
  let created = 0;
  for (const stub of hosts) {
    const host: Host = {
      id: crypto.randomUUID(),
      name: stub.hostname,
      hostname: stub.hostname,
      port: stub.port,
      username: "",
      auth_method: "key",
      password: null,
      identity_id: null,
      group_id: null,
      tags: ["imported"],
      notes: "Imported from known_hosts",
      created_at: now,
      updated_at: now,
      deleted_at: null,
    };
    try {
      await invoke("hosts_upsert", { host });
      created += 1;
    } catch {
      // fail-closed per host: skip + count as error, never crash the import
      // (secrets are never written — stubs have no password/identity)
    }
  }
  const upsertErrors = hosts.length - created;
  const totalErrors = errors + upsertErrors;
  await refreshSide();

  openSheet(`
    <h2>Import complete</h2>
    <p class="lead">Hosts were created without secrets — add a key or password before connecting.</p>
    <div class="group-card">
      <div class="cell"><span>Imported</span><strong>${created}</strong></div>
      <div class="cell"><span>Skipped / errors</span><strong>${totalErrors}</strong></div>
    </div>
    <div class="row"><button type="button" class="primary" id="import-ok" data-testid="import-ok">OK</button></div>`);
  $("import-ok").onclick = () => $("modal").classList.add("hidden");
}

function resetSftpCwd() {
  state.sftpCwd = "";
  state.sftpCwdHostId = null;
}

async function ensureSftpCwd(hostId: string) {
  if (state.sftpCwdHostId === hostId && state.sftpCwd) return;
  state.sftpCwdHostId = hostId;
  try {
    const cwd = await invoke<string>("sftp_realpath", { hostId, path: "." });
    state.sftpCwd = typeof cwd === "string" ? cwd.replace(/\/+$/, "") || cwd : "";
  } catch {
    state.sftpCwd = "";
  }
}

function openSftpFor(hostId: string) {
  sftpBrowser = openSingle(sftpBrowser, hostId);
  state.sftpHostId = hostId;
  state.sftpRoot = "/";
  state.sftpPath = ".";
  state.localSelected.clear();
  state.sftpSelected.clear();
  resetSftpCwd();
  setActivity("sftp");
  enterSftpMode();
  ensureSftpShell(true);
  void loadSftp(hostId, ".");
}

function ensureSftpView(): HTMLElement {
  let view = document.querySelector<HTMLElement>("#workspace > .sftp-view");
  if (!view) {
    view = document.createElement("div");
    view.className = "sftp-view";
    view.dataset.testid = "sftp-view";
    $("workspace").appendChild(view);
  }
  return view;
}

function paneSectionEl(slot: SftpPaneSlot): HTMLElement | null {
  return ensureSftpView().querySelector<HTMLElement>(`[data-testid="sftp-pane-${slot}"]`);
}

function paneBodyEl(slot: SftpPaneSlot): HTMLElement | null {
  return paneSectionEl(slot)?.querySelector<HTMLElement>(".sftp-pane-body") ?? null;
}

function endpointForSlot(slot: SftpPaneSlot): SftpEndpointId | null {
  if (slot === "a") return sftpBrowser.paneA;
  return sftpBrowser.paneB;
}

/** Remote-pane body for the active SFTP host (or pane A). */
function remotePaneEl(): HTMLElement {
  const view = ensureSftpView();
  if (state.sftpHostId) {
    const match = view.querySelector<HTMLElement>(
      `[data-endpoint="${state.sftpHostId}"] .sftp-pane-body`,
    );
    if (match) return match;
  }
  return (
    view.querySelector<HTMLElement>('[data-testid="sftp-pane-a"] .sftp-pane-body') ??
    view.querySelector<HTMLElement>('[data-testid="sftp-pane-a"]')!
  );
}

function localPaneEl(): HTMLElement {
  const view = ensureSftpView();
  const match = view.querySelector<HTMLElement>(
    `[data-endpoint="${SFTP_LOCAL_ID}"] .sftp-pane-body`,
  );
  if (match) return match;
  return (
    view.querySelector<HTMLElement>('[data-testid="sftp-pane-b"] .sftp-pane-body') ??
    view.querySelector<HTMLElement>('[data-testid="sftp-pane-b"]')!
  );
}

function sftpEndpointOptions(selected: SftpEndpointId | null): string {
  const localSelected = selected === SFTP_LOCAL_ID ? "selected" : "";
  const local = `<option value="${SFTP_LOCAL_ID}" ${localSelected}>This computer</option>`;
  const hosts = state.hosts
    .map(
      (h) =>
        `<option value="${escapeHtml(h.id)}" ${h.id === selected ? "selected" : ""}>${escapeHtml(h.name || h.hostname)}</option>`,
    )
    .join("");
  return local + hosts;
}

function paneChrome(slot: SftpPaneSlot, endpoint: SftpEndpointId): string {
  return `<div class="sftp-pane-head">
      <select class="sftp-pane-host" data-testid="sftp-pane-${slot}-host" data-slot="${slot}" aria-label="Host ${slot.toUpperCase()}">${sftpEndpointOptions(endpoint)}</select>
    </div>
    <div class="sftp-pane-body"></div>`;
}

/** Build single or split shell from sftpBrowser state. */
function sftpShellMatchesBrowser(view: HTMLElement): boolean {
  if (view.dataset.layoutMode !== sftpBrowser.mode) return false;
  if (!view.querySelector('[data-testid="sftp-pane-a"]')) return false;
  const a = view.querySelector<HTMLElement>('[data-testid="sftp-pane-a"]');
  if (!a || a.dataset.endpoint !== sftpBrowser.paneA) return false;
  if (sftpBrowser.mode === "split") {
    const b = view.querySelector<HTMLElement>('[data-testid="sftp-pane-b"]');
    const want = sftpBrowser.paneB ?? SFTP_LOCAL_ID;
    if (!b || b.dataset.endpoint !== want) return false;
  }
  return true;
}

function ensureSftpShell(force = false): HTMLElement {
  const view = ensureSftpView();
  const mode = sftpBrowser.mode;
  if (!force && sftpShellMatchesBrowser(view)) {
    syncPaneEndpoints();
    return view;
  }
  view.dataset.layoutMode = mode;
  syncSftpViewClasses();
  if (mode === "single") {
    view.innerHTML = `<div class="sftp-single" data-testid="sftp-single">
      <section class="sftp-pane" data-testid="sftp-pane-a" data-slot="a" data-endpoint="${escapeHtml(sftpBrowser.paneA)}">${paneChrome("a", sftpBrowser.paneA)}</section>
    </div>`;
  } else {
    const b = sftpBrowser.paneB ?? SFTP_LOCAL_ID;
    view.innerHTML = `
      <div class="sftp-transfer" data-testid="sftp-transfer"></div>
      <div class="sftp-batch-bar hidden" data-testid="sftp-batch-bar">
        <span class="sftp-batch-count" data-testid="sftp-batch-count">0 selected</span>
        <div class="sftp-batch-actions">
          <button type="button" class="ghost" data-testid="sftp-batch-clear">Clear</button>
          <button type="button" class="primary" data-testid="sftp-batch-upload">Upload</button>
          <button type="button" class="ghost" data-testid="sftp-batch-download">Download</button>
        </div>
      </div>
      <div class="sftp-dual">
        <section class="sftp-pane" data-testid="sftp-pane-a" data-slot="a" data-endpoint="${escapeHtml(sftpBrowser.paneA)}">${paneChrome("a", sftpBrowser.paneA)}</section>
        <div class="sftp-arrows" data-testid="sftp-arrows" aria-hidden="true">
          <button type="button" id="sftp-tx-up" class="sftp-arrow-btn" title="Upload selection" aria-label="Upload" data-testid="sftp-tx-up">${icons.upload}</button>
          <button type="button" id="sftp-tx-down" class="sftp-arrow-btn" title="Download selection" aria-label="Download" data-testid="sftp-tx-down">${icons.download}</button>
        </div>
        <section class="sftp-pane" data-testid="sftp-pane-b" data-slot="b" data-endpoint="${escapeHtml(b)}">${paneChrome("b", b)}</section>
      </div>`;
    bindTransferArrowButtons();
    bindBatchBarButtons();
    renderDndTargets();
  }
  bindPaneHostSelects();
  return view;
}

function syncPaneEndpoints() {
  const a = paneSectionEl("a");
  if (a) a.dataset.endpoint = sftpBrowser.paneA;
  const b = paneSectionEl("b");
  if (b && sftpBrowser.paneB) b.dataset.endpoint = sftpBrowser.paneB;
}

function bindPaneHostSelects() {
  ensureSftpView()
    .querySelectorAll<HTMLSelectElement>("select.sftp-pane-host")
    .forEach((sel) => {
      sel.onchange = () => {
        const slot = (sel.dataset.slot || "a") as SftpPaneSlot;
        void changePaneEndpoint(slot, sel.value as SftpEndpointId);
      };
    });
}

async function changePaneEndpoint(slot: SftpPaneSlot, endpoint: SftpEndpointId) {
  sftpBrowser = setPaneEndpoint(sftpBrowser, slot, endpoint);
  const section = paneSectionEl(slot);
  if (section) section.dataset.endpoint = endpoint;
  if (isLocalEndpoint(endpoint)) {
    await initLocalPane();
  } else {
    state.sftpRoot = "/";
    resetSftpCwd();
    await loadSftp(endpoint, ".");
  }
  updateTransferUi();
  renderSftpSidebar(state.sftpHostId);
}

async function activateSplit() {
  sftpBrowser = enterSplit(
    sftpBrowser,
    state.hosts.map((h) => h.id),
  );
  ensureSftpShell(true);
  renderSftpSidebar(state.sftpHostId);
  // Prioritize remote (or local-as-A) first; defer companion local pane (#55).
  if (!isLocalEndpoint(sftpBrowser.paneA)) {
    await loadSftp(sftpBrowser.paneA, state.sftpPath || ".");
  } else {
    await initLocalPane();
  }
  if (sftpBrowser.paneB && isLocalEndpoint(sftpBrowser.paneB)) {
    scheduleLocalPaneInit();
  } else if (sftpBrowser.paneB) {
    await loadSftp(sftpBrowser.paneB, ".");
  }
}

/** Defer local FS listing so remote TTI stays snappy (#55). */
function scheduleLocalPaneInit(): void {
  const run = () => {
    void initLocalPane();
  };
  const ric = (window as unknown as { requestIdleCallback?: (cb: () => void, opts?: { timeout: number }) => void })
    .requestIdleCallback;
  if (typeof ric === "function") ric(run, { timeout: 400 });
  else window.setTimeout(run, 0);
}

function deactivateSplit() {
  sftpBrowser = exitSplit(sftpBrowser);
  ensureSftpShell(true);
  renderSftpSidebar(state.sftpHostId);
  if (isLocalEndpoint(sftpBrowser.paneA)) void initLocalPane();
  else void loadSftp(sftpBrowser.paneA, state.sftpPath || ".");
}

function enterSftpMode() {
  state.sftpMode = true;
  $("workspace").classList.add("sftp-mode");
  ensureSftpView().classList.add("active");
  syncSftpViewClasses();
  $("workspace-empty").classList.add("hidden");
}

function exitSftpMode() {
  if (!state.sftpMode && !$("workspace").classList.contains("sftp-mode")) return;
  sftpFilterDebounce.cancel();
  state.sftpMode = false;
  $("workspace").classList.remove("sftp-mode");
  ensureSftpView().classList.remove("active");
  $("workspace-empty").classList.toggle("hidden", state.panes.length > 0);
}

function sftpConnectionLabel(conn: string): string {
  switch (conn) {
    case "connected":
      return "Connected";
    case "connecting":
      return "Connecting…";
    case "error":
      return "Error";
    case "local":
      return "Local";
    default:
      return "Disconnected";
  }
}

function sftpConnColor(conn: string): string {
  if (conn === "connected" || conn === "local") return "var(--green)";
  if (conn === "connecting") return "var(--yellow)";
  if (conn === "error") return "var(--red)";
  return "var(--tertiary)";
}

function sftpSidebarConn(hostId: string | null): string {
  if (!hostId) return "disconnected";
  if (state.sftpHostId === hostId) return state.sftpConn;
  return "disconnected";
}

function setSftpConn(status: string) {
  state.sftpConn = status;
  if (status !== "connected") return;
  const id = state.sftpHostId;
  if (!id) return;
  const rt = state.hostsRuntime.find((r) => r.host_id === id);
  if (rt) {
    if (rt.connection === "disconnected" || rt.connection === "connecting" || rt.connection === "error") {
      rt.connection = "connected";
    }
  } else {
    state.hostsRuntime.push({ host_id: id, connection: "connected", open_count: 0 });
  }
}

function sftpHostOptions(hostId: string | null): string {
  return sftpEndpointOptions(hostId ?? SFTP_LOCAL_ID);
}

function sftpListFilter(): SftpListFilter {
  return {
    query: $input("host-filter").value,
    showHidden: state.sftpShowHidden,
  };
}

function syncSftpViewClasses() {
  ensureSftpView().classList.toggle("compact-rows", state.sftpCompact !== false);
}

function activeSftpPaneLabel(): string {
  if (sftpBrowser.mode === "single") return "pane A";
  const aLocal = isLocalEndpoint(sftpBrowser.paneA);
  const bLocal = sftpBrowser.paneB != null && isLocalEndpoint(sftpBrowser.paneB);
  if (aLocal && !bLocal) return "remote pane";
  if (!aLocal && bLocal) return "local pane";
  return "both panes";
}

function syncSftpSearchScopeHint() {
  const hint = document.querySelector<HTMLElement>('[data-testid="sftp-filter-hint"]');
  if (hint) hint.textContent = `Search filters ${activeSftpPaneLabel()}.`;
  ($("host-filter") as HTMLInputElement).placeholder = sftpSearchPlaceholder(activeSftpPaneLabel());
}

function sftpTableHeadHtml(): string {
  return `<div class="sftp-table-head" data-testid="sftp-table-head">
    <span class="col-sel"></span><span class="col-name">Name</span><span class="col-size">Size</span><span class="col-mtime">Modified</span><span class="col-actions"></span>
  </div>`;
}

function buildLocalRowHtml(e: LocalEntry): string {
  const glyph = sftpKindIcon(e.name, e.is_dir);
  const sel = state.localSelected.has(e.path);
  return `<div class="local-row sftp-file-row item" draggable="true" data-path="${escapeHtml(e.path)}" data-dir="${e.is_dir}" data-name="${escapeHtml(e.name)}" data-selected="${sel}" data-testid="local-row" title="${e.is_dir ? "Open folder" : "File"}">
          <span class="cell-sel" data-testid="local-select" role="checkbox" aria-checked="${sel}" title="Select">${sel ? icons.check : ""}</span>
          <span class="leading">${glyph.icon}</span>
          <strong class="sftp-name col-name">${escapeHtml(e.name)}</strong>
          <span class="sftp-size col-size">${escapeHtml(formatSftpSize(e.size, e.is_dir))}</span>
          <span class="sftp-mtime col-mtime">${escapeHtml(formatLocalMtime(e.modified))}</span>
          <span class="col-actions"></span>
        </div>`;
}

function buildRemoteRowHtml(e: SftpEntry): string {
  const glyph = sftpKindIcon(e.name, e.is_dir);
  const sel = state.sftpSelected.has(e.path);
  return `<div class="sftp-row sftp-file-row item" draggable="true" data-sftp="${escapeHtml(e.path)}" data-dir="${e.is_dir}" data-kind="${glyph.kind}" data-name="${escapeHtml(e.name)}" data-selected="${sel}" data-testid="sftp-row" title="${e.is_dir ? "Open folder" : "Open with default app"}">
          <span class="cell-sel" data-testid="sftp-select" role="checkbox" aria-checked="${sel}" title="Select">${sel ? icons.check : ""}</span>
          <span class="leading">${glyph.icon}</span>
          <strong class="sftp-name col-name">${escapeHtml(e.name)}</strong>
          <span class="sftp-size col-size">${escapeHtml(formatSftpSize(e.size, e.is_dir))}</span>
          <span class="sftp-mtime col-mtime">${escapeHtml(formatSftpMtime(e.mtime))}</span>
          <span class="trail col-actions">
            <button type="button" class="quick sftp-more" data-testid="sftp-more" title="Actions" aria-label="Actions">${icons.more}</button>
          </span>
        </div>`;
}

function localTableHtml(entries: LocalEntry[]): string {
  const filtered = filterFileEntries(entries, sftpListFilter());
  if (!filtered.length) {
    const msg = entries.length ? "No matching files" : "This folder is empty";
    return `${sftpTableHeadHtml()}<div class="sftp-empty" data-testid="local-empty"><span>${msg}</span></div>`;
  }
  return `${sftpTableHeadHtml()}<div class="sftp-table sftp-virtual-table" data-testid="local-table"><div class="sftp-virtual-viewport" data-testid="local-virtual-viewport"></div></div>`;
}

function remoteTableHtml(entries: SftpEntry[]): string {
  const filtered = filterFileEntries(entries, sftpListFilter());
  if (!filtered.length) {
    const msg = entries.length ? "No matching files" : "This folder is empty";
    return `${sftpTableHeadHtml()}<div class="sftp-empty" data-testid="sftp-empty"><span>${msg}</span></div>`;
  }
  return `${sftpTableHeadHtml()}<div class="sftp-table sftp-virtual-table" data-testid="sftp-table"><div class="sftp-virtual-viewport" data-testid="sftp-virtual-viewport"></div></div>`;
}

function mountLocalVirtualList(entries: LocalEntry[]) {
  localVirtual?.destroy();
  localVirtual = null;
  const filtered = filterFileEntries(entries, sftpListFilter());
  const vp = localPaneEl()?.querySelector<HTMLElement>('[data-testid="local-virtual-viewport"]');
  if (!vp || !filtered.length) return;
  delete vp.dataset.boundLocal;
  localVirtual = mountVirtualList({
    viewport: vp,
    items: filtered,
    renderRow: (e) => buildLocalRowHtml(e),
  });
}

function mountRemoteVirtualList(entries: SftpEntry[]) {
  remoteVirtual?.destroy();
  remoteVirtual = null;
  const filtered = filterFileEntries(entries, sftpListFilter());
  const vp = remotePaneEl()?.querySelector<HTMLElement>('[data-testid="sftp-virtual-viewport"]');
  if (!vp || !filtered.length) return;
  delete vp.dataset.boundSftp;
  remoteVirtual = mountVirtualList({
    viewport: vp,
    items: filtered,
    renderRow: (e) => buildRemoteRowHtml(e),
  });
}

function renderLocalTableBody() {
  const pane = localPaneEl();
  if (!pane || !state.localCwd) return;
  void (async () => {
    const filtered = await filterFileEntriesAsync(state.localEntries, sftpListFilter());
    const vp = pane.querySelector<HTMLElement>('[data-testid="local-virtual-viewport"]');
    if (vp && localVirtual && filtered.length) {
      localVirtual.setItems(filtered);
      return;
    }
    const toolbar = pane.querySelector('[data-testid="local-toolbar"]');
    const after = toolbar ? toolbar.outerHTML : "";
    localVirtual?.destroy();
    localVirtual = null;
    pane.innerHTML = `${after}${localTableHtml(state.localEntries)}`;
    bindLocalToolbar();
    mountLocalVirtualList(state.localEntries);
    bindLocalRows();
  })();
}

function renderRemoteTableBody(hostId: string) {
  const pane = remotePaneEl();
  if (!pane) return;
  void (async () => {
    const filtered = await filterFileEntriesAsync(state.sftpEntries, sftpListFilter());
    const vp = pane.querySelector<HTMLElement>('[data-testid="sftp-virtual-viewport"]');
    if (vp && remoteVirtual && filtered.length) {
      remoteVirtual.setItems(filtered);
      return;
    }
    const toolbar = pane.querySelector('[data-testid="sftp-toolbar"]');
    const after = toolbar ? toolbar.outerHTML : "";
    remoteVirtual?.destroy();
    remoteVirtual = null;
    pane.innerHTML = `${after}${remoteTableHtml(state.sftpEntries)}`;
    bindSftpToolbar(hostId, state.sftpPath);
    mountRemoteVirtualList(state.sftpEntries);
    bindSftpRows(hostId, pane);
  })();
}

function rerenderSftpListings() {
  if (navState.active !== "sftp" || !state.sftpMode) return;
  syncSftpSearchScopeHint();
  if (isLocalEndpoint(sftpBrowser.paneA) || (sftpBrowser.paneB && isLocalEndpoint(sftpBrowser.paneB))) {
    if (state.localEntries.length || state.localCwd) renderLocalTableBody();
  }
  if (state.sftpHostId && state.sftpEntries.length) renderRemoteTableBody(state.sftpHostId);
  else if (state.sftpHostId && remotePaneEl()?.querySelector('[data-testid="sftp-toolbar"]')) {
    renderRemoteTableBody(state.sftpHostId);
  }
  updateTransferUi();
}

function renderSftpSidebar(hostId: string | null) {
  const id = hostId ?? (isLocalEndpoint(sftpBrowser.paneA) ? null : sftpBrowser.paneA) ?? state.hosts[0]?.id ?? null;
  const conn = id ? sftpSidebarConn(id) : isLocalEndpoint(sftpBrowser.paneA) ? "local" : "disconnected";
  const splitBtn =
    sftpBrowser.mode === "split"
      ? `<button type="button" class="ghost sftp-side-split" id="sftp-close-split-btn" data-testid="sftp-close-split-btn">Close split</button>`
      : `<button type="button" class="primary sftp-side-split" id="sftp-split-btn" data-testid="sftp-split-btn">Split</button>`;
  const hiddenLabel = state.sftpShowHidden ? "Hide hidden files" : "Show hidden files";
  const hiddenIcon = state.sftpShowHidden ? icons.eye : icons.eyeOff;
  $("panel-sftp").innerHTML = `<div class="sftp-side" data-testid="sftp-side">
    <div class="sftp-side-status" data-testid="sftp-side-status" data-state="${escapeHtml(conn)}">
      <span class="connection-dot" style="background: ${sftpConnColor(conn)};"></span>
      <span>${escapeHtml(sftpConnectionLabel(conn))}</span>
    </div>
    <button type="button" class="ghost sftp-side-toggle" id="sftp-toggle-hidden" data-testid="sftp-toggle-hidden" aria-pressed="${state.sftpShowHidden}" title="${escapeHtml(hiddenLabel)}">${hiddenIcon}<span>${state.sftpShowHidden ? "Hidden on" : "Hidden off"}</span></button>
    ${splitBtn}
    <p class="sftp-side-hint" data-testid="sftp-filter-hint">Search filters ${escapeHtml(activeSftpPaneLabel())}.</p>
  </div>`;
  const split = document.getElementById("sftp-split-btn");
  if (split) split.onclick = () => void activateSplit();
  const closeSplit = document.getElementById("sftp-close-split-btn");
  if (closeSplit) closeSplit.onclick = () => deactivateSplit();
  const hiddenBtn = document.getElementById("sftp-toggle-hidden");
  if (hiddenBtn) {
    hiddenBtn.onclick = () => {
      state.sftpShowHidden = !state.sftpShowHidden;
      localStorage.setItem("terminus-sftp-show-hidden", state.sftpShowHidden ? "1" : "0");
      renderSftpSidebar(hostId);
      rerenderSftpListings();
    };
  }
  syncSftpSearchScopeHint();
}

function renderSftpWorkspace(hostId: string, path: string, bodyHtml: string) {
  enterSftpMode();
  if (!isLocalEndpoint(sftpBrowser.paneA) && sftpBrowser.paneA !== hostId && sftpBrowser.mode === "single") {
    sftpBrowser = openSingle(sftpBrowser, hostId);
  }
  ensureSftpShell();
  renderSftpSidebar(hostId);
  const remote = remotePaneEl();
  remote.innerHTML = `${sftpToolbarHtml(hostId, path)}${bodyHtml}`;
  bindSftpToolbar(hostId, path);
}

function renderSftpEmpty() {
  state.sftpConn = "disconnected";
  enterSftpMode();
  ensureSftpView().classList.add("active");
  renderSftpSidebar(null);
  const view = ensureSftpView();
  view.innerHTML = `<div class="empty" data-testid="empty-sftp">
    ${icons.folder}
    <span class="empty-title">No host to browse</span>
    <span class="empty-hint">Add a host to browse files over SFTP.</span>
    <div class="empty-actions">
      <button type="button" class="primary" id="sftp-add-host" data-testid="empty-sftp-add-host">Add host</button>
    </div>
  </div>`;
  $("sftp-add-host").onclick = () => editHost();
}

function formatSftpSize(bytes: number, isDir: boolean): string {
  if (isDir) return "—";
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(1)} GB`;
}

function formatSftpMtime(mtime?: number | null): string {
  if (!mtime) return "—";
  try {
    return new Date(mtime * 1000).toLocaleString(undefined, {
      month: "short",
      day: "numeric",
      hour: "2-digit",
      minute: "2-digit",
    });
  } catch {
    return "—";
  }
}

function sftpToolbarHtml(hostId: string, path: string): string {
  const display = sftpDisplayPath(state.sftpCwd, path);
  const atRoot = parentSftpPath(path) == null;
  const pathIcon = atRoot ? icons.home : icons.folder;
  return `<div class="sftp-toolbar" data-testid="sftp-toolbar">
    <button type="button" id="sftp-up" class="sftp-icon-btn" title="Up" aria-label="Up" data-testid="sftp-up" ${atRoot ? "disabled" : ""}>${icons.arrowUp}</button>
    <label class="sftp-path-wrap">
      <span class="sftp-path-ico" aria-hidden="true">${pathIcon}</span>
      <input id="sftp-path" class="sftp-path" type="text" spellcheck="false" value="${escapeHtml(display)}" data-testid="sftp-path" aria-label="Path" />
    </label>
    <button type="button" id="sftp-refresh" class="sftp-icon-btn" title="Refresh" aria-label="Refresh" data-testid="sftp-refresh">${icons.reconnect}</button>
    <button type="button" id="sftp-mkdir" class="sftp-icon-btn" title="New folder" aria-label="New folder" data-testid="sftp-mkdir">${icons.plus}</button>
    <button type="button" id="sftp-upload" class="sftp-icon-btn" title="Upload file" aria-label="Upload file" data-testid="sftp-upload">${icons.upload}</button>
  </div>`;
}

function bindSftpToolbar(hostId: string, path: string) {
  const hostSel = document.getElementById("sftp-host") as HTMLSelectElement | null;
  if (hostSel) {
    hostSel.onchange = () => {
      state.sftpRoot = "/";
      resetSftpCwd();
      void loadSftp(hostSel.value, ".");
    };
  }
  $("sftp-up").onclick = () => {
    const parent = parentSftpPath(path);
    if (parent != null) void loadSftp(hostId, parent);
  };
  $("sftp-refresh").onclick = () => void loadSftp(hostId, path);
  $("sftp-mkdir").onclick = () => sftpMkdirSheet(hostId, path);
  $("sftp-upload").onclick = () => void sftpUpload(hostId, path);
  const pathInput = $input("sftp-path") as HTMLInputElement;
  pathInput.onkeydown = (ev) => {
    if (ev.key === "Enter") {
      ev.preventDefault();
      void navigateSftpPath(hostId, pathInput.value);
    }
  };
}

function renderSftpError(hostId: string, path: string, err: unknown) {
  const typed = parseSftpError(err);
  if (state.sftpConn === "connecting") setSftpConn("error");
  renderSftpWorkspace(
    hostId,
    path,
    `<div class="sftp-error" data-testid="sftp-error" data-kind="${escapeHtml(typed.kind)}" role="alert">
      <strong>${escapeHtml(typed.kind)}</strong>
      <span>${escapeHtml(typed.message)}</span>
    </div>`,
  );
}

async function navigateSftpPath(hostId: string, raw: string) {
  try {
    const next = logicalFromDisplayPath(state.sftpCwd, state.sftpRoot, raw.trim() || ".");
    await loadSftp(hostId, next);
  } catch (err) {
    renderSftpError(hostId, state.sftpPath, err);
  }
}

function bindSftpRows(_hostId: string, root: ParentNode) {
  const viewport =
    (root as HTMLElement).querySelector?.(".sftp-virtual-viewport") ??
    (root as HTMLElement).closest?.(".sftp-pane") ??
    root;
  const el = viewport as HTMLElement;
  if (el.dataset.boundSftp === "1") return;
  el.dataset.boundSftp = "1";
  const host = () => state.sftpHostId || _hostId;
  el.addEventListener("click", (ev) => {
    const t = ev.target as HTMLElement;
    const row = t.closest(".sftp-row") as HTMLElement | null;
    if (!row || !el.contains(row)) return;
    if (t.closest(".sftp-more")) return;
    if (t.closest(".cell-sel")) {
      ev.stopPropagation();
      toggleRemoteSelection(row.dataset.sftp!, row.dataset.selected === "true");
      return;
    }
    const hid = host();
    if (!hid) return;
    if (row.dataset.dir === "true") void loadSftp(hid, row.dataset.sftp!);
    else void sftpOpen(hid, row.dataset.sftp!, row.dataset.name || "", row);
  });
  el.addEventListener("dragstart", (ev) => {
    const row = (ev.target as HTMLElement).closest(".sftp-row") as HTMLElement | null;
    if (!row || !el.contains(row)) return;
    const path = row.dataset.sftp!;
    const items =
      state.sftpSelected.size > 0
        ? state.sftpEntries.filter((x) => state.sftpSelected.has(x.path))
        : [{ path, name: row.dataset.name || "", is_dir: row.dataset.dir === "true" }];
    setDragPayload(ev, "remote", items.map((x) => ({ path: x.path, name: x.name, is_dir: x.is_dir })));
  });
  el.addEventListener("click", (ev) => {
    const more = (ev.target as HTMLElement).closest(".sftp-more") as HTMLButtonElement | null;
    if (!more) return;
    ev.stopPropagation();
    const row = more.closest(".sftp-row") as HTMLElement | null;
    if (!row) return;
    const hid = host();
    if (!hid) return;
    const entryPath = row.dataset.sftp!;
    const isDir = row.dataset.dir === "true";
    const name = row.dataset.name || "";
    showMenu(ev.clientX, ev.clientY, [
      {
        label: "Open",
        run: () => {
          if (isDir) void loadSftp(hid, entryPath);
          else void sftpOpen(hid, entryPath, name, row);
        },
      },
      {
        label: "Download",
        hidden: isDir,
        run: () => void sftpDownload(hid, entryPath, name),
      },
      { label: "Rename", run: () => sftpRenameSheet(hid, entryPath, name, isDir) },
      {
        label: "Delete",
        danger: true,
        run: () => sftpDeleteConfirm(hid, entryPath, name, isDir),
      },
    ]);
  });
}

function toggleRemoteSelection(path: string, currentlySelected: boolean): void {
  if (currentlySelected) state.sftpSelected.delete(path);
  else state.sftpSelected.add(path);
  refreshRemoteSelection();
  updateTransferUi();
}

function refreshRemoteSelection(): void {
  remotePaneEl()
    .querySelectorAll<HTMLElement>(".sftp-row")
    .forEach((el) => {
      const sel = state.sftpSelected.has(el.dataset.sftp!);
      el.dataset.selected = String(sel);
      const cell = el.querySelector<HTMLElement>(".cell-sel");
      if (cell) {
        cell.setAttribute("aria-checked", String(sel));
        cell.innerHTML = sel ? icons.check : "";
      }
    });
}

let sftpLoadSeq = 0;

async function loadSftp(hostId: string, path: string) {
  const seq = ++sftpLoadSeq;
  state.sftpHostId = hostId;
  if (sftpBrowser.mode === "single") {
    sftpBrowser = openSingle(sftpBrowser, hostId);
  } else if (sftpBrowser.paneA !== hostId && sftpBrowser.paneB !== hostId) {
    sftpBrowser = setPaneEndpoint(sftpBrowser, "a", hostId);
  }
  setSftpConn("connecting");
  if (!state.hosts.length && !isLocalEndpoint(hostId)) {
    setSftpConn("disconnected");
    renderSftpEmpty();
    return;
  }
  renderSftpSidebar(hostId);
  await ensureSftpCwd(hostId);
  if (seq !== sftpLoadSeq) return;
  let safePath: string;
  try {
    const raw = path && path.trim() ? path : ".";
    // Full-FS mode: anchor relative/"." onto the session home; absolute passes.
    const anchored = raw.startsWith("/")
      ? raw
      : state.sftpCwd
        ? normalizeSftpPath(`${state.sftpCwd}/${raw}`)
        : raw;
    safePath = resolveUnderRoot(state.sftpRoot, anchored);
  } catch (err) {
    renderSftpError(hostId, state.sftpPath, err);
    return;
  }
  state.sftpPath = safePath;
  const loadingPath = sftpDisplayPath(state.sftpCwd, safePath);
  renderSftpWorkspace(
    hostId,
    safePath,
    `<div class="empty" data-testid="sftp-loading">${icons.folder}<span>Loading ${escapeHtml(loadingPath)}…</span></div>`,
  );
  try {
    const entries = await invoke<SftpEntry[]>("sftp_list", {
      hostId,
      path: safePath,
      root: state.sftpRoot,
    });
    if (seq !== sftpLoadSeq) return;
    setSftpConn("connected");
    if (!entries.length) {
      state.sftpEntries = [];
      renderSftpWorkspace(hostId, safePath, remoteTableHtml([]));
      return;
    }
    state.sftpEntries = entries;
    renderSftpWorkspace(hostId, safePath, remoteTableHtml(entries));
    mountRemoteVirtualList(entries);
    bindSftpRows(hostId, remotePaneEl());
    updateTransferUi();
  } catch (err) {
    if (seq !== sftpLoadSeq) return;
    renderSftpError(hostId, safePath, err);
  }
}

async function sftpOpen(hostId: string, path: string, _name: string, row?: HTMLElement) {
  row?.classList.add("is-pending");
  try {
    const safe = resolveUnderRoot(state.sftpRoot, path);
    await invoke("sftp_open", {
      hostId,
      path: safe,
      root: state.sftpRoot,
    });
  } catch (err) {
    renderSftpError(hostId, state.sftpPath, err);
  } finally {
    row?.classList.remove("is-pending");
  }
}

async function sftpDownload(hostId: string, path: string, name: string) {
  try {
    const safe = resolveUnderRoot(state.sftpRoot, path);
    const bytes = await invoke<number[] | Uint8Array>("sftp_read", {
      hostId,
      path: safe,
      root: state.sftpRoot,
    });
    const data = bytes instanceof Uint8Array ? bytes : Uint8Array.from(bytes);
    await saveLocalFile(name, data);
  } catch (err) {
    renderSftpError(hostId, state.sftpPath, err);
  }
}

async function saveLocalFile(name: string, data: Uint8Array) {
  const w = window as unknown as {
    showSaveFilePicker?: (opts: { suggestedName: string }) => Promise<{
      createWritable: () => Promise<{ write: (d: Uint8Array) => Promise<void>; close: () => Promise<void> }>;
    }>;
  };
  if (typeof w.showSaveFilePicker === "function") {
    try {
      const handle = await w.showSaveFilePicker({ suggestedName: name });
      const writable = await handle.createWritable();
      await writable.write(data);
      await writable.close();
      return;
    } catch (err) {
      // User cancel / headless abort → fall through to <a download>
      const msg = err instanceof Error ? err.message : String(err);
      const name_ = err instanceof Error ? err.name : "";
      if (name_ !== "AbortError" && !/abort/i.test(msg)) throw err;
    }
  }
  const copy = new Uint8Array(data.byteLength);
  copy.set(data);
  const blob = new Blob([copy]);
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = name;
  a.click();
  URL.revokeObjectURL(url);
}

async function sftpUpload(hostId: string, dirPath: string) {
  const input = document.createElement("input");
  input.type = "file";
  input.multiple = true;
  input.dataset.testid = "sftp-file-picker";
  input.onchange = async () => {
    const files = Array.from(input.files || []);
    if (!files.length) return;
    const job: TransferJob = {
      direction: "upload",
      total: files.length,
      done: 0,
      failed: 0,
      label: "Uploading",
      active: true,
      cancel: false,
    };
    setTransfer(job);
    try {
      for (const file of files) {
        if (job.cancel) break;
        try {
          const joined = resolveUnderRoot(
            state.sftpRoot,
            `${dirPath === "/" ? "" : dirPath}/${file.name}`.replace(/\/+/g, "/"),
          );
          const buf = new Uint8Array(await file.arrayBuffer());
          await invoke("sftp_write", {
            hostId,
            path: joined,
            data: Array.from(buf),
            root: state.sftpRoot,
          });
          job.done += 1;
        } catch {
          job.failed += 1;
        }
        renderTransfer();
      }
    } finally {
      job.active = false;
      renderTransfer();
      await loadSftp(hostId, dirPath);
    }
  };
  input.click();
}

function sftpMkdirSheet(hostId: string, dirPath: string) {
  openSheet(`<h2>New folder</h2>
    <label class="field">Name<input id="sftp-mkdir-input" data-testid="sftp-mkdir-input" value="" spellcheck="false" /></label>
    <div class="row">
      <button type="button" id="sftp-mkdir-cancel">Cancel</button>
      <button type="button" class="primary" id="sftp-mkdir-ok" data-testid="sftp-mkdir-ok">Create</button>
    </div>`);
  $("sftp-mkdir-cancel").onclick = () => $("modal").classList.add("hidden");
  $("sftp-mkdir-ok").onclick = async () => {
    const name = ($input("sftp-mkdir-input") as HTMLInputElement).value.trim();
    $("modal").classList.add("hidden");
    if (!name || name.includes("/") || name === "." || name === "..") return;
    try {
      const target = dirPath === "/" || dirPath === "." ? name : `${dirPath.replace(/\/+$/, "")}/${name}`;
      await invoke("sftp_mkdir", {
        hostId,
        path: resolveUnderRoot(state.sftpRoot, target),
        root: state.sftpRoot,
      });
      await loadSftp(hostId, dirPath);
    } catch (err) {
      renderSftpError(hostId, state.sftpPath, err);
    }
  };
}

function sftpRenameSheet(hostId: string, path: string, name: string, _isDir: boolean) {
  openSheet(`
    <h2>Rename</h2>
    <p class="lead">${escapeHtml(name)}</p>
    <label class="field">New name<input id="sftp-rename-input" data-testid="sftp-rename-input" value="${escapeHtml(name)}" spellcheck="false" /></label>
    <div class="row">
      <button type="button" id="sftp-rename-cancel">Cancel</button>
      <button type="button" class="primary" id="sftp-rename-ok" data-testid="sftp-rename-ok">Rename</button>
    </div>`);
  $("sftp-rename-cancel").onclick = () => $("modal").classList.add("hidden");
  $("sftp-rename-ok").onclick = async () => {
    const nextName = ($input("sftp-rename-input") as HTMLInputElement).value.trim();
    if (!nextName || nextName.includes("/") || nextName === "." || nextName === "..") {
      renderSftpError(hostId, state.sftpPath, {
        kind: "SftpPathTraversal",
        message: "invalid name",
        path: nextName,
      });
      $("modal").classList.add("hidden");
      return;
    }
    try {
      const parent = parentSftpPath(path) ?? state.sftpRoot;
      const to =
        parent === "/"
          ? `/${nextName}`
          : parent === "."
            ? nextName
            : `${parent}/${nextName}`;
      const fromSafe = resolveUnderRoot(state.sftpRoot, path);
      const toSafe = resolveUnderRoot(state.sftpRoot, to);
      await invoke("sftp_rename", {
        hostId,
        from: fromSafe,
        to: toSafe,
        root: state.sftpRoot,
      });
      $("modal").classList.add("hidden");
      await loadSftp(hostId, state.sftpPath);
    } catch (err) {
      $("modal").classList.add("hidden");
      renderSftpError(hostId, state.sftpPath, err);
    }
  };
}

function sftpDeleteConfirm(hostId: string, path: string, name: string, isDir: boolean) {
  openSheet(`
    <h2>Delete ${isDir ? "folder" : "file"}?</h2>
    <p class="lead">This cannot be undone.</p>
    <p class="form-error">${escapeHtml(name)}</p>
    <div class="row">
      <button type="button" id="sftp-del-cancel" data-testid="sftp-del-cancel">Cancel</button>
      <button type="button" class="danger" id="sftp-del-ok" data-testid="sftp-del-confirm">Delete</button>
    </div>`);
  $("sftp-del-cancel").onclick = () => $("modal").classList.add("hidden");
  $("sftp-del-ok").onclick = async () => {
    try {
      const safe = resolveUnderRoot(state.sftpRoot, path);
      state.sftpSelected.delete(path);
      if (isDir) {
        await invoke("sftp_rmtree", { hostId, path: safe, root: state.sftpRoot });
      } else {
        await invoke("sftp_remove", { hostId, path: safe, isDir, root: state.sftpRoot });
      }
      $("modal").classList.add("hidden");
      await loadSftp(hostId, state.sftpPath);
    } catch (err) {
      $("modal").classList.add("hidden");
      renderSftpError(hostId, state.sftpPath, err);
    }
  };
}

// ─── Local pane ─────────────────────────────────────────────────────────────
const IS_WIN = /^Win/.test(navigator.platform || "");

type DragItem = { path: string; name: string; is_dir: boolean };
type FileRef = { src: string; rel: string; name: string };
const DRAG_MIME = "application/x-terminus-files";

async function initLocalPane(): Promise<void> {
  if (!state.localCwd) {
    try {
      state.localCwd = await invoke<string>("local_home");
      state.localRoot = state.localCwd;
    } catch {
      state.localCwd = "";
    }
  }
  await loadLocal();
}

async function loadLocal(): Promise<void> {
  const pane = localPaneEl();
  if (!pane) return;
  if (!state.localCwd) {
    pane.innerHTML = `<div class="sftp-pane-empty" data-testid="local-pane-empty">
      <span>No folder chosen</span>
      <button type="button" class="primary" id="local-pick-empty" data-testid="local-pick-folder">${icons.folder} Choose folder</button>
    </div>`;
    $("local-pick-empty").onclick = () => void pickLocalFolder();
    updateTransferUi();
    return;
  }
  pane.innerHTML = `${localToolbarHtml(state.localCwd)}<div class="local-loading" data-testid="local-loading">Loading ${escapeHtml(state.localCwd)}…</div>`;
  bindLocalToolbar();
  try {
    const entries = await invoke<LocalEntry[]>("local_list", { path: state.localCwd });
    state.localEntries = entries;
    pane.innerHTML = `${localToolbarHtml(state.localCwd)}${localTableHtml(entries)}`;
    bindLocalToolbar();
    mountLocalVirtualList(entries);
    bindLocalRows();
  } catch (err) {
    pane.innerHTML = `${localToolbarHtml(state.localCwd)}<div class="sftp-error" data-testid="sftp-error"><strong>Local</strong><span>${escapeHtml(String(err))}</span></div>`;
    bindLocalToolbar();
  }
  updateTransferUi();
}

function formatLocalMtime(modified?: number | null): string {
  if (!modified) return "—";
  try {
    return new Date(modified).toLocaleString(undefined, {
      month: "short",
      day: "numeric",
      hour: "2-digit",
      minute: "2-digit",
    });
  } catch {
    return "—";
  }
}

function localToolbarHtml(cwd: string): string {
  const atRoot = parentLocalPath(cwd) == null;
  return `<div class="sftp-toolbar" data-testid="local-toolbar">
    <button type="button" id="local-pick" class="sftp-icon-btn" title="Choose folder" aria-label="Choose folder" data-testid="local-pick">${icons.folder}</button>
    <button type="button" id="local-up" class="sftp-icon-btn" title="Up" aria-label="Up" data-testid="local-up" ${atRoot ? "disabled" : ""}>${icons.arrowUp}</button>
    <label class="sftp-path-wrap">
      <span class="sftp-path-ico" aria-hidden="true">${icons.home}</span>
      <input id="local-path" class="sftp-path" type="text" spellcheck="false" value="${escapeHtml(cwd)}" data-testid="local-path" aria-label="Local path" />
    </label>
    <button type="button" id="local-refresh" class="sftp-icon-btn" title="Refresh" aria-label="Refresh" data-testid="local-refresh">${icons.reconnect}</button>
    <button type="button" id="local-mkdir" class="sftp-icon-btn" title="New folder" aria-label="New folder" data-testid="local-mkdir">${icons.plus}</button>
  </div>`;
}

function bindLocalToolbar(): void {
  $("local-pick").onclick = () => void pickLocalFolder();
  const up = $("local-up");
  if (up)
    up.onclick = () => {
      const p = parentLocalPath(state.localCwd);
      if (p != null) {
        state.localCwd = p;
        void loadLocal();
      }
    };
  $("local-refresh").onclick = () => void loadLocal();
  $("local-mkdir").onclick = () => localMkdirSheet();
  const pathInput = $input("local-path") as HTMLInputElement;
  pathInput.onkeydown = (ev) => {
    if (ev.key === "Enter") {
      ev.preventDefault();
      const v = pathInput.value.trim();
      if (v) {
        state.localCwd = v;
        void loadLocal();
      }
    }
  };
}

function bindLocalRows(): void {
  const pane = localPaneEl();
  const root = (pane.querySelector(".sftp-virtual-viewport") as HTMLElement | null) ?? pane;
  if (root.dataset.boundLocal === "1") return;
  root.dataset.boundLocal = "1";
  root.addEventListener("click", (ev) => {
    const t = ev.target as HTMLElement;
    const row = t.closest(".local-row") as HTMLElement | null;
    if (!row || !root.contains(row)) return;
    if (t.closest(".cell-sel")) {
      ev.stopPropagation();
      toggleLocalSelection(row.dataset.path!, row.dataset.selected === "true");
      return;
    }
    if (row.dataset.dir === "true") {
      state.localCwd = row.dataset.path!;
      void loadLocal();
    }
  });
  root.addEventListener("dragstart", (ev) => {
    const row = (ev.target as HTMLElement).closest(".local-row") as HTMLElement | null;
    if (!row || !root.contains(row)) return;
    const path = row.dataset.path!;
    const items =
      state.localSelected.size > 0
        ? state.localEntries.filter((x) => state.localSelected.has(x.path))
        : [{ path, name: row.dataset.name || "", is_dir: row.dataset.dir === "true" }];
    setDragPayload(ev, "local", items.map((x) => ({ path: x.path, name: x.name, is_dir: x.is_dir })));
  });
}

function toggleLocalSelection(path: string, currentlySelected: boolean): void {
  if (currentlySelected) state.localSelected.delete(path);
  else state.localSelected.add(path);
  refreshLocalSelection();
  updateTransferUi();
}

function refreshLocalSelection(): void {
  localPaneEl()
    .querySelectorAll<HTMLElement>(".local-row")
    .forEach((el) => {
      const sel = state.localSelected.has(el.dataset.path!);
      el.dataset.selected = String(sel);
      const cell = el.querySelector<HTMLElement>(".cell-sel");
      if (cell) {
        cell.setAttribute("aria-checked", String(sel));
        cell.innerHTML = sel ? icons.check : "";
      }
    });
}

async function pickLocalFolder(): Promise<void> {
  try {
    const dir = await open({ directory: true, multiple: false });
    if (typeof dir === "string" && dir) {
      state.localCwd = dir;
      state.localRoot = dir;
      await loadLocal();
    }
  } catch {
    /* user cancelled */
  }
}

function localMkdirSheet(): void {
  openSheet(`<h2>New folder</h2>
    <label class="field">Name<input id="local-mkdir-input" data-testid="local-mkdir-input" value="" spellcheck="false" /></label>
    <div class="row">
      <button type="button" id="local-mkdir-cancel">Cancel</button>
      <button type="button" class="primary" id="local-mkdir-ok" data-testid="local-mkdir-ok">Create</button>
    </div>`);
  $("local-mkdir-cancel").onclick = () => $("modal").classList.add("hidden");
  $("local-mkdir-ok").onclick = async () => {
    const name = ($input("local-mkdir-input") as HTMLInputElement).value.trim();
    $("modal").classList.add("hidden");
    if (!name || name.includes("/") || name.includes("\\")) return;
    try {
      await invoke("local_mkdir", { path: joinLocalPath(state.localCwd, name) });
      await loadLocal();
    } catch (err) {
      console.error("local_mkdir failed", err);
    }
  };
}

// ─── Transfer engine ────────────────────────────────────────────────────────
function setTransfer(job: TransferJob | null): void {
  state.transfer = job;
  renderTransfer();
  updateTransferUi();
}

function renderTransfer(): void {
  const bar = ensureSftpView().querySelector<HTMLElement>('[data-testid="sftp-transfer"]');
  if (!bar) return;
  const job = state.transfer;
  if (!job || job.total === 0) {
    bar.innerHTML = "";
    return;
  }
  const pct = Math.min(100, Math.round((job.done / job.total) * 100));
  const failed = job.failed ? ` · ${job.failed} failed` : "";
  bar.innerHTML = `<div class="sftp-transfer-inner" data-testid="sftp-transfer-bar">
    <span class="sftp-transfer-label">${escapeHtml(job.label)} ${job.done}/${job.total}${failed}</span>
    <span class="sftp-transfer-track"><span class="sftp-transfer-fill" style="width:${pct}%"></span></span>
    ${job.active ? `<button type="button" id="sftp-tx-cancel" data-testid="sftp-tx-cancel" title="Cancel">${icons.close}</button>` : ""}
  </div>`;
  const cancel = $("sftp-tx-cancel");
  if (cancel) cancel.onclick = () => {
    if (state.transfer) state.transfer.cancel = true;
  };
}

function updateTransferUi(): void {
  const up = $("sftp-tx-up") as HTMLButtonElement | null;
  const down = $("sftp-tx-down") as HTMLButtonElement | null;
  const canTx = canTransferBetween(sftpBrowser.paneA, sftpBrowser.paneB);
  const localN = state.localSelected.size;
  const remoteN = state.sftpSelected.size;
  if (up) up.disabled = !canTx || localN === 0;
  if (down) down.disabled = !canTx || remoteN === 0;

  const batch = document.querySelector<HTMLElement>('[data-testid="sftp-batch-bar"]');
  const showBatch = shouldShowBatchBar({
    split: sftpBrowser.mode === "split",
    canTransfer: canTx,
    localSelected: localN,
    remoteSelected: remoteN,
  });
  if (batch) {
    batch.classList.toggle("hidden", !showBatch);
    const count = batch.querySelector<HTMLElement>('[data-testid="sftp-batch-count"]');
    if (count) count.textContent = `${localN + remoteN} selected`;
    const upload = batch.querySelector<HTMLButtonElement>('[data-testid="sftp-batch-upload"]');
    const download = batch.querySelector<HTMLButtonElement>('[data-testid="sftp-batch-download"]');
    if (upload) upload.disabled = !batchUploadEnabled(localN);
    if (download) download.disabled = !batchDownloadEnabled(remoteN);
  }
}

function bindBatchBarButtons(): void {
  const upload = document.querySelector<HTMLButtonElement>('[data-testid="sftp-batch-upload"]');
  const download = document.querySelector<HTMLButtonElement>('[data-testid="sftp-batch-download"]');
  const clear = document.querySelector<HTMLButtonElement>('[data-testid="sftp-batch-clear"]');
  if (upload) upload.onclick = () => void transferSelected("upload");
  if (download) download.onclick = () => void transferSelected("download");
  if (clear) {
    clear.onclick = () => {
      state.localSelected.clear();
      state.sftpSelected.clear();
      refreshLocalSelection();
      refreshRemoteSelection();
      updateTransferUi();
    };
  }
  updateTransferUi();
}

function bindTransferArrowButtons(): void {
  const up = $("sftp-tx-up");
  const down = $("sftp-tx-down");
  if (up) up.onclick = () => void transferSelected("upload");
  if (down) down.onclick = () => void transferSelected("download");
  updateTransferUi();
}

async function transferSelected(direction: "upload" | "download"): Promise<void> {
  const hostId = state.sftpHostId;
  if (!hostId) return;
  if (direction === "upload") {
    const items = state.localEntries.filter((e) => state.localSelected.has(e.path));
    if (!items.length) return;
    const files: FileRef[] = [];
    await collectLocalTree(items, files);
    await transferFiles({
      direction: "upload",
      hostId,
      remoteTargetDir: state.sftpPath,
      localTargetDir: state.localCwd,
      files,
    });
  } else {
    const items = state.sftpEntries.filter((e) => state.sftpSelected.has(e.path));
    if (!items.length) return;
    const files: FileRef[] = [];
    await collectRemoteTree(hostId, items, files);
    await transferFiles({
      direction: "download",
      hostId,
      remoteTargetDir: state.sftpPath,
      localTargetDir: state.localCwd,
      files,
    });
  }
}

async function collectLocalTree(
  items: { path: string; name: string; is_dir: boolean }[],
  acc: FileRef[],
): Promise<void> {
  for (const it of items) {
    if (it.is_dir) await collectLocalDir(it.path, it.name, acc);
    else acc.push({ src: it.path, rel: it.name, name: it.name });
  }
}

async function collectLocalDir(dirPath: string, relDir: string, acc: FileRef[]): Promise<void> {
  const entries = await invoke<LocalEntry[]>("local_list", { path: dirPath });
  for (const e of entries) {
    const rel = relDir ? `${relDir}/${e.name}` : e.name;
    if (e.is_dir) await collectLocalDir(e.path, rel, acc);
    else acc.push({ src: e.path, rel, name: e.name });
  }
}

async function collectRemoteTree(
  hostId: string,
  items: { path: string; name: string; is_dir: boolean }[],
  acc: FileRef[],
): Promise<void> {
  for (const it of items) {
    if (it.is_dir) await collectRemoteDir(hostId, it.path, it.name, acc);
    else acc.push({ src: it.path, rel: it.name, name: it.name });
  }
}

async function collectRemoteDir(
  hostId: string,
  remotePath: string,
  relDir: string,
  acc: FileRef[],
): Promise<void> {
  const entries = await invoke<SftpEntry[]>("sftp_list", {
    hostId,
    path: remotePath,
    root: state.sftpRoot,
  });
  for (const e of entries) {
    const rel = relDir ? `${relDir}/${e.name}` : e.name;
    if (e.is_dir) await collectRemoteDir(hostId, e.path, rel, acc);
    else acc.push({ src: e.path, rel, name: e.name });
  }
}

async function transferFiles(p: {
  direction: "upload" | "download";
  hostId: string;
  remoteTargetDir: string;
  localTargetDir: string;
  files: FileRef[];
}): Promise<void> {
  if (p.files.length === 0) return;
  const job: TransferJob = {
    direction: p.direction,
    total: p.files.length,
    done: 0,
    failed: 0,
    label: p.direction === "upload" ? "Uploading" : "Downloading",
    active: true,
    cancel: false,
  };
  setTransfer(job);
  const created = new Set<string>();
  for (const f of p.files) {
    if (job.cancel) break;
    const relDir = f.rel.includes("/") ? f.rel.slice(0, f.rel.lastIndexOf("/")) : "";
    try {
      if (relDir && !created.has(relDir)) {
        created.add(relDir);
        if (p.direction === "upload") {
          await invoke("sftp_mkdir", {
            hostId: p.hostId,
            path: joinRemote(p.remoteTargetDir, relDir),
            root: state.sftpRoot,
          }).catch(() => undefined);
        } else {
          await invoke("local_mkdir", { path: joinLocalPath(p.localTargetDir, relToSep(relDir)) });
        }
      }
      if (p.direction === "upload") {
        const data = await invoke<number[] | Uint8Array>("local_read", { path: f.src });
        await invoke("sftp_write", {
          hostId: p.hostId,
          path: joinRemote(p.remoteTargetDir, f.rel),
          data: Array.from(data instanceof Uint8Array ? data : Uint8Array.from(data)),
          root: state.sftpRoot,
        });
      } else {
        const data = await invoke<number[] | Uint8Array>("sftp_read", {
          hostId: p.hostId,
          path: f.src,
          root: state.sftpRoot,
        });
        await invoke("local_write", {
          path: joinLocalPath(p.localTargetDir, relToSep(f.rel)),
          data: Array.from(data instanceof Uint8Array ? data : Uint8Array.from(data)),
        });
      }
      job.done += 1;
    } catch {
      job.failed += 1;
    }
    renderTransfer();
  }
  job.active = false;
  renderTransfer();
  state.localSelected.clear();
  state.sftpSelected.clear();
  if (p.direction === "upload") await loadSftp(p.hostId, state.sftpPath);
  else await loadLocal();
  refreshLocalSelection();
  refreshRemoteSelection();
  updateTransferUi();
}

function joinRemote(dir: string, rel: string): string {
  const d = dir === "/" ? "/" : dir.replace(/\/*$/, "");
  if (!d || d === ".") return rel;
  return d === "/" ? `/${rel}` : `${d}/${rel}`;
}

function relToSep(rel: string): string {
  return IS_WIN ? rel.split("/").join("\\") : rel;
}

// ─── Drag & drop between panes ──────────────────────────────────────────────
function setDragPayload(ev: DragEvent, side: "local" | "remote", items: DragItem[]): void {
  if (!ev.dataTransfer) return;
  ev.dataTransfer.setData(DRAG_MIME, JSON.stringify({ side, items }));
  ev.dataTransfer.effectAllowed = "copy";
}

function readDragPayload(ev: DragEvent): { side: "local" | "remote"; items: DragItem[] } | null {
  const raw = ev.dataTransfer?.getData(DRAG_MIME);
  if (!raw) return null;
  try {
    const parsed = JSON.parse(raw) as { side: "local" | "remote"; items: DragItem[] };
    if (parsed && Array.isArray(parsed.items)) return parsed;
  } catch {
    /* ignore */
  }
  return null;
}

function renderDndTargets(): void {
  const view = ensureSftpView();
  const local = view.querySelector<HTMLElement>(`[data-endpoint="${SFTP_LOCAL_ID}"]`);
  const remote = state.sftpHostId
    ? view.querySelector<HTMLElement>(`[data-endpoint="${state.sftpHostId}"]`)
    : view.querySelector<HTMLElement>('[data-testid="sftp-pane-a"]');
  if (local) wireDrop(local, "local");
  if (remote) wireDrop(remote, "remote");
}

function wireDrop(pane: HTMLElement, side: "local" | "remote"): void {
  pane.addEventListener("dragover", (ev) => {
    ev.preventDefault();
    if (ev.dataTransfer) ev.dataTransfer.dropEffect = "copy";
    pane.classList.add("drop-target");
  });
  pane.addEventListener("dragleave", () => pane.classList.remove("drop-target"));
  pane.addEventListener("drop", (ev) => {
    ev.preventDefault();
    pane.classList.remove("drop-target");
    const payload = readDragPayload(ev);
    if (payload) void handleDrop(payload, side);
  });
}

async function handleDrop(
  payload: { side: "local" | "remote"; items: DragItem[] },
  targetSide: "local" | "remote",
): Promise<void> {
  const hostId = state.sftpHostId;
  if (!hostId || !payload.items.length) return;
  if (targetSide === "remote" && payload.side === "local") {
    const files: FileRef[] = [];
    await collectLocalTree(payload.items, files);
    await transferFiles({
      direction: "upload",
      hostId,
      remoteTargetDir: state.sftpPath,
      localTargetDir: state.localCwd,
      files,
    });
  } else if (targetSide === "local" && payload.side === "remote") {
    const files: FileRef[] = [];
    await collectRemoteTree(hostId, payload.items, files);
    await transferFiles({
      direction: "download",
      hostId,
      remoteTargetDir: state.sftpPath,
      localTargetDir: state.localCwd,
      files,
    });
  }
}

document.querySelector('[data-activity="sftp"]')?.addEventListener("click", () => {
  void reopenSftpBrowser();
});

/** Re-enter Files: soft-restore shell when layout unchanged; reload only empty panes (#49). */
async function reopenSftpBrowser(): Promise<void> {
  const activeHost = activePane()?.session?.host_id ?? activePane()?.pending?.hostId;
  const hostId = state.sftpHostId ?? activeHost ?? state.hosts[0]?.id ?? null;

  if (!hostId) {
    sftpBrowser = openSingle(sftpBrowser, SFTP_LOCAL_ID);
    ensureSftpShell(true);
    enterSftpMode();
    renderSftpSidebar(null);
    await initLocalPane();
    return;
  }

  if (sftpBrowser.mode === "single") {
    sftpBrowser = openSingle(sftpBrowser, hostId);
  } else if (
    sftpBrowser.paneA !== hostId &&
    sftpBrowser.paneB !== hostId &&
    !isLocalEndpoint(sftpBrowser.paneA)
  ) {
    sftpBrowser = setPaneEndpoint(sftpBrowser, "a", hostId);
  }

  const view = ensureSftpView();
  const soft = sftpShellMatchesBrowser(view);
  ensureSftpShell(!soft);
  enterSftpMode();
  renderSftpSidebar(hostId);

  const remoteReady = Boolean(
    remotePaneEl()?.querySelector(
      '[data-testid="sftp-table"], [data-testid="sftp-row"], [data-testid="sftp-empty"], [data-testid="sftp-error"]',
    ),
  );
  const localReady = Boolean(
    localPaneEl()?.querySelector(
      '[data-testid="local-table"], [data-testid="local-row"], [data-testid="local-empty"]',
    ),
  );
  const needsLocal =
    isLocalEndpoint(sftpBrowser.paneA) ||
    (sftpBrowser.paneB != null && isLocalEndpoint(sftpBrowser.paneB));

  const loads: Promise<void>[] = [];
  if (!remoteReady || !soft) loads.push(loadSftp(hostId, state.sftpPath || "."));
  if (needsLocal && (!localReady || !soft)) {
    // Soft path with empty local: schedule so remote paint isn't blocked (#55).
    if (soft && remoteReady) scheduleLocalPaneInit();
    else loads.push(initLocalPane());
  }
  if (loads.length) await Promise.all(loads);
}

boot().catch((err) => {
  console.error(err);
  openSheet(`<h2>Couldn't start</h2><p class="form-error">${escapeHtml(String(err))}</p>`);
});

// E2E-only bridge: opt-in via VITE_E2E=1 at build time (not every build / not DEV).
if (import.meta.env.VITE_E2E === "1") {
  initTestBridge();
  (window as any).__terminusUpdateTest = {
    setPendingAppUpdate: setPendingAppUpdateForTest,
  };
  window.addEventListener("terminus-e2e-refresh", () => {
    void refreshSide();
    void refreshSync();
  });
  window.addEventListener("terminus-e2e-open-vault", () => {
    void openVault();
  });
}
