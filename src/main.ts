import "./assets/font-logos/font-logos.css";
import "./styles.css";
import { icons, sftpKindIcon, hostOsIcon } from "./icons";
import { applyChrome, type Theme } from "./theme";
import { resolveMonoFont } from "./fonts";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { open } from "@tauri-apps/plugin-dialog";
import { open as openExternal } from "@tauri-apps/plugin-shell";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import {
  readText as tauriClipboardReadText,
  writeText as tauriClipboardWriteText,
} from "@tauri-apps/plugin-clipboard-manager";
import {
  claimPasteDelivery,
  decideDomPasteAction,
  formatTerminalPaste,
  nextPasteSuppressUntil,
} from "./termPaste";
import {
  encodeTermMouse,
  mouseReportingEnabled,
  xtermButtonCode,
} from "./termMouse";
import { findUrlAt, isOpenableHttpUrl, shouldAttemptLinkOpen } from "./termLinks";
import { computeAffectedGroups, findOrphanedHosts, applySoftDelete, detachHost } from "./groupSoftDelete";
import { initTestBridge } from "./testBridge";
import { installE2eMock } from "./e2eMock";
import { joinLocalPath, parentLocalPath, fileNameOfPath } from "./localPath";
import {
  MOTION_FAST_MS,
  beginOverlayClose,
  finishOverlayClose,
  motionDurationMs,
  prepareOverlayOpen,
  prefersReducedMotion,
} from "./motion";
import {
  destroyHostDragGhost,
  scheduleGhostFrame,
  spawnHostDragGhost,
  type GhostState,
} from "./hostDragGhost";
import { applyWindowChrome, resolveWindowChrome } from "./windowChrome";
import {
  resolveUnderRoot,
  normalizeSftpPath,
  parentSftpPath,
  parseSftpError,
  sftpDisplayPath,
  logicalFromDisplayPath,
} from "./sftpPath";
import {
  sanitizeTabTitle,
  resolveTabTitle,
  buildTabContextMenu,
  handleRenameInputKeydown,
} from "./tabRename";
import {
  SFTP_LOCAL_ID,
  canTransferBetween,
  createSftpBrowserState,
  enterSplit,
  exitSplit,
  focusRemoteInLayout,
  isLocalEndpoint,
  openSingle,
  placeAdditionalRemote,
  setPaneEndpoint,
  transferLane,
  type SftpBrowserState,
  type SftpEndpointId,
  type SftpPaneSlot,
} from "./sftpLayout";
import {
  closeRemote,
  createSftpSessions,
  defaultSessionSnap,
  hasRemote,
  openOrFocusRemote,
  openRemoteIds,
  rememberRemote,
  type SftpSessions,
} from "./sftpSessions";
import {
  beginPaneLoad,
  createSftpPaneLive,
  failPaneLoad,
  finishPaneLoad,
  isPaneLoadCurrent,
  type SftpPaneLive,
} from "./sftpPaneState";
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
import {
  decodeGpuFrame,
  growAtlasR8,
  isGpuFrame,
  tryCreateTermGl,
  type DecodedGpuFrame,
  type TermGlPainter,
} from "./termGl";
import { createTrailingDebounce, FRAME_MIN_MS, SFTP_FILTER_DEBOUNCE_MS } from "./perfTiming";
import { parseKnownHosts } from "./knownHostsParse";
import {
  inferIdentityKind,
  parseIdentityKind,
  parseIdentityKeyError,
  type IdentityKind,
} from "./identityKind";
import { hostAuthUi } from "./authMethod";
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
type WslDistro = { name: string; state: string; version: number; is_default: boolean };
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
  vault_configured?: boolean;
  vault_unlocked?: boolean;
};
type VaultStatus = {
  configured: boolean;
  unlocked: boolean;
  key_id?: string | null;
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
  /** "upload" = local→remote, "download" = remote→local, "remote-copy" = remote→remote (#105). */
  direction: "upload" | "download" | "remote-copy";
  total: number;
  done: number;
  failed: number;
  label: string;
  active: boolean;
  cancel: boolean;
};

type Pane = {
  id: string;
  customTitle?: string;
  session?: SessionInfo;
  pending?: { title: string; kind: string; hostId?: string };
  exited?: boolean;
  /** Set when the tab is closed while a connect is still in flight. */
  aborted?: boolean;
  canvas: HTMLCanvasElement;
  ctx: CanvasRenderingContext2D | null;
  gl: TermGlPainter | null;
  atlasR8: Uint8Array | null;
  atlasW: number;
  atlasH: number;
  glyphMap: Map<number, { x: number; y: number; w: number; h: number; ox: number; oy: number }>;
  el: HTMLDivElement;
  /** Wraps canvas + selection so overlay coords share the canvas origin. */
  viewport: HTMLDivElement;
  banner: HTMLDivElement;
  cellW: number;
  cellH: number;
  /** Terminal mode bits from frame header: APP_CURSOR|APP_KEYPAD|ALT_SCREEN|BRACKETED_PASTE|MOUSE|SGR|DRAG */
  modeFlags: number;
  /** Alacritty display_offset (0 = live bottom). */
  scrollOffset: number;
  /** Lines of scrollback history above the viewport (0 = no scrollbar). */
  scrollMax: number;
  scrollbar: HTMLDivElement;
  scrollThumb: HTMLDivElement;
  _scrollDrag?: { startY: number; startOffset: number } | null;
  rasterScale: number;
  cols: number;
  rows: number;
  paintGen: number;
  selAnchor: { row: number; col: number } | null;
  selFocus: { row: number; col: number } | null;
  selLayer: HTMLDivElement;
  _selecting?: boolean;
  _selStart?: { row: number; col: number } | null;
  /** Ignore DOM paste until this timestamp (keydown Ctrl+V already inserted). */
  _pasteSuppressUntil?: number;
  /** Button currently reported to the PTY while mouse mode is active. */
  _mouseBtn?: number | null;
};

const state = {
  hosts: [] as Host[],
  hostsRuntime: [] as HostRuntime[],
  wslDistros: [] as WslDistro[],
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
  /** Last focused SFTP side (for Paste / keyboard shortcuts). */
  sftpFocusSide: "remote" as "local" | "remote",
  /** In-app file clipboard for Copy/Cut/Paste across panes. */
  fileClipboard: null as null | {
    side: "local" | "remote";
    hostId: string | null;
    items: { path: string; name: string; is_dir: boolean }[];
    mode: "copy" | "cut";
  },
  transfer: null as TransferJob | null,
  sftpConn: "disconnected" as string,
  sftpShowHidden: localStorage.getItem("terminus-sftp-show-hidden") === "1",
  sftpCompact: localStorage.getItem("terminus-sftp-compact") !== "0",
  customCss: document.createElement("style"),
  expandedGroups: new Set<string>(JSON.parse(localStorage.getItem("terminus-expanded-groups") || "[]")),
  localOsId: null as string | null,
};

/** True while a host card is dragged for group assign/ungroup (#88). */
let hostGroupDragActive = false;

/** Activity Bar + contextual sidebar navigation (#27). */
let navState: NavState = createNavState();
let forwardUi: ForwardUiState = createForwardUiState();
/** SFTP single/split browser (#41). */
let sftpBrowser: SftpBrowserState = createSftpBrowserState();
/** Parked remote SFTP sessions (#101). */
let sftpSessions: SftpSessions = createSftpSessions();
const sftpEntryCache = new Map<string, SftpEntry[]>();
/** Per-host live load bookkeeping (#109) — opening B never cancels A. */
const sftpPaneLive = new Map<string, SftpPaneLive>();
const sftpLoadInflight = new Map<string, Promise<void>>();
const sftpFilterDebounce = createTrailingDebounce(SFTP_FILTER_DEBOUNCE_MS, () => {
  rerenderSftpListings();
});
let localVirtual: VirtualListHandle<LocalEntry> | null = null;
let remoteVirtual: VirtualListHandle<SftpEntry> | null = null;

document.head.appendChild(state.customCss);

/** Windows host UI (WSL rows). Evaluated once at load. */
const IS_WIN = /^Win/.test(navigator.platform || "");

/** Titlebar caption style: mac traffic lights vs win/linux symbols (#99). */
applyWindowChrome(
  resolveWindowChrome({
    platform: navigator.platform || "",
    userAgent: navigator.userAgent || "",
  }),
);
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
  void Promise.all([refreshSide({ forceWsl: true }), refreshSync()]).then(() => {
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
    hideModalOverlay();
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
    $("update-dismiss").onclick = () => hideModalOverlay();
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
  if (ArrayBuffer.isView(data)) {
    const view = data as ArrayBufferView;
    return new Uint8Array(view.buffer, view.byteOffset, view.byteLength);
  }
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

/** Size canvas to pane content box × dpr and fill theme bg — never leave HTML 300×150 black. */
function clearPaneSurface(pane: Pane) {
  const workspace = $("workspace");
  const padX = TERMINAL_PANE_PADDING_X_PX;
  const padY = TERMINAL_PANE_PADDING_Y_PX;
  const cssW = Math.max(
    1,
    (pane.el.clientWidth || workspace.clientWidth || 1) - padX * 2,
  );
  const cssH = Math.max(
    1,
    (pane.el.clientHeight || workspace.clientHeight || 1) - padY * 2,
  );
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
  const bg = termBgColor();
  if (pane.ctx) {
    pane.ctx.setTransform(1, 0, 0, 1, 0, 0);
    pane.ctx.imageSmoothingEnabled = false;
    pane.ctx.fillStyle = bg;
    pane.ctx.fillRect(0, 0, width, height);
    return;
  }
  if (pane.gl) {
    const gl = pane.canvas.getContext("webgl2");
    if (gl) {
      const m = /^#?([0-9a-f]{6})$/i.exec(bg.trim());
      let r = 0.11;
      let g = 0.11;
      let b = 0.12;
      if (m) {
        const n = parseInt(m[1], 16);
        r = ((n >> 16) & 255) / 255;
        g = ((n >> 8) & 255) / 255;
        b = (n & 255) / 255;
      }
      gl.viewport(0, 0, width, height);
      gl.clearColor(r, g, b, 1);
      gl.clear(gl.COLOR_BUFFER_BIT);
    }
  }
}

function paintGpuSoftware(pane: Pane, frame: DecodedGpuFrame, dpr: number) {
  const ctx = pane.ctx;
  if (!ctx) return;
  const spl = Math.max(1, frame.spritesPerLayer);
  const stampH = frame.cellH + 1;
  const layerW = spl * frame.cellW;
  const layers = Math.max(1, frame.layerCount);
  if (!pane.atlasR8 || pane.atlasW !== layerW || pane.atlasH !== stampH * layers) {
    pane.atlasR8 = growAtlasR8(
      pane.atlasR8,
      pane.atlasW,
      pane.atlasH,
      layerW,
      stampH * layers,
    );
    pane.atlasW = layerW;
    pane.atlasH = stampH * layers;
  }
  const atlas = pane.atlasR8;
  for (const s of frame.sprites) {
    if (!s.bits || s.bits.byteLength < frame.cellW * stampH) continue;
    if (s.spriteLayer >= layers) continue;
    const base = s.spriteLayer * layerW * stampH;
    const x0 = s.spriteIdx * frame.cellW;
    for (let dy = 0; dy < stampH; dy++) {
      for (let dx = 0; dx < frame.cellW; dx++) {
        atlas[base + dy * layerW + x0 + dx] = s.bits[dy * frame.cellW + dx]!;
      }
    }
  }
  const width = Math.max(1, frame.cols * frame.cellW);
  const height = Math.max(1, frame.rows * frame.cellH);
  pane.canvas.style.width = `${width / dpr}px`;
  pane.canvas.style.height = `${height / dpr}px`;
  if (pane.canvas.width !== width || pane.canvas.height !== height) {
    pane.canvas.width = width;
    pane.canvas.height = height;
  }
  const img = ctx.createImageData(width, height);
  const data = img.data;
  const view = new DataView(frame.cells.buffer, frame.cells.byteOffset, frame.cells.byteLength);
  for (let row = 0; row < frame.rows; row++) {
    for (let col = 0; col < frame.cols; col++) {
      const i = row * frame.cols + col;
      const o = i * 20;
      const fg = view.getUint32(o, true);
      const bg = view.getUint32(o + 4, true);
      const dec = view.getUint32(o + 8, true);
      const spriteIdx = view.getUint16(o + 12, true);
      const spriteLayer = view.getUint16(o + 14, true);
      const attrs = view.getUint32(o + 16, true);
      const fr = fg & 255;
      const fg_ = (fg >> 8) & 255;
      const fb = (fg >> 16) & 255;
      const br = bg & 255;
      const bg_ = (bg >> 8) & 255;
      const bb = (bg >> 16) & 255;
      const x0 = col * frame.cellW;
      const y0 = row * frame.cellH;
      for (let dy = 0; dy < frame.cellH; dy++) {
        for (let dx = 0; dx < frame.cellW; dx++) {
          const pi = ((y0 + dy) * width + (x0 + dx)) * 4;
          data[pi] = br;
          data[pi + 1] = bg_;
          data[pi + 2] = bb;
          data[pi + 3] = 255;
        }
      }
      if (spriteIdx > 0) {
        const base = spriteLayer * layerW * stampH;
        const sx0 = spriteIdx * frame.cellW;
        for (let dy = 0; dy < frame.cellH; dy++) {
          for (let dx = 0; dx < frame.cellW; dx++) {
            const cover = atlas[base + dy * layerW + sx0 + dx] ?? 0;
            if (!cover) continue;
            const pi = ((y0 + dy) * width + (x0 + dx)) * 4;
            const a = cover / 255;
            data[pi] = Math.round(fr * a + data[pi]! * (1 - a));
            data[pi + 1] = Math.round(fg_ * a + data[pi + 1]! * (1 - a));
            data[pi + 2] = Math.round(fb * a + data[pi + 2]! * (1 - a));
            data[pi + 3] = 255;
          }
        }
      }
      const underline = attrs & 0xf;
      if (underline) {
        const lineY = frame.cellH - 2;
        const dr = dec & 255;
        const dg = (dec >> 8) & 255;
        const db = (dec >> 16) & 255;
        for (let dx = 0; dx < frame.cellW; dx++) {
          const excl =
            spriteIdx > 0
              ? (atlas[
                  spriteLayer * layerW * stampH + frame.cellH * layerW + spriteIdx * frame.cellW + dx
                ] ?? 0)
              : 0;
          if (excl > 128) continue;
          const pi = ((y0 + lineY) * width + (x0 + dx)) * 4;
          data[pi] = dr;
          data[pi + 1] = dg;
          data[pi + 2] = db;
          data[pi + 3] = 255;
        }
      }
    }
  }
  pane.paintGen += 1;
  ctx.setTransform(1, 0, 0, 1, 0, 0);
  ctx.imageSmoothingEnabled = false;
  ctx.putImageData(img, 0, 0);
}

async function paintFrame(sessionId: string, force = false) {
  const pane = state.panes.find((p) => p.session?.id === sessionId);
  if (!pane || pane.exited) return;
  const raw = toBytes(await invoke("session_frame", { id: sessionId, force }).catch(() => new Uint8Array()));
  if (raw.byteLength < 4) {
    // No new screen state. Keep the last frame — clearing flashes the terminal.
    return;
  }
  if (isGpuFrame(raw)) {
    let frame: DecodedGpuFrame | null = null;
    try {
      frame = decodeGpuFrame(raw);
    } catch {
      frame = null;
    }
    if (!frame) return;
    const cellChanged = frame.cellW !== pane.cellW || frame.cellH !== pane.cellH;
    pane.cellW = frame.cellW || pane.cellW;
    pane.cellH = frame.cellH || pane.cellH;
    pane.rasterScale = displayScale();
    // Optional GPU2 trailer: mode_flags + scroll_off + scroll_max (12 bytes).
    {
      let o = 24;
      for (const s of frame.sprites) {
        o += 6 + (s.bits ? s.bits.byteLength : 0);
      }
      o += frame.cols * frame.rows * 20;
      if (raw.byteLength >= o + 12) {
        const tv = new DataView(raw.buffer, raw.byteOffset + o, 12);
        pane.modeFlags = tv.getUint32(0, true);
        pane.scrollOffset = tv.getUint32(4, true);
        pane.scrollMax = tv.getUint32(8, true);
      }
    }
    if (!frame.cols || !frame.rows) {
      clearPaneSurface(pane);
      return;
    }
    // Prefer Canvas2D software composite: WebGL2 on WSL/ZINK often creates a
    // context that clears but never shows glyphs (blank pane + stray cursor).
    if (pane.ctx) {
      paintGpuSoftware(pane, frame, pane.rasterScale);
    } else if (pane.gl) {
      try {
        pane.gl.paint(frame, pane.canvas, pane.rasterScale);
        pane.paintGen += 1;
      } catch {
        /* leave last frame */
      }
    }
    if (cellChanged) scheduleLayout();
    return;
  }
  // RGBA packed frames: w,h,cellW,cellH[,modeFlags][,scrollOff,scrollMax] + pixels.
  if (raw.byteLength < 16) return;
  if (!pane.ctx) return;
  const view = new DataView(raw.buffer, raw.byteOffset, raw.byteLength);
  const width = view.getUint32(0, true);
  const height = view.getUint32(4, true);
  const nextW = view.getUint32(8, true) || pane.cellW;
  const nextH = view.getUint32(12, true) || pane.cellH;
  const pixelsNeeded = width * height * 4;
  const header =
    raw.byteLength >= 28 + pixelsNeeded ? 28 : raw.byteLength >= 20 + pixelsNeeded ? 20 : 16;
  if (header >= 20) pane.modeFlags = view.getUint32(16, true);
  if (header >= 28) {
    pane.scrollOffset = view.getUint32(20, true);
    pane.scrollMax = view.getUint32(24, true);
  }
  const cellChanged = nextW !== pane.cellW || nextH !== pane.cellH;
  pane.cellW = nextW;
  pane.cellH = nextH;
  pane.rasterScale = displayScale();
  if (!width || !height) {
    clearPaneSurface(pane);
    return;
  }
  const pixels = raw.subarray(header);
  if (pixels.byteLength < width * height * 4) {
    clearPaneSurface(pane);
    return;
  }
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
  updateTermScrollbar(pane);
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

async function refreshSide(opts?: { forceWsl?: boolean }) {
  const [hosts, hostsRuntime, groups, identities, snippets, history, forwards, running, wslDistros] =
    await Promise.all([
      invoke<Host[]>("hosts_list"),
      invoke<HostRuntime[]>("hosts_runtime").catch(() => [] as HostRuntime[]),
      invoke<Group[]>("groups_list").catch(() => [] as Group[]),
      invoke<Identity[]>("identities_list"),
      invoke<Snippet[]>("snippets_list"),
      invoke<HistoryEntry[]>("history_search", { query: "", limit: 80 }),
      invoke<PortForward[]>("forwards_list").catch(() => [] as PortForward[]),
      invoke<string[]>("forwards_running").catch(() => [] as string[]),
      IS_WIN
        ? invoke<WslDistro[]>("wsl_list_distros", { force: Boolean(opts?.forceWsl) }).catch(
            () => [] as WslDistro[],
          )
        : Promise.resolve([] as WslDistro[]),
    ]);
  state.hosts = hosts;
  state.hostsRuntime = hostsRuntime;
  state.wslDistros = wslDistros;
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
  // Full rebuild mid-drag destroys the dragged node and cancels HTML5 DnD (#88).
  if (hostGroupDragActive) {
    syncHostHighlights();
    return;
  }
  renderHosts();
}

async function refreshHostsRuntime() {
  try {
    state.hostsRuntime = await invoke<HostRuntime[]>("hosts_runtime");
    if (hostGroupDragActive) {
      syncHostHighlights();
      return;
    }
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
  if (hostId) {
    return state.panes.filter(
      (p) => p.session?.host_id === hostId || p.pending?.hostId === hostId,
    );
  }
  // True local only — exclude WSL (`host_id` wsl:…).
  return state.panes.filter((p) => {
    const hid = p.session?.host_id ?? p.pending?.hostId;
    if (hid?.startsWith("wsl:")) return false;
    return p.session?.kind === "local" || p.pending?.kind === "local";
  });
}

function wslHostId(distro: string) {
  return `wsl:${distro}`;
}

function inferOsFromDistroName(name: string): string {
  const n = name.toLowerCase().replace(/[^a-z0-9]+/g, "");
  const keys = [
    "ubuntu",
    "debian",
    "fedora",
    "arch",
    "alpine",
    "kali",
    "opensuse",
    "suse",
    "oracle",
    "centos",
    "rocky",
    "alma",
    "gentoo",
    "nixos",
    "manjaro",
  ];
  for (const k of keys) {
    if (n.includes(k)) return k === "suse" ? "opensuse" : k;
  }
  return "linux";
}

async function closeBackendSession(sessionId: string) {
  await invoke("session_close", { id: sessionId }).catch(() => undefined);
}

/** Kill backend sessions that no longer belong to any open tab. */
async function closeOrphanSessions(kind: "local" | "ssh" | "wsl", hostId?: string) {
  const live = await invoke<SessionInfo[]>("session_list").catch(() => [] as SessionInfo[]);
  const kept = new Set(
    state.panes.flatMap((p) => (p.session?.id && !p.aborted ? [p.session.id] : [])),
  );
  await Promise.all(
    live
      .filter((s) => {
        if (kept.has(s.id)) return false;
        if (kind === "local") return s.kind === "local" && !s.host_id;
        if (kind === "wsl") return s.kind === "wsl" && !!hostId && s.host_id === hostId;
        return !!hostId && s.host_id === hostId;
      })
      .map((s) => closeBackendSession(s.id)),
  );
}

function renderHosts() {
  // Replacing the hosts DOM mid-drag cancels the HTML5 drag (#88).
  if (hostGroupDragActive) {
    syncHostHighlights();
    return;
  }
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
  
  // If searching, expand ancestors of matching hosts/groups so hits stay visible.
  const expandedBySearch = new Set<string>();
  const searching = Boolean(q.trim());
  if (searching) {
    for (const host of filteredHosts) {
      if (host.group_id) {
        expandedBySearch.add(host.group_id);
        expandAncestors(host.group_id);
      }
    }
    for (const groupId of matchingGroupIds) {
      expandAncestors(groupId);
    }
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
    return `<div class="${classes}" data-host="${h.id}" data-testid="host-${h.id}" title="${escapeHtml(userAtHost)}" tabindex="0" draggable="${IS_WIN ? "false" : "true"}">
        ${hostLeading(h)}
        <div class="body">
          <strong class="host-title">${escapeHtml(h.name || h.hostname)}</strong>
          <small class="host-subtitle"><span class="host-user">${escapeHtml(h.username)}</span><span class="host-sep">@</span><span class="host-addr">${escapeHtml(h.hostname)}${h.port !== 22 ? `:${h.port}` : ""}</span>${keyHtml}</small>
        </div>
        <span class="host-actions">
          <button type="button" class="quick" draggable="false" data-new="${h.id}" data-testid="host-action-new" title="New session" aria-label="New session">${icons.plus}</button>
          <button type="button" class="more" draggable="false" data-more="${h.id}" data-testid="host-action-more" title="More actions" aria-label="More actions" aria-haspopup="menu">${icons.more}</button>
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
    const isExpanded =
      state.expandedGroups.has(group.id) ||
      (searching && (expandedBySearch.has(group.id) || matchingGroupIds.has(group.id)));
    const totalHosts = groupHosts.length;

    let html = `<div class="group-row ${isExpanded ? "expanded" : ""}" data-group="${group.id}" data-testid="group-${group.id}" title="Drop hosts here" role="button" aria-expanded="${isExpanded}">
      <span class="chevron">${icons.chevronRight}</span>
      <span class="leading">${icons.folder}</span>
      <div class="body">
        <strong>${escapeHtml(group.name)}</strong>
        <small class="group-meta">${totalHosts} host${totalHosts === 1 ? "" : "s"}</small>
      </div>
    </div>`;

    if (isExpanded) {
      html += `<div class="group-children" data-group-drop="${group.id}">`;
      if (!groupHosts.length && !children.length) {
        html += `<div class="group-empty" data-testid="group-empty">Drop hosts here</div>`;
      }
      for (const host of groupHosts) {
        html += hostRow(host);
      }
      for (const child of children) {
        html += renderGroup(child, depth + 1);
      }
      html += `</div>`;
    }

    return html;
  };
  
  // Build the panel HTML
  const localActive =
    (active?.session?.kind === "local" || active?.pending?.kind === "local") &&
    !(active?.session?.host_id ?? active?.pending?.hostId)?.startsWith("wsl:");
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

  const filteredWsl = IS_WIN
    ? state.wslDistros.filter((d) => !q.trim() || d.name.toLowerCase().includes(q))
    : [];
  for (const d of filteredWsl) {
    const hid = wslHostId(d.name);
    const runtime = state.hostsRuntime.find((r) => r.host_id === hid);
    const openCount = Math.max(
      runtime?.open_count ?? 0,
      hostPanes(hid).filter((p) => p.session && !p.exited).length,
    );
    const isActive = hostPanes(hid).some((p) => p.id === state.activePane);
    const classes = hostCardClassList({ openCount, active: isActive, kind: "host" });
    const dotClass = connectionDotClassList("local");
    const dotColor = connectionDotColor("local");
    const aria = hostCardAriaStatus("local");
    const countHtml = shouldShowSessionCount(openCount)
      ? `<span class="sess-count" data-testid="open-count-pill" title="${openCount} sessions">${sessionCountLabel(openCount)}</span>`
      : "";
    const subtitle = openCount
      ? `${openCount} open shell${openCount > 1 ? "s" : ""}`
      : d.state || "WSL";
    const osIco = hostOsIcon(inferOsFromDistroName(d.name));
    const defaultMark = d.is_default ? " · default" : "";
    panelHtml += `<div class="${classes}" data-wsl-distro="${escapeHtml(d.name)}" data-testid="host-wsl" title="WSL · ${escapeHtml(d.name)}${defaultMark}" tabindex="0">
        <span class="leading" data-os="${escapeHtml(osIco.os)}">${osIco.icon}</span>
        <div class="body">
          <strong class="host-title">${escapeHtml(d.name)}</strong>
          <small class="host-subtitle">${escapeHtml(subtitle)}${d.is_default ? " · default" : ""}</small>
        </div>
        <span class="host-actions">
          <button type="button" class="quick" data-new-wsl="${escapeHtml(d.name)}" data-testid="host-action-new" title="New session" aria-label="New session">${icons.plus}</button>
          <button type="button" class="more" data-more-wsl="${escapeHtml(d.name)}" data-testid="host-action-more" title="More actions" aria-label="More actions" aria-haspopup="menu">${icons.more}</button>
        </span>
        <span class="host-status">
          <span class="${dotClass}" style="background: ${dotColor};" data-testid="connection-dot" data-state="local" role="status" aria-label="${escapeHtml(aria)}"></span>
          ${countHtml}
        </span>
      </div>`;
  }
  
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
  if (!state.hosts.length && noFilter && !filteredWsl.length) {
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
  } else if (!filteredHosts.length && !filteredWsl.length) {
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
  $("panel-hosts").querySelectorAll<HTMLElement>("[data-wsl-distro]").forEach((el) => {
    const distro = el.dataset.wslDistro!;
    el.onclick = () => focusOrOpenWsl(distro);
    el.oncontextmenu = (ev) => {
      ev.preventDefault();
      const hid = wslHostId(distro);
      const open = hostPanes(hid);
      showMenu(ev.clientX, ev.clientY, [
        { label: "Connect", run: () => void openWsl(distro) },
        { label: "Focus session", run: () => focusHost(hid), hidden: !open.length },
        { label: "Files", run: () => openSftpFor(hid) },
        { label: "Copy name", run: () => void navigator.clipboard.writeText(distro) },
      ]);
    };
  });
  $("panel-hosts").querySelectorAll<HTMLButtonElement>("[data-new-wsl]").forEach((btn) => {
    btn.onclick = (ev) => {
      ev.stopPropagation();
      void openWsl(btn.dataset.newWsl!);
    };
  });
  $("panel-hosts").querySelectorAll<HTMLButtonElement>("[data-more-wsl]").forEach((btn) => {
    btn.onclick = (ev) => {
      ev.stopPropagation();
      const distro = btn.dataset.moreWsl!;
      const hid = wslHostId(distro);
      const rect = btn.getBoundingClientRect();
      showMenu(rect.left, rect.bottom + 4, [
        { label: "Connect", run: () => void openWsl(distro) },
        { label: "Focus session", run: () => focusHost(hid), hidden: !hostPanes(hid).length },
        { label: "Files", run: () => openSftpFor(hid) },
        { label: "Copy name", run: () => void navigator.clipboard.writeText(distro) },
      ]);
    };
  });

  // Group toggle + context menu + drop targets
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
    el.oncontextmenu = (ev) => {
      ev.preventDefault();
      const group = state.groups.find((g) => g.id === el.dataset.group);
      if (!group) return;
      showMenu(ev.clientX, ev.clientY, [
        { label: "Edit group", run: () => editGroup(group) },
        { danger: true, label: "Delete group", run: () => void deleteGroupConfirm(group) },
      ]);
    };
  });

  bindHostGroupDragDrop();

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
  panel.querySelectorAll<HTMLElement>("[data-wsl-distro]").forEach((el) => {
    const hid = wslHostId(el.dataset.wslDistro!);
    const open = hostPanes(hid);
    el.classList.toggle("open", open.length > 0);
    el.classList.toggle("active-host", open.some((p) => p.id === state.activePane));
  });
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
      hideModalOverlay();
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
    $("fwd-nohost-ok").onclick = () => hideModalOverlay();
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
    hideModalOverlay();
    await refreshSide();
  };
  if (existing) {
    $("f-del").onclick = async () => {
      await invoke("forwards_delete", { id: existing.id });
      hideModalOverlay();
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
  window.addEventListener("click", (ev) => {
    if ((ev.target as HTMLElement | null)?.closest?.("#ctx-menu")) return;
    hideMenu();
  });
  window.addEventListener("blur", () => hideMenu(true));
  installContextMenuGuard();
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
  finishOverlayClose($("palette"));
  hideMenu(true);
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
  if (ev.key === "F2" && !ev.ctrlKey && !ev.metaKey && !ev.altKey) {
    const tag = (ev.target as HTMLElement | null)?.tagName;
    if (tag !== "INPUT" && tag !== "TEXTAREA" && state.activePane) {
      ev.preventDefault();
      startTabRename(state.activePane);
      return;
    }
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
  if (
    state.sftpMode &&
    navState.active === "sftp" &&
    (ev.metaKey || ev.ctrlKey) &&
    !ev.altKey &&
    !ev.shiftKey
  ) {
    const tag = (ev.target as HTMLElement | null)?.tagName;
    if (tag !== "INPUT" && tag !== "TEXTAREA") {
      const key = ev.key.toLowerCase();
      if (key === "c") {
        ev.preventDefault();
        fileClipboardCopyFromSelection();
        return;
      }
      if (key === "x") {
        ev.preventDefault();
        fileClipboardCutFromSelection();
        return;
      }
      if (key === "v") {
        const dest = fileClipboardTargetFromFocus();
        if (dest && state.fileClipboard?.items.length) {
          ev.preventDefault();
          void fileClipboardPaste(dest);
          return;
        }
      }
    }
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
  // Host switcher (overrides saved tab.next/prev defaults on this chord).
  // Linux/X11 often reports Shift+Tab as "ISO_Left_Tab"; prefer `code` when present.
  if (
    (ev.ctrlKey || ev.metaKey) &&
    !ev.altKey &&
    (ev.key === "Tab" || ev.key === "ISO_Left_Tab" || ev.code === "Tab")
  ) {
    ev.preventDefault();
    ev.stopPropagation();
    const backward = ev.shiftKey || ev.key === "ISO_Left_Tab";
    cycleHost(backward ? -1 : 1);
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
    (combo === "ctrl+b" || combo === "cmd+b"
      ? "sidebar.toggle"
      : combo === "ctrl+shift+c" || combo === "cmd+shift+c"
        ? "terminal.copy"
        : combo === "ctrl+shift+v" || combo === "cmd+shift+v"
          ? "terminal.paste"
          : "");
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
    case "tab.rename":
      if (state.activePane) startTabRename(state.activePane);
      break;
    case "tab.next":
      cycleTab(1);
      break;
    case "tab.prev":
      cycleTab(-1);
      break;
    case "host.next":
      cycleHost(1);
      break;
    case "host.prev":
      cycleHost(-1);
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
    case "terminal.copy": {
      const pane = activePane();
      if (!pane?.session || pane.exited) break;
      const s = selectionRect(pane);
      if (s) void copyTerminalSelection(pane, s);
      break;
    }
    case "terminal.paste": {
      const pane = activePane();
      if (!pane) break;
      const claimed = claimPasteDelivery(pane._pasteSuppressUntil ?? 0);
      if (claimed === null) break;
      pane._pasteSuppressUntil = claimed;
      void pasteIntoPane(pane);
      break;
    }
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

function focusOrOpenWsl(distro: string) {
  const open = hostPanes(wslHostId(distro));
  if (open.length) selectPane(open[open.length - 1]!.id);
  else void openWsl(distro);
}

function focusHost(hostId: string) {
  const open = hostPanes(hostId);
  if (!open.length) {
    if (hostId.startsWith("wsl:")) void openWsl(hostId.slice(4));
    else void openSsh(hostId);
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

async function openWsl(distro: string, reuse?: Pane) {
  const hostId = wslHostId(distro);
  if (!reuse) await closeOrphanSessions("wsl", hostId);
  const pane = reuse ?? createPendingPane(distro, "wsl", hostId);
  try {
    const size = paneSize(pane);
    const info = await invoke<SessionInfo>("session_open_wsl", {
      distro,
      cols: size.cols,
      rows: size.rows,
      scale: displayScale(),
    });
    if (pane.aborted || !state.panes.some((p) => p.id === pane.id)) {
      await closeBackendSession(info.id);
      return;
    }
    attachSession(info, pane);
    void refreshHostsRuntime();
  } catch (err) {
    if (!pane.aborted) failPane(pane, `Couldn't open WSL · ${distro}`, String(err));
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

function revealOverlay(el: HTMLElement): void {
  // Instant open under E2E so Playwright hit-targets stay layout-stable.
  const reduced = prefersReducedMotion() || import.meta.env.VITE_E2E === "1";
  prepareOverlayOpen(el, reduced);
  if (reduced) return;
  void el.offsetWidth;
  requestAnimationFrame(() => {
    el.classList.remove("motion-prep");
    el.classList.add("motion-open");
  });
}

function concealOverlay(el: HTMLElement, after?: () => void): void {
  const reduced = prefersReducedMotion() || import.meta.env.VITE_E2E === "1";
  const mode = beginOverlayClose(el, reduced);
  if (mode === "instant") {
    after?.();
    return;
  }
  const ms = motionDurationMs(MOTION_FAST_MS, false);
  let done = false;
  const finish = () => {
    if (done) return;
    done = true;
    el.removeEventListener("transitionend", onEnd);
    finishOverlayClose(el);
    after?.();
  };
  const onEnd = (ev: TransitionEvent) => {
    if (ev.target !== el) return;
    finish();
  };
  el.addEventListener("transitionend", onEnd);
  window.setTimeout(finish, ms + 50);
}

function hideModalSheet() {
  const sheet = modalSheetEl();
  sheet.id = "modal-sheet";
  sheet.innerHTML = "";
  concealOverlay($("modal"));
}

function showModalOverlay(): void {
  revealOverlay($("modal"));
}

function hideModalOverlay(): void {
  concealOverlay($("modal"));
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
  showModalOverlay();

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
        $("ssh-fail-ok").onclick = () => hideModalOverlay();
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
        $("ssh-fail-ok").onclick = () => hideModalOverlay();
      }
      return Boolean(pane.session);
    }
    failPane(pane, `Couldn't reach ${host?.name || host?.hostname || "host"}`, ipcErrorText(err));
    openSheet(`<h2>SSH failed</h2><p class="form-error">${escapeHtml(ipcErrorText(err))}</p><div class="row"><button class="primary" id="ssh-fail-ok">Close</button></div>`);
    $("ssh-fail-ok").onclick = () => hideModalOverlay();
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
    if (handleTerminalCut(ev, pane)) return;
    if (handleTerminalPaste(ev, pane)) return;
    const bytes = encodeTermKey(ev, pane);
    if (!bytes) return;
    ev.preventDefault();
    sendText(bytes, pane);
  };
  pane.el.onwheel = (ev) => {
    void handleTermWheel(ev, pane);
  };
  pane.el.onpaste = (ev) => {
    ev.preventDefault();
    if (!pane.session || pane.exited) return;
    const clipboardText = ev.clipboardData?.getData("text") ?? "";
    const action = decideDomPasteAction({
      suppressUntil: pane._pasteSuppressUntil ?? 0,
      clipboardText,
    });
    if (action === "ignore") return;
    // Claim so a following keydown / keybinding twin is skipped.
    pane._pasteSuppressUntil = nextPasteSuppressUntil();
    if (action === "fallback") {
      void pasteIntoPane(pane);
      return;
    }
    void pasteIntoPane(pane, action.send);
  };
  selectPane(pane.id);
  layoutPane(pane);
  scheduleFrame(info.id, true);
  renderHosts();
  void refreshSide();
}

function encodeTermKey(ev: KeyboardEvent, pane?: Pane): string | null {
  if (ev.ctrlKey && ev.key.toLowerCase() === "v") return null;
  if (ev.ctrlKey && ev.key.length === 1) {
    const code = ev.key.toLowerCase().charCodeAt(0);
    if (code >= 97 && code <= 122) return String.fromCharCode(code - 96);
  }
  const appCursor = Boolean(pane && (pane.modeFlags & 0b0001));
  // xterm modifier param: 1 + shift + 2*alt + 4*ctrl (CSI 1;5D = Ctrl+Left).
  const mods = 1 + (ev.shiftKey ? 1 : 0) + (ev.altKey ? 2 : 0) + (ev.ctrlKey ? 4 : 0);
  const arrow = (letter: string, application: string) => {
    if (mods === 1) return appCursor ? application : `\x1b[${letter}`;
    return `\x1b[1;${mods}${letter}`;
  };
  const tilde = (code: number) => (mods === 1 ? `\x1b[${code}~` : `\x1b[${code};${mods}~`);
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
      return arrow("A", "\x1bOA");
    case "ArrowDown":
      return arrow("B", "\x1bOB");
    case "ArrowRight":
      return arrow("C", "\x1bOC");
    case "ArrowLeft":
      return arrow("D", "\x1bOD");
    case "Home":
      return mods === 1 ? arrow("H", "\x1bOH") : `\x1b[1;${mods}H`;
    case "End":
      return mods === 1 ? arrow("F", "\x1bOF") : `\x1b[1;${mods}F`;
    case "Delete":
      return tilde(3);
    case "PageUp":
      return tilde(5);
    case "PageDown":
      return tilde(6);
    default:
      return ev.key.length === 1 && !ev.ctrlKey && !ev.altKey && !ev.metaKey ? ev.key : null;
  }
}

async function handleTermWheel(ev: WheelEvent, pane: Pane) {
  if (!pane.session || pane.exited) return;
  // Ignore pinch-zoom / horizontal-only gestures.
  if (ev.ctrlKey || Math.abs(ev.deltaY) < 0.5) return;
  ev.preventDefault();
  const linePx = Math.max(1, pane.cellH / Math.max(1, pane.rasterScale));
  let lines = 0;
  if (ev.deltaMode === WheelEvent.DOM_DELTA_LINE) {
    lines = Math.round(-ev.deltaY);
  } else if (ev.deltaMode === WheelEvent.DOM_DELTA_PAGE) {
    lines = Math.round(-ev.deltaY * Math.max(1, pane.rows || 24));
  } else {
    lines = Math.round(-ev.deltaY / linePx);
  }
  if (!lines) lines = ev.deltaY < 0 ? 1 : -1;

  // xterm mouse wheel reports when the app enabled mouse tracking.
  if (mouseReportingEnabled(pane.modeFlags) && !ev.shiftKey) {
    const cell = selCellFromEvent(pane, ev) ?? { col: 0, row: 0 };
    const wheel = lines > 0 ? "up" : "down";
    const n = Math.min(32, Math.abs(lines));
    let seq = "";
    for (let i = 0; i < n; i++) {
      const part = encodeTermMouse({
        modeFlags: pane.modeFlags,
        col: cell.col,
        row: cell.row,
        button: xtermButtonCode(0, wheel),
        action: "press",
        shift: ev.shiftKey,
        alt: ev.altKey,
        ctrl: ev.ctrlKey,
      });
      if (part) seq += part;
    }
    if (seq) sendText(seq, pane);
    return;
  }

  const altScreen = Boolean(pane.modeFlags & 0b0100);
  if (altScreen) {
    const appCursor = Boolean(pane.modeFlags & 0b0001);
    const up = appCursor ? "\x1bOA" : "\x1b[A";
    const down = appCursor ? "\x1bOB" : "\x1b[B";
    const seq =
      lines > 0
        ? up.repeat(Math.min(32, Math.abs(lines)))
        : down.repeat(Math.min(32, Math.abs(lines)));
    sendText(seq, pane);
    return;
  }
  const ok = await invoke<boolean>("session_scroll", {
    id: pane.session.id,
    lines,
  }).catch(() => true);
  if (ok === false) {
    const appCursor = Boolean(pane.modeFlags & 0b0001);
    const up = appCursor ? "\x1bOA" : "\x1b[A";
    const down = appCursor ? "\x1bOB" : "\x1b[B";
    const seq =
      lines > 0
        ? up.repeat(Math.min(32, Math.abs(lines)))
        : down.repeat(Math.min(32, Math.abs(lines)));
    sendText(seq, pane);
    return;
  }
  scheduleFrame(pane.session.id, true);
}

function updateTermScrollbar(pane: Pane) {
  const max = pane.scrollMax | 0;
  const offset = pane.scrollOffset | 0;
  if (max <= 0 || (pane.modeFlags & 0b0100)) {
    pane.scrollbar.classList.add("hidden");
    return;
  }
  pane.scrollbar.classList.remove("hidden");
  const track = pane.scrollbar.clientHeight || pane.viewport.clientHeight || 1;
  const minThumb = 24;
  const linePx = Math.max(1, pane.cellH / Math.max(1, pane.rasterScale));
  const content = track + max * linePx;
  const thumbH = Math.max(minThumb, Math.round((track / Math.max(content, 1)) * track));
  const travel = Math.max(1, track - thumbH);
  // offset 0 = bottom (live); offset max = top of history
  const t = (max - Math.min(offset, max)) / max;
  const top = Math.round(t * travel);
  pane.scrollThumb.style.height = `${thumbH}px`;
  pane.scrollThumb.style.transform = `translateY(${top}px)`;
}

function offsetFromScrollbarY(pane: Pane, clientY: number): number {
  const max = pane.scrollMax | 0;
  if (max <= 0) return 0;
  const rect = pane.scrollbar.getBoundingClientRect();
  const track = rect.height || 1;
  const thumbH = pane.scrollThumb.offsetHeight || 24;
  const travel = Math.max(1, track - thumbH);
  const y = Math.min(Math.max(0, clientY - rect.top - thumbH / 2), travel);
  const fromTop = y / travel; // 0 top … 1 bottom
  return Math.round(max * (1 - fromTop));
}

async function scrollPaneTo(pane: Pane, offset: number) {
  if (!pane.session || pane.exited) return;
  const ok = await invoke<boolean>("session_scroll_to", {
    id: pane.session.id,
    offset: Math.max(0, offset | 0),
  }).catch(() => false);
  if (ok !== false) scheduleFrame(pane.session.id, true);
}

function createPane(pending?: Pane["pending"]): Pane {
  const el = document.createElement("div");
  el.className = "pane";
  const viewport = document.createElement("div");
  viewport.className = "term-viewport";
  const canvas = document.createElement("canvas");
  canvas.className = "term-canvas";
  const selLayer = document.createElement("div");
  selLayer.className = "term-selection";
  selLayer.style.display = "none";
  const scrollbar = document.createElement("div");
  scrollbar.className = "term-scrollbar hidden";
  const scrollThumb = document.createElement("div");
  scrollThumb.className = "term-scroll-thumb";
  scrollbar.appendChild(scrollThumb);
  viewport.append(canvas, selLayer, scrollbar);
  const banner = document.createElement("div");
  banner.className = "pane-banner hidden";
  el.append(viewport, banner);
  $("workspace").appendChild(el);
  // Always keep a 2D context for reliable paint. WebGL2 is optional and often
  // broken under WSL/ZINK (blank pane). Compact GPU2 frames still apply.
  const ctx = canvas.getContext("2d", { alpha: false });
  const gl = ctx ? null : tryCreateTermGl(canvas);
  const pane: Pane = {
    id: crypto.randomUUID(),
    canvas,
    ctx,
    gl,
    atlasR8: null,
    atlasW: 1,
    atlasH: 1,
    glyphMap: new Map(),
    el,
    viewport,
    banner,
    cellW: 9,
    cellH: 23,
    modeFlags: 0,
    scrollOffset: 0,
    scrollMax: 0,
    scrollbar,
    scrollThumb,
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
    selectPane(pane.id);
    el.focus();
    const cell = selCellFromEvent(pane, ev);
    if (!cell) return;
    // Mouse protocol apps (vim/htop): report to PTY unless Shift forces selection.
    if (
      pane.session &&
      !pane.exited &&
      mouseReportingEnabled(pane.modeFlags) &&
      !ev.shiftKey &&
      (ev.button === 0 || ev.button === 1 || ev.button === 2)
    ) {
      const btn = xtermButtonCode(ev.button);
      const seq = encodeTermMouse({
        modeFlags: pane.modeFlags,
        col: cell.col,
        row: cell.row,
        button: btn,
        action: "press",
        shift: ev.shiftKey,
        alt: ev.altKey,
        ctrl: ev.ctrlKey,
      });
      if (seq) {
        sendText(seq, pane);
        pane._mouseBtn = btn;
        ev.preventDefault();
        return;
      }
    }
    if (ev.button !== 0) return;
    // Click without drag clears selection; drag creates a new one.
    clearSelection(pane);
    pane._selStart = cell;
    pane._selecting = true;
    ev.preventDefault();
  };
  window.addEventListener("mousemove", (ev) => {
    if (pane._mouseBtn != null && pane.session && !pane.exited) {
      const cell = selCellFromEvent(pane, ev);
      if (!cell) return;
      const seq = encodeTermMouse({
        modeFlags: pane.modeFlags,
        col: cell.col,
        row: cell.row,
        button: pane._mouseBtn,
        action: "motion",
        shift: ev.shiftKey,
        alt: ev.altKey,
        ctrl: ev.ctrlKey,
      });
      if (seq) sendText(seq, pane);
      return;
    }
    if (!pane._selecting || !pane._selStart) return;
    const cell = selCellFromEvent(pane, ev);
    if (!cell) return;
    pane.selAnchor = pane._selStart;
    pane.selFocus = cell;
    renderSelection(pane);
  });
  window.addEventListener("mouseup", (ev) => {
    if (pane._mouseBtn != null && pane.session && !pane.exited) {
      const cell = selCellFromEvent(pane, ev) ?? pane.selFocus ?? { col: 0, row: 0 };
      const seq = encodeTermMouse({
        modeFlags: pane.modeFlags,
        col: cell.col,
        row: cell.row,
        button: pane._mouseBtn,
        action: "release",
        shift: ev.shiftKey,
        alt: ev.altKey,
        ctrl: ev.ctrlKey,
      });
      if (seq) sendText(seq, pane);
      pane._mouseBtn = null;
      pane._selecting = false;
      pane._selStart = null;
      return;
    }
    const down = pane._selStart;
    const selecting = pane._selecting;
    pane._selecting = false;
    pane._selStart = null;
    if (!selecting || !down || !pane.session || pane.exited) return;
    const up = selCellFromEvent(pane, ev) ?? down;
    const dragOccurred = up.row !== down.row || up.col !== down.col;
    if (
      !shouldAttemptLinkOpen({
        button: ev.button,
        shiftKey: ev.shiftKey,
        mouseReporting: mouseReportingEnabled(pane.modeFlags),
        dragOccurred,
      })
    ) {
      return;
    }
    void openTerminalLinkAt(pane, up.row, up.col);
  });
  scrollbar.onmousedown = (ev) => {
    if (ev.button !== 0 || !pane.session || pane.exited) return;
    ev.preventDefault();
    ev.stopPropagation();
    selectPane(pane.id);
    el.focus();
    const onThumb = ev.target === scrollThumb || scrollThumb.contains(ev.target as Node);
    if (onThumb) {
      pane._scrollDrag = { startY: ev.clientY, startOffset: pane.scrollOffset };
    } else {
      const jumped = offsetFromScrollbarY(pane, ev.clientY);
      void scrollPaneTo(pane, jumped);
      pane._scrollDrag = { startY: ev.clientY, startOffset: jumped };
    }
  };
  window.addEventListener("mousemove", (ev) => {
    if (!pane._scrollDrag || !pane.session) return;
    const rect = scrollbar.getBoundingClientRect();
    const track = rect.height || 1;
    const thumbH = scrollThumb.offsetHeight || 24;
    const travel = Math.max(1, track - thumbH);
    const max = pane.scrollMax | 0;
    if (max <= 0) return;
    const dy = ev.clientY - pane._scrollDrag.startY;
    // Dragging down → toward live bottom → decrease offset
    const deltaLines = Math.round((-dy / travel) * max);
    void scrollPaneTo(pane, pane._scrollDrag.startOffset + deltaLines);
  });
  window.addEventListener("mouseup", () => {
    pane._scrollDrag = null;
  });
  el.oncontextmenu = (ev) => {
    ev.preventDefault();
    ev.stopPropagation();
    selectPane(pane.id);
    el.focus();
    // In mouse mode, right-click is for the app; Shift+right keeps our menu.
    if (mouseReportingEnabled(pane.modeFlags) && !ev.shiftKey) return;
    showMenu(ev.clientX, ev.clientY, terminalContextMenuItems(pane));
  };
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

/** Open hosts in sidebar order (local first, then WSL, then SSH), for Ctrl+Tab switching. */
function openHostKeys(): string[] {
  const keys: string[] = [];
  const seen = new Set<string>();
  const add = (key: string) => {
    if (seen.has(key)) return;
    seen.add(key);
    keys.push(key);
  };
  if (hostPanes().length) add("local");
  for (const d of state.wslDistros) {
    const id = wslHostId(d.name);
    if (hostPanes(id).length) add(id);
  }
  for (const h of state.hosts) {
    if (h.deleted_at) continue;
    if (hostPanes(h.id).length) add(h.id);
  }
  for (const p of state.panes) {
    if (p.session?.kind === "local" || p.pending?.kind === "local") {
      const hid = p.session?.host_id ?? p.pending?.hostId;
      if (!hid?.startsWith("wsl:")) add("local");
    } else {
      const id = p.session?.host_id ?? p.pending?.hostId;
      if (id) add(id);
    }
  }
  return keys;
}

function currentHostKey(): string | null {
  const pane = activePane();
  if (!pane) return null;
  const hid = pane.session?.host_id ?? pane.pending?.hostId;
  if (hid?.startsWith("wsl:")) return hid;
  if (pane.session?.kind === "local" || pane.pending?.kind === "local") return "local";
  return hid ?? null;
}

function cycleHost(delta: number) {
  const keys = openHostKeys();
  if (!keys.length) return;
  if (keys.length === 1) {
    const only = keys[0]!;
    if (only === "local") focusOrOpenLocal();
    else if (only.startsWith("wsl:")) focusOrOpenWsl(only.slice(4));
    else focusOrOpenSsh(only);
    return;
  }
  const cur = currentHostKey();
  let idx = cur ? keys.indexOf(cur) : -1;
  if (idx < 0) idx = 0;
  const next = keys[(idx + delta + keys.length) % keys.length]!;
  if (next === "local") focusOrOpenLocal();
  else if (next.startsWith("wsl:")) focusOrOpenWsl(next.slice(4));
  else focusOrOpenSsh(next);
}

function selCellFromEvent(
  pane: Pane,
  ev: { clientX: number; clientY: number },
): { row: number; col: number } | null {
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

async function openTerminalLinkAt(pane: Pane, row: number, col: number): Promise<void> {
  if (!pane.session || pane.exited) return;
  try {
    const line = await invoke<string>("session_selection_text", {
      id: pane.session.id,
      r0: row,
      c0: 0,
      r1: row,
      c1: Math.max(0, pane.cols - 1),
    });
    const hit = findUrlAt(line, col);
    if (!hit || !isOpenableHttpUrl(hit.url)) return;
    await openExternal(hit.url);
  } catch (err) {
    console.error("open link failed", err);
  }
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

function handleTerminalCut(ev: KeyboardEvent, pane: Pane): boolean {
  if (!(ev.ctrlKey || ev.metaKey) || ev.key.toLowerCase() !== "x") return false;
  const s = selectionRect(pane);
  if (!s) return false;
  ev.preventDefault();
  void copyTerminalSelection(pane, s);
  return true;
}

function handleTerminalPaste(ev: KeyboardEvent, pane: Pane): boolean {
  // Ctrl+V and Ctrl+Shift+V (Linux terminal convention) both paste.
  if (!(ev.ctrlKey || ev.metaKey) || ev.key.toLowerCase() !== "v") return false;
  ev.preventDefault();
  // Global keybinding may already have delivered Ctrl+Shift+V (capture phase).
  const claimed = claimPasteDelivery(pane._pasteSuppressUntil ?? 0);
  if (claimed === null) return true;
  pane._pasteSuppressUntil = claimed;
  void pasteIntoPane(pane);
  return true;
}

function selectAllTerminal(pane: Pane): void {
  if (pane.cols < 1 || pane.rows < 1) return;
  pane.selAnchor = { row: 0, col: 0 };
  pane.selFocus = { row: pane.rows - 1, col: pane.cols - 1 };
  renderSelection(pane);
}

function terminalContextMenuItems(pane: Pane): MenuItem[] {
  const hasSel = Boolean(selectionRect(pane));
  const live = Boolean(pane.session && !pane.exited);
  return [
    {
      label: "Copy",
      disabled: !hasSel,
      run: () => {
        const s = selectionRect(pane);
        if (s) void copyTerminalSelection(pane, s);
      },
    },
    {
      label: "Cut",
      disabled: !hasSel,
      run: () => {
        const s = selectionRect(pane);
        if (s) void copyTerminalSelection(pane, s);
      },
    },
    {
      label: "Paste",
      disabled: !live,
      run: () => void pasteIntoPane(pane),
    },
    { sep: true },
    {
      label: "Select all",
      disabled: pane.cols < 1 || pane.rows < 1,
      run: () => selectAllTerminal(pane),
    },
    {
      label: "Clear selection",
      disabled: !hasSel,
      run: () => clearSelection(pane),
    },
  ];
}

async function pasteIntoPane(pane: Pane, textOverride?: string): Promise<void> {
  if (!pane.session || pane.exited) return;
  const text = textOverride ?? (await readClipboard());
  if (!text) return;
  if (import.meta.env.VITE_E2E === "1") {
    (window as any).__pasteIntoPaneCalls = ((window as any).__pasteIntoPaneCalls ?? 0) + 1;
    (window as any).__pasteIntoPaneLast = text;
  }
  const formatted = formatTerminalPaste(text, pane.modeFlags);
  sendText(formatted, pane);
}

async function writeClipboard(text: string): Promise<void> {
  // Prefer the native Tauri clipboard plugin — navigator.clipboard often fails
  // under WebKitGTK on Linux (Fedora etc.) without a usable paste prompt.
  try {
    await tauriClipboardWriteText(text);
    return;
  } catch {
    /* vite preview / missing capability */
  }
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

async function readClipboard(): Promise<string> {
  try {
    const text = await tauriClipboardReadText();
    if (text) return text;
  } catch {
    /* vite preview / e2e without native plugin */
  }
  if (navigator.clipboard?.readText) {
    try {
      return await navigator.clipboard.readText();
    } catch {
      /* fall through */
    }
  }
  return "";
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

function paneTitle(pane?: Pane | null): string {
  if (!pane) return "";
  return resolveTabTitle(
    {
      id: pane.id,
      customTitle: pane.customTitle,
      kind: pane.session?.kind ?? pane.pending?.kind,
      sessionTitle: pane.session?.title,
      pendingTitle: pane.pending?.title,
    },
    state.panes.map((p) => ({
      id: p.id,
      customTitle: p.customTitle,
      kind: p.session?.kind ?? p.pending?.kind,
      sessionTitle: p.session?.title,
      pendingTitle: p.pending?.title,
    })),
  );
}

function paneTabIcon(pane: Pane): { icon: string; os: string } {
  const kind = pane.session?.kind ?? pane.pending?.kind;
  if (kind === "local") return hostOsIcon(state.localOsId);
  if (kind === "wsl") {
    const title = pane.session?.title || pane.pending?.title || "";
    return hostOsIcon(inferOsFromDistroName(title));
  }
  if (kind !== "ssh") return { icon: icons.laptop, os: "" };
  const hostId = pane.session?.host_id ?? pane.pending?.hostId;
  const host = hostId ? state.hosts.find((h) => h.id === hostId) : undefined;
  return host ? hostOsIcon(host.os_id) : { icon: icons.server, os: "" };
}

let editingPaneId: string | null = null;

function startTabRename(paneId: string) {
  const pane = state.panes.find((p) => p.id === paneId);
  if (!pane) return;
  if (editingPaneId && editingPaneId !== paneId) {
    const prevInput = $("tabs").querySelector<HTMLInputElement>(`[data-tab="${editingPaneId}"] .tab-rename-input`);
    if (prevInput) commitTabRename(editingPaneId, prevInput.value);
  }
  editingPaneId = paneId;
  if (state.activePane !== paneId) {
    selectPane(paneId);
  } else {
    renderTabs();
  }
  const root = $("tabs");
  const btn = root.querySelector<HTMLElement>(`[data-tab="${paneId}"]`);
  if (!btn) return;
  const input = btn.querySelector<HTMLInputElement>(".tab-rename-input");
  if (!input) return;
  input.value = pane.customTitle ?? paneTitle(pane);
  btn.scrollIntoView({ behavior: "smooth", block: "nearest", inline: "nearest" });
  requestAnimationFrame(() => {
    input.focus();
    input.select();
  });
}

function commitTabRename(paneId: string, newTitle: string) {
  if (editingPaneId !== paneId) return;
  editingPaneId = null;
  const pane = state.panes.find((p) => p.id === paneId);
  if (!pane) return;
  const sanitized = sanitizeTabTitle(newTitle);
  if (sanitized) {
    pane.customTitle = sanitized;
  } else {
    delete pane.customTitle;
  }
  const active = activePane();
  if (active) $("status-session").textContent = paneTitle(active) || "idle";
  renderTabs();
  if (pane.id === state.activePane) {
    pane.el.focus();
  }
}

function cancelTabRename(paneId: string) {
  if (editingPaneId !== paneId) return;
  editingPaneId = null;
  renderTabs();
  const pane = state.panes.find((p) => p.id === paneId);
  if (pane && pane.id === state.activePane) {
    pane.el.focus();
  }
}

function resetTabName(paneId: string) {
  const pane = state.panes.find((p) => p.id === paneId);
  if (!pane) return;
  delete pane.customTitle;
  if (editingPaneId === paneId) editingPaneId = null;
  const active = activePane();
  if (active) $("status-session").textContent = paneTitle(active) || "idle";
  renderTabs();
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
        return `<button type="button" role="tab" data-tab="${p.id}" data-testid="tab-${p.id}"><span class="tab-ico"${osAttr}>${osIco.icon}</span><span class="live-dot"></span><span class="label"></span><input class="tab-rename-input" data-testid="tab-rename-input" type="text" spellcheck="false" autocomplete="off" /><span class="x" data-close="${p.id}">${icons.close}</span></button>`;
      })
      .join("");
    root.querySelectorAll<HTMLElement>("[data-tab]").forEach((el) => {
      const tabId = el.dataset.tab!;
      el.draggable = editingPaneId !== tabId;
      el.onclick = (ev) => {
        const target = ev.target as HTMLElement;
        if (target.closest(".tab-rename-input")) return;
        const close = target.closest("[data-close]") as HTMLElement | null;
        if (close) {
          ev.stopPropagation();
          void closePane(close.dataset.close!);
        } else {
          selectPane(tabId);
        }
      };
      el.ondblclick = (ev) => {
        const target = ev.target as HTMLElement;
        if (target.closest(".tab-rename-input") || target.closest("[data-close]")) return;
        ev.preventDefault();
        ev.stopPropagation();
        startTabRename(tabId);
      };
      el.onauxclick = (ev) => {
        if (ev.button !== 1) return;
        ev.preventDefault();
        void closePane(tabId);
      };
      el.ondragstart = (ev) => {
        if (editingPaneId === tabId) {
          ev.preventDefault();
          return;
        }
        ev.dataTransfer?.setData("text/plain", tabId);
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
        if (id && id !== tabId) movePane(id, tabId);
      };
      const input = el.querySelector<HTMLInputElement>(".tab-rename-input");
      if (input) {
        input.onclick = (ev) => ev.stopPropagation();
        input.onmousedown = (ev) => ev.stopPropagation();
        input.onmouseup = (ev) => ev.stopPropagation();
        input.ondblclick = (ev) => ev.stopPropagation();
        input.onkeydown = (ev) => {
          ev.stopPropagation();
          handleRenameInputKeydown(ev.key, {
            onCommit: () => {
              ev.preventDefault();
              commitTabRename(tabId, input.value);
            },
            onCancel: () => {
              ev.preventDefault();
              cancelTabRename(tabId);
            },
          });
        };
        input.onblur = () => {
          if (editingPaneId === tabId) {
            commitTabRename(tabId, input.value);
          }
        };
      }
      el.oncontextmenu = (ev) => {
        ev.preventDefault();
        const pane = state.panes.find((p) => p.id === tabId);
        if (!pane) return;
        const menuDescriptors = buildTabContextMenu({
          paneId: pane.id,
          hasCustomTitle: Boolean(pane.customTitle),
          isExited: Boolean(pane.exited),
          totalPanes: state.panes.length,
        });
        const menuItems: MenuItem[] = menuDescriptors.map((desc) => {
          if (desc.sep) return { sep: true };
          let run: (() => void) | undefined;
          switch (desc.action) {
            case "rename":
              run = () => startTabRename(pane.id);
              break;
            case "reset-name":
              run = () => resetTabName(pane.id);
              break;
            case "close":
              run = () => void closePane(pane.id);
              break;
            case "close-others":
              run = () => closeOtherPanes(pane.id);
              break;
            case "close-all":
              run = () => closeAllPanes();
              break;
            case "reconnect":
            case "duplicate":
              run = () => duplicatePane(pane);
              break;
          }
          return {
            label: desc.label,
            testId: desc.testId,
            hidden: desc.hidden,
            disabled: desc.disabled,
            danger: desc.danger,
            run,
          };
        });
        showMenu(ev.clientX, ev.clientY, menuItems);
      };
    });
  }
  for (const pane of state.panes) {
    const btn = root.querySelector<HTMLElement>(`[data-tab="${pane.id}"]`);
    if (!btn) continue;
    const isRenaming = editingPaneId === pane.id;
    btn.classList.toggle("renaming", isRenaming);
    btn.draggable = !isRenaming;
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
    const title = paneTitle(pane);
    const label = btn.querySelector(".label");
    if (label) label.textContent = title;
    btn.title = title;
    const input = btn.querySelector<HTMLInputElement>(".tab-rename-input");
    if (input && !isRenaming) {
      input.value = pane.customTitle ?? title;
    }
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
  if (editingPaneId === id) editingPaneId = null;
  const pane = state.panes.find((p) => p.id === id);
  if (!pane) return;
  pane.aborted = true;
  const sessionId = pane.session?.id;
  const hostId = pane.session?.host_id ?? pane.pending?.hostId;
  const kind = pane.session?.kind ?? pane.pending?.kind;
  const local = kind === "local";
  const wsl = kind === "wsl" || !!hostId?.startsWith("wsl:");
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
  else if (wsl && hostId) await closeOrphanSessions("wsl", hostId);
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
  if (pane.session?.kind === "wsl" && pane.session.host_id?.startsWith("wsl:")) {
    void openWsl(pane.session.host_id.slice(4));
  } else if (pane.pending?.kind === "wsl" && pane.pending.hostId?.startsWith("wsl:")) {
    void openWsl(pane.pending.hostId.slice(4));
  } else if (pane.session?.kind === "ssh" && pane.session.host_id) void openSsh(pane.session.host_id);
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
  const kind = pane.session?.kind ?? pane.pending?.kind;
  const local = kind === "local";
  const wsl = kind === "wsl" || !!hostId?.startsWith("wsl:");
  const distro = wsl ? (hostId?.startsWith("wsl:") ? hostId.slice(4) : pane.pending?.title || "") : "";
  if (pane.session) await closeBackendSession(pane.session.id);
  pane.session = undefined;
  pane.exited = false;
  pane.aborted = false;
  pane.cols = 0;
  pane.rows = 0;
  pane.pending = {
    title: local
      ? "This computer"
      : wsl
        ? distro || pane.pending?.title || "WSL"
        : state.hosts.find((h) => h.id === hostId)?.name || pane.pending?.title || "SSH",
    kind: local ? "local" : wsl ? "wsl" : "ssh",
    hostId: wsl ? wslHostId(distro) : hostId,
  };
  showBanner(
    pane,
    local
      ? "Opening local shell…"
      : wsl
        ? `Opening WSL · ${pane.pending.title}…`
        : `Connecting to ${pane.pending.title}…`,
  );
  renderTabs();
  if (local) await openLocal(pane);
  else if (wsl && distro) await openWsl(distro, pane);
  else if (hostId) await openSsh(hostId, pane);
}

type MenuItem = {
  label?: string;
  testId?: string;
  run?: () => void;
  danger?: boolean;
  hidden?: boolean;
  disabled?: boolean;
  sep?: boolean;
};

function showMenu(x: number, y: number, items: MenuItem[]) {
  const menu = $("ctx-menu");
  const visible = items.filter((i) => !i.hidden);
  if (!visible.length) return;
  menu.innerHTML = "";
  for (const item of visible) {
    if (item.sep) {
      menu.appendChild(document.createElement("hr"));
      continue;
    }
    const btn = document.createElement("button");
    btn.type = "button";
    btn.textContent = item.label ?? "";
    if (item.testId) btn.dataset.testid = item.testId;
    if (item.danger) btn.classList.add("danger");
    if (item.disabled) btn.disabled = true;
    if (!item.disabled && item.run) {
      const run = item.run;
      btn.onclick = (ev) => {
        ev.stopPropagation();
        hideMenu(true);
        run();
      };
    }
    menu.appendChild(btn);
  }
  revealOverlay(menu);
  menu.onclick = (ev) => ev.stopPropagation();
  const pad = 8;
  const left = Math.min(x, window.innerWidth - menu.offsetWidth - pad);
  const top = Math.min(y, window.innerHeight - menu.offsetHeight - pad);
  menu.style.left = `${Math.max(pad, left)}px`;
  menu.style.top = `${Math.max(pad, top)}px`;
}

function hideMenu(immediate = false) {
  const menu = $("ctx-menu");
  const clear = () => {
    menu.innerHTML = "";
  };
  if (immediate || prefersReducedMotion()) {
    finishOverlayClose(menu);
    clear();
    return;
  }
  concealOverlay(menu, clear);
}

/** Always block the Tauri/browser native context menu; scoped handlers show `#ctx-menu`. */
function installContextMenuGuard() {
  document.addEventListener(
    "contextmenu",
    (ev) => {
      ev.preventDefault();
    },
    true,
  );
  document.addEventListener("contextmenu", (ev) => {
    const t = ev.target as HTMLElement | null;
    if (!t || t.closest("#ctx-menu")) return;
    if (t instanceof HTMLInputElement || t instanceof HTMLTextAreaElement) {
      if (t.disabled || t.readOnly) return;
      showMenu(ev.clientX, ev.clientY, textFieldContextMenuItems(t));
    }
  });
}

function textFieldContextMenuItems(el: HTMLInputElement | HTMLTextAreaElement): MenuItem[] {
  const start = el.selectionStart ?? 0;
  const end = el.selectionEnd ?? 0;
  const hasSel = start !== end;
  const canEdit = !el.disabled && !el.readOnly;
  return [
    {
      label: "Cut",
      disabled: !canEdit || !hasSel,
      run: () => {
        el.focus();
        document.execCommand("cut");
      },
    },
    {
      label: "Copy",
      disabled: !hasSel,
      run: () => {
        el.focus();
        document.execCommand("copy");
      },
    },
    {
      label: "Paste",
      disabled: !canEdit,
      run: () => {
        el.focus();
        void (async () => {
          const text = await readClipboard();
          if (!text) return;
          const s = el.selectionStart ?? el.value.length;
          const e = el.selectionEnd ?? el.value.length;
          el.setRangeText(text, s, e, "end");
          el.dispatchEvent(new Event("input", { bubbles: true }));
        })();
      },
    },
    { sep: true },
    {
      label: "Select all",
      disabled: !el.value.length,
      run: () => {
        el.focus();
        el.select();
      },
    },
  ];
}

async function deleteHost(host: Host) {
  await invoke("hosts_delete", { id: host.id });
  await refreshSide();
}

function sendText(text: string, target?: Pane) {
  const pane = target ?? activePane();
  if (!pane?.session) return;
  invoke("session_write", { id: pane.session.id, data: b64encode(text) });
}

function togglePalette() {
  const el = $("palette");
  if (el.classList.contains("hidden") || el.classList.contains("motion-out")) {
    $input("palette-input").value = "";
    renderPalette("");
    revealOverlay(el);
    $input("palette-input").focus();
    return;
  }
  concealOverlay(el);
}

function renderPalette(query: string) {
  const q = query.toLowerCase();
  const items: { label: string; hint: string; run: () => void }[] = [
    { label: "New local shell", hint: "session", run: () => openLocal() },
    ...(state.activePane
      ? [
          {
            label: "Rename active tab",
            hint: "tab",
            run: () => {
              if (state.activePane) startTabRename(state.activePane);
            },
          },
        ]
      : []),
    { label: "Identities", hint: "vault", run: () => void openVault() },
    { label: "Settings", hint: "app", run: () => openSettings() },
    { label: "Sync now", hint: "cloud", run: () => invoke("sync_now").then(refreshSync) },
    ...state.wslDistros.map((d) => ({
      label: `WSL ${d.name}`,
      hint: d.state || "wsl",
      run: () => void openWsl(d.name),
    })),
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
          : i.hint === "tab"
            ? icons.terminal
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
      finishOverlayClose($("palette"));
    };
  });
}

$input("palette-input").addEventListener("input", () => renderPalette($input("palette-input").value));
$input("palette-input").addEventListener("keydown", (ev: Event) => {
  const key = (ev as KeyboardEvent).key;
  const items = [...$("palette-results").querySelectorAll<HTMLElement>("li")];
  const activeIdx = items.findIndex((li) => li.classList.contains("active"));
  if (key === "Escape") {
    concealOverlay($("palette"));
    return;
  }
  if (key === "ArrowDown" || key === "ArrowUp") {
    (ev as KeyboardEvent).preventDefault();
    if (!items.length) return;
    const cur = activeIdx < 0 ? 0 : activeIdx;
    const next =
      key === "ArrowDown" ? (cur + 1) % items.length : (cur - 1 + items.length) % items.length;
    items.forEach((li, i) => li.classList.toggle("active", i === next));
    items[next]?.scrollIntoView({ block: "nearest" });
    return;
  }
  if (key === "Enter") {
    (ev as KeyboardEvent).preventDefault();
    const target = (activeIdx >= 0 ? items[activeIdx] : items[0]) ?? null;
    target?.click();
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
          <button type="button" data-auth="key" class="${hostAuthUi(host.auth_method).segment === "key" ? "on" : ""}">Key</button>
          <button type="button" data-auth="password" class="${hostAuthUi(host.auth_method).segment === "password" ? "on" : ""}">Password</button>
          <button type="button" data-auth="gssapi" class="${hostAuthUi(host.auth_method).segment === "gssapi" ? "on" : ""}">Kerberos</button>
        </div>
      </div>
      <p class="hint" id="gssapi-hint" ${hostAuthUi(host.auth_method).segment === "gssapi" ? "" : "hidden"}>Uses the Kerberos ticket on this computer (<code>kinit</code>). Terminus does not store a Kerberos password.</p>
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
    const ui = hostAuthUi(authValue());
    $("key-row").classList.toggle("hidden", !ui.showKey);
    $("pass-row").classList.toggle("hidden", !ui.showPassword);
    const hint = document.getElementById("gssapi-hint");
    if (hint) hint.hidden = ui.segment !== "gssapi";
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
    if (host.auth_method === "gssapi") {
      host.password = "";
      host.identity_id = null;
    } else {
      host.password = ($("f-pass") as HTMLInputElement).value;
    }
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
    hideModalOverlay();
    await refreshSide();
  };
  const del = document.getElementById("f-del");
  if (del) {
    del.onclick = async () => {
      await invoke("hosts_delete", { id: host.id });
      hideModalOverlay();
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
    hideModalOverlay();
    await refreshSide();
  };
}

function editGroup(existing?: Group) {
  const isNew = !existing;
  const group = existing ?? {
    id: crypto.randomUUID(),
    name: "",
    parent_id: null,
    created_at: new Date().toISOString(),
    updated_at: new Date().toISOString(),
  };

  const parentOpts = [`<option value="">None (top-level)</option>`]
    .concat(
      state.groups
        .filter((g) => g.id !== group.id && !g.parent_id && !g.deleted_at)
        .map(
          (g) =>
            `<option value="${escapeHtml(g.id)}" ${group.parent_id === g.id ? "selected" : ""}>${escapeHtml(g.name)}</option>`,
        ),
    )
    .join("");

  openSheet(`
    <h2>${existing ? "Edit group" : "New group"}</h2>
    <p class="lead">Organize your hosts into collapsible groups. You can drag hosts into an empty group.</p>
    <div class="group-card">
      <label class="cell stack"><span>Name</span><input id="g-name" data-testid="group-name" value="${escapeHtml(group.name)}" placeholder="Production servers" /></label>
      <label class="cell stack"><span>Parent group</span><select id="g-parent" data-testid="group-parent">${parentOpts}</select></label>
      <p class="form-error hidden" id="g-error" data-testid="group-error">Name is required.</p>
    </div>
    <div class="row">
      ${existing ? `<button id="g-del" class="danger" data-testid="group-delete">Delete</button>` : ""}
      <button class="primary" id="g-save" data-testid="group-save">Save</button>
    </div>`);

  $("g-save").onclick = async () => {
    group.name = ($("g-name") as HTMLInputElement).value.trim();
    group.parent_id = ($("g-parent") as HTMLSelectElement).value || null;
    group.updated_at = new Date().toISOString();
    const err = $("g-error");
    if (!group.name) {
      err.classList.remove("hidden");
      ($("g-name") as HTMLInputElement).focus();
      return;
    }
    err.classList.add("hidden");

    await invoke("groups_upsert", { group });
    if (isNew) {
      state.expandedGroups.add(group.id);
      localStorage.setItem("terminus-expanded-groups", JSON.stringify([...state.expandedGroups]));
    }
    hideModalOverlay();
    await refreshSide();
  };

  const del = document.getElementById("g-del");
  if (del) {
    del.onclick = () => {
      hideModalOverlay();
      void deleteGroupConfirm(group);
    };
  }
}

async function deleteGroupConfirm(group: Group) {
  openSheet(`
    <h2>Delete group?</h2>
    <p class="lead">Hosts in this group become ungrouped. Nested groups are deleted too.</p>
    <p class="form-error">${escapeHtml(group.name)}</p>
    <div class="row">
      <button type="button" id="g-del-cancel">Cancel</button>
      <button type="button" class="danger" id="g-del-ok" data-testid="group-delete-confirm">Delete</button>
    </div>`);
  $("g-del-cancel").onclick = () => hideModalOverlay();
  $("g-del-ok").onclick = async () => {
    const affectedGroupIds = computeAffectedGroups(group.id, state.groups);
    const now = new Date().toISOString();
    for (const groupId of affectedGroupIds) {
      const targetGroup = state.groups.find((g) => g.id === groupId);
      if (targetGroup) {
        const deletedGroup = applySoftDelete(targetGroup, now);
        await invoke("groups_upsert", { group: deletedGroup });
      }
    }
    const orphanedHosts = findOrphanedHosts(affectedGroupIds, state.hosts);
    for (const host of orphanedHosts) {
      const detachedHostData = detachHost(host, now);
      await invoke("hosts_upsert", { host: detachedHostData });
    }
    for (const id of affectedGroupIds) state.expandedGroups.delete(id);
    localStorage.setItem("terminus-expanded-groups", JSON.stringify([...state.expandedGroups]));
    hideModalOverlay();
    await refreshSide();
  };
}

const HOST_GROUP_MIME = "application/x-terminus-host";
let hostGroupDnDAbort: AbortController | null = null;
/** Id of the host currently being rearranged (pointer or HTML5 path) (#90). */
let hostGroupDragHostId: string | null = null;
/** Active pointer-path drag ghost (#92). */
let hostDragGhost: GhostState | null = null;

function bindHostGroupDragDrop(): void {
  const panel = $("panel-hosts");
  hostGroupDnDAbort?.abort();
  hostGroupDnDAbort = new AbortController();
  const { signal } = hostGroupDnDAbort;

  let ptrHostEl: HTMLElement | null = null;
  let ptrStartX = 0;
  let ptrStartY = 0;
  let ptrDragging = false;
  let ptrId: number | null = null;
  let suppressHostClick = false;
  const DRAG_THRESH_PX = 8;

  destroyHostDragGhost(hostDragGhost);
  hostDragGhost = null;

  const clearDropMarks = () => {
    panel.classList.remove("drop-ungroup");
    panel.querySelectorAll(".group-row.drop-target, .group-children.drop-target").forEach((n) => {
      n.classList.remove("drop-target");
    });
  };

  const isGroupDropZone = (node: EventTarget | null): HTMLElement | null => {
    const el = node as HTMLElement | null;
    if (!el || typeof el.closest !== "function") return null;
    return el.closest(".group-row, .group-children");
  };

  const endHostDrag = () => {
    destroyHostDragGhost(hostDragGhost);
    hostDragGhost = null;
    hostGroupDragActive = false;
    hostGroupDragHostId = null;
    panel.querySelectorAll("[data-host].is-dragging").forEach((n) => n.classList.remove("is-dragging"));
    clearDropMarks();
  };

  const markUnderPoint = (clientX: number, clientY: number) => {
    const under = document.elementFromPoint(clientX, clientY);
    const zone = isGroupDropZone(under);
    clearDropMarks();
    if (zone) {
      zone.classList.add("drop-target");
      return;
    }
    if ((under as HTMLElement | null)?.closest?.("[data-host],[data-local],[data-wsl-distro]")) {
      return;
    }
    if (under && panel.contains(under)) panel.classList.add("drop-ungroup");
  };

  const finishPointerDrag = (clientX: number, clientY: number) => {
    const hostId = hostGroupDragHostId;
    const under = document.elementFromPoint(clientX, clientY);
    const zone = isGroupDropZone(under);
    const groupId = zone ? zone.dataset.group || zone.dataset.groupDrop || null : null;
    const overHostRow = !!(under as HTMLElement | null)?.closest?.(
      "[data-host],[data-local],[data-wsl-distro]",
    );
    endHostDrag();
    ptrHostEl = null;
    ptrDragging = false;
    ptrId = null;
    if (!hostId) return;
    suppressHostClick = true;
    queueMicrotask(() => {
      suppressHostClick = false;
    });
    if (groupId) {
      void assignHostToGroup(hostId, groupId);
      return;
    }
    if (overHostRow) {
      void refreshHostsRuntime();
      return;
    }
    if (under && panel.contains(under)) {
      void assignHostToGroup(hostId, null);
      return;
    }
    void refreshHostsRuntime();
  };

  panel.addEventListener(
    "selectstart",
    (ev) => {
      if ((ev.target as HTMLElement | null)?.closest?.("[data-host], .group-row, .group-children")) {
        ev.preventDefault();
      }
    },
    { signal },
  );

  // Pointer DnD is Windows/WebView2 only — on Linux/Playwright it races HTML5 dragTo (#97).
  if (IS_WIN) {
    panel.addEventListener(
      "pointerdown",
      (ev) => {
        if (ev.button !== 0) return;
        if ((ev.target as HTMLElement | null)?.closest?.("button, a, input, textarea")) return;
        const hostEl = (ev.target as HTMLElement | null)?.closest?.<HTMLElement>("[data-host]");
        if (!hostEl || !panel.contains(hostEl)) return;
        const id = hostEl.dataset.host || "";
        if (!id) return;
        ptrHostEl = hostEl;
        ptrStartX = ev.clientX;
        ptrStartY = ev.clientY;
        ptrDragging = false;
        ptrId = ev.pointerId;
        window.getSelection()?.removeAllRanges();
      },
      { signal },
    );

    panel.addEventListener(
      "pointermove",
      (ev) => {
        if (!ptrHostEl || ptrId !== ev.pointerId) return;
        const dx = ev.clientX - ptrStartX;
        const dy = ev.clientY - ptrStartY;
        if (!ptrDragging) {
          if (dx * dx + dy * dy < DRAG_THRESH_PX * DRAG_THRESH_PX) return;
          ptrDragging = true;
          hostGroupDragActive = true;
          hostGroupDragHostId = ptrHostEl.dataset.host || null;
          ptrHostEl.classList.add("is-dragging");
          window.getSelection()?.removeAllRanges();
          destroyHostDragGhost(hostDragGhost);
          hostDragGhost = spawnHostDragGhost(ptrHostEl, ev.clientX, ev.clientY);
          scheduleGhostFrame(hostDragGhost);
          try {
            ptrHostEl.setPointerCapture(ev.pointerId);
          } catch {
            /* ignore */
          }
        }
        if (hostDragGhost) {
          hostDragGhost.pointerX = ev.clientX;
          hostDragGhost.pointerY = ev.clientY;
        }
        markUnderPoint(ev.clientX, ev.clientY);
      },
      { signal },
    );

    const onPointerEnd = (ev: PointerEvent) => {
      if (ptrId !== ev.pointerId) return;
      if (ptrDragging) {
        try {
          ptrHostEl?.releasePointerCapture(ev.pointerId);
        } catch {
          /* ignore */
        }
        finishPointerDrag(ev.clientX, ev.clientY);
        return;
      }
      ptrHostEl = null;
      ptrId = null;
    };

    panel.addEventListener("pointerup", onPointerEnd, { signal });
    panel.addEventListener("pointercancel", onPointerEnd, { signal });

    // Suppress the click that would open SSH after a successful rearrange.
    panel.addEventListener(
      "click",
      (ev) => {
        if (!suppressHostClick) return;
        if ((ev.target as HTMLElement | null)?.closest?.("[data-host]")) {
          ev.preventDefault();
          ev.stopPropagation();
        }
      },
      { capture: true, signal },
    );
    return;
  }

  // HTML5 path for non-Windows (Playwright / Linux).
  panel.addEventListener(
    "dragstart",
    (ev) => {
      const hostEl = (ev.target as HTMLElement | null)?.closest?.<HTMLElement>("[data-host]");
      if (!hostEl || !panel.contains(hostEl) || !ev.dataTransfer) return;
      if ((ev.target as HTMLElement | null)?.closest?.("button, a, input")) {
        ev.preventDefault();
        return;
      }
      const id = hostEl.dataset.host || "";
      if (!id) return;
      hostGroupDragHostId = id;
      hostGroupDragActive = true;
      try {
        ev.dataTransfer.setData("text", id);
        ev.dataTransfer.setData("text/plain", id);
        ev.dataTransfer.setData(HOST_GROUP_MIME, id);
      } catch {
        /* ignore */
      }
      ev.dataTransfer.effectAllowed = "all";
      hostEl.classList.add("is-dragging");
    },
    { signal },
  );

  panel.addEventListener(
    "dragend",
    () => {
      endHostDrag();
      void refreshHostsRuntime();
    },
    { signal },
  );

  panel.addEventListener(
    "dragenter",
    (ev) => {
      if (!hostGroupDragActive && !hostGroupDragHostId) return;
      if (isGroupDropZone(ev.target) || panel.contains(ev.target as Node)) ev.preventDefault();
    },
    { signal },
  );

  panel.addEventListener(
    "dragover",
    (ev) => {
      if (!hostGroupDragActive && !hostGroupDragHostId) return;
      const zone = isGroupDropZone(ev.target);
      if (zone) {
        ev.preventDefault();
        if (ev.dataTransfer) ev.dataTransfer.dropEffect = "move";
        clearDropMarks();
        zone.classList.add("drop-target");
        return;
      }
      if ((ev.target as HTMLElement | null)?.closest?.("[data-host],[data-local],[data-wsl-distro]")) {
        clearDropMarks();
        ev.preventDefault();
        if (ev.dataTransfer) ev.dataTransfer.dropEffect = "none";
        return;
      }
      ev.preventDefault();
      if (ev.dataTransfer) ev.dataTransfer.dropEffect = "move";
      clearDropMarks();
      panel.classList.add("drop-ungroup");
    },
    { signal },
  );

  panel.addEventListener(
    "drop",
    (ev) => {
      if (!hostGroupDragActive && !hostGroupDragHostId) return;
      ev.preventDefault();
      ev.stopPropagation();
      let hostId = hostGroupDragHostId || "";
      if (!hostId && ev.dataTransfer) {
        for (const type of ["text", "text/plain", HOST_GROUP_MIME]) {
          try {
            hostId = ev.dataTransfer.getData(type) || hostId;
          } catch {
            /* ignore */
          }
          if (hostId) break;
        }
      }
      const zone = isGroupDropZone(ev.target);
      const groupId = zone ? zone.dataset.group || zone.dataset.groupDrop || null : null;
      endHostDrag();
      if (!hostId) return;
      if (groupId) {
        void assignHostToGroup(hostId, groupId);
        return;
      }
      if ((ev.target as HTMLElement | null)?.closest?.("[data-host],[data-local],[data-wsl-distro]")) {
        void refreshHostsRuntime();
        return;
      }
      void assignHostToGroup(hostId, null);
    },
    { signal },
  );
}

async function assignHostToGroup(hostId: string, groupId: string | null): Promise<void> {
  const host = state.hosts.find((h) => h.id === hostId);
  if (!host) return;
  if ((host.group_id ?? null) === groupId) return;
  host.group_id = groupId;
  host.updated_at = new Date().toISOString();
  await invoke("hosts_upsert", { host });
  if (groupId) {
    state.expandedGroups.add(groupId);
    localStorage.setItem("terminus-expanded-groups", JSON.stringify([...state.expandedGroups]));
  }
  await refreshSide();
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
    () => ({ configured: false, sync_secrets: false, vault_configured: false, vault_unlocked: false }) as SyncStatus,
  );
  void statusPromise.then((status) => {
    const syncSecrets = Boolean(status.sync_secrets);
    const vaultConfigured = Boolean(status.vault_configured);
    const vaultUnlocked = Boolean(status.vault_unlocked);
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
    <p class="lead">Keys and passwords stay on this device unless you enable encrypted vault sync.</p>
    <div class="group-card">
      <label class="cell"><span>Sync secrets<small class="hint">${syncSecrets ? "Encrypted vault sync" : "Secrets stay local"}</small></span>
        <span class="toggle"><input id="vault-sync-secrets" type="checkbox" ${syncSecrets ? "checked" : ""} ${vaultConfigured ? "" : "disabled"} data-testid="vault-sync-secrets" /><span class="track"></span></span>
      </label>
      ${
        vaultConfigured
          ? `<label class="cell stack"><span>${vaultUnlocked ? "Vault unlocked" : "Unlock vault"}</span>
        <input id="vault-passphrase" type="password" placeholder="Vault passphrase" autocomplete="off" ${vaultUnlocked ? "disabled" : ""} />
        </label>
        <div class="row">${vaultUnlocked ? `<button type="button" id="vault-lock-btn">Lock</button>` : `<button type="button" id="vault-unlock-btn">Unlock</button>`}</div>`
          : `<label class="cell stack"><span>Create vault<small class="hint">Argon2id + XChaCha20-Poly1305 — min 8 characters</small></span>
        <input id="vault-passphrase" type="password" placeholder="New vault passphrase" autocomplete="new-password" />
        </label>
        <div class="row"><button type="button" id="vault-create-btn">Create vault</button></div>`
      }
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
    const syncToggle = $("vault-sync-secrets") as HTMLInputElement | null;
    if (syncToggle) {
      syncToggle.onchange = async (ev) => {
        const on = (ev.target as HTMLInputElement).checked;
        try {
          await invoke("sync_set_secrets", { syncSecrets: on });
        } catch (err) {
          (ev.target as HTMLInputElement).checked = !on;
          const msg = document.getElementById("vault-sync-msg");
          if (msg) msg.textContent = String(err);
        }
      };
    }
    const createBtn = document.getElementById("vault-create-btn");
    if (createBtn) {
      createBtn.onclick = async () => {
        const pass = (document.getElementById("vault-passphrase") as HTMLInputElement | null)?.value ?? "";
        try {
          await invoke("vault_create", { passphrase: pass });
          renderVaultSheet();
        } catch (err) {
          const msg = document.getElementById("vault-sync-msg");
          if (msg) msg.textContent = String(err);
        }
      };
    }
    const unlockBtn = document.getElementById("vault-unlock-btn");
    if (unlockBtn) {
      unlockBtn.onclick = async () => {
        const pass = (document.getElementById("vault-passphrase") as HTMLInputElement | null)?.value ?? "";
        try {
          await invoke("vault_unlock", { passphrase: pass });
          renderVaultSheet();
        } catch (err) {
          const msg = document.getElementById("vault-sync-msg");
          if (msg) msg.textContent = String(err);
        }
      };
    }
    const lockBtn = document.getElementById("vault-lock-btn");
    if (lockBtn) {
      lockBtn.onclick = async () => {
        await invoke("vault_lock");
        renderVaultSheet();
      };
    }
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
      <div class="cell"><span>Renderer<small class="hint">Alacritty VT + GPU2 Kitty-style frames (Canvas2D / WebGL2)</small></span><span class="meta">native</span></div>
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
      <label class="cell"><span>Sync secrets<small class="hint">${status.sync_secrets ? "Encrypted (vault)" : "Secrets stay local"}</small></span>
        <span class="toggle"><input id="sync-secrets" type="checkbox" ${status.sync_secrets ? "checked" : ""} ${status.vault_configured ? "" : "disabled"} /><span class="track"></span></span>
      </label>
      ${
        status.vault_configured
          ? `<label class="cell stack"><span>${status.vault_unlocked ? "Vault unlocked" : "Unlock vault"}</span>
             <input id="vault-settings-pass" type="password" placeholder="Vault passphrase" autocomplete="off" ${status.vault_unlocked ? "disabled" : ""} /></label>`
          : `<label class="cell stack"><span>Vault passphrase<small class="hint">Required to sync SSH keys and passwords (min 8 chars)</small></span>
             <input id="vault-settings-pass" type="password" placeholder="Create vault passphrase" autocomplete="new-password" /></label>`
      }
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
    const vaultPass = (document.getElementById("vault-settings-pass") as HTMLInputElement | null)?.value.trim() ?? "";
    try {
      if (!status.vault_configured && vaultPass) {
        await invoke("vault_create", { passphrase: vaultPass });
      } else if (status.vault_configured && !status.vault_unlocked && vaultPass) {
        await invoke("vault_unlock", { passphrase: vaultPass });
      }
    } catch (err) {
      $("sync-msg").textContent = String(err);
      return;
    }
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
    hideModalOverlay();
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
  showModalOverlay();
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
    hideModalOverlay();
    if (start) void editHost();
  };
  $("onboard-skip").onclick = () => finish(false);
  $("onboard-go").onclick = () => finish(true);
  $("sheet-close").onclick = () => {
    markOnboarded();
    localStorage.removeItem("terminus.e2e.showOnboard");
    hideModalOverlay();
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
    $("import-ok").onclick = () => hideModalOverlay();
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
  $("import-ok").onclick = () => hideModalOverlay();
}

function resetSftpCwd() {
  state.sftpCwd = "";
  state.sftpCwdHostId = null;
}

function snapshotActiveRemote(): void {
  const hostId = state.sftpHostId;
  if (!hostId || isLocalEndpoint(hostId)) return;
  sftpSessions = rememberRemote(sftpSessions, hostId, {
    path: state.sftpPath || ".",
    cwd: state.sftpCwd || "",
    root: state.sftpRoot || "/",
    selected: [...state.sftpSelected],
  });
  // Mid-flight loads leave entries=[] while conn is still "connecting".
  // Do not poison the parked-pane cache with that empty snapshot (#107).
  if (state.sftpConn === "connected" || sftpEntryCache.has(hostId) || state.sftpEntries.length > 0) {
    sftpEntryCache.set(hostId, state.sftpEntries.slice());
  }
}

function remotePathFor(hostId: string): string {
  if (!hostId) return ".";
  if (state.sftpHostId === hostId) return state.sftpPath || ".";
  return sftpSessions.byId[hostId]?.path || ".";
}

function remoteRootFor(hostId: string): string {
  if (!hostId) return "/";
  if (state.sftpHostId === hostId) return state.sftpRoot || "/";
  return sftpSessions.byId[hostId]?.root || "/";
}

function remoteEntriesFor(hostId: string): SftpEntry[] {
  if (!hostId) return [];
  if (state.sftpHostId === hostId) return state.sftpEntries;
  return sftpEntryCache.get(hostId)?.slice() ?? [];
}

function remoteSelectionFor(hostId: string): Set<string> {
  if (!hostId) return new Set();
  if (state.sftpHostId === hostId) return state.sftpSelected;
  return new Set(sftpSessions.byId[hostId]?.selected ?? []);
}

function remoteSelectedCount(hostId: string | null | undefined): number {
  if (!hostId || isLocalEndpoint(hostId)) return 0;
  return remoteSelectionFor(hostId).size;
}

function applyRemoteSession(hostId: string): boolean {
  const snap = sftpSessions.byId[hostId];
  if (!snap) return false;
  state.sftpHostId = hostId;
  state.sftpPath = snap.path || ".";
  state.sftpCwd = snap.cwd || "";
  state.sftpCwdHostId = snap.cwd ? hostId : null;
  state.sftpRoot = snap.root || "/";
  state.sftpSelected = new Set(snap.selected);
  state.sftpEntries = sftpEntryCache.get(hostId)?.slice() ?? [];
  return true;
}

function bindLayoutToRemote(hostId: string): void {
  sftpBrowser = focusRemoteInLayout(sftpBrowser, hostId);
}

function paneBodyForEndpoint(endpoint: string): HTMLElement | null {
  if (sftpBrowser.paneA === endpoint) return paneBodyEl("a");
  if (sftpBrowser.paneB === endpoint) return paneBodyEl("b");
  return null;
}

/** Paint a non-active remote that remains visible in the other pane (#103 / #109). */
function paintParkedRemote(hostId: string): void {
  if (!hostId || isLocalEndpoint(hostId)) return;
  const body = paneBodyForEndpoint(hostId);
  if (!body) return;
  const snap = sftpSessions.byId[hostId];
  const live = sftpPaneLive.get(hostId);
  const path = live?.path || snap?.path || ".";
  // Prefer completed cache. If a load is already in flight for this host, wait for it
  // (show Loading) — never start a second sftp_list (#109).
  if (!sftpEntryCache.has(hostId)) {
    body.innerHTML = `${sftpToolbarHtml(hostId, path)}${sftpListLoadingHtml(
      sftpDisplayPath(live?.cwd || snap?.cwd || "", path),
      "sftp-loading",
    )}`;
    bindSftpToolbar(hostId, path);
    if (!sftpLoadInflight.has(hostId)) {
      void loadSftp(hostId, path, { focus: false });
    }
    return;
  }
  const entries = sftpEntryCache.get(hostId)?.slice() ?? [];
  const filtered = filterFileEntries(entries, sftpListFilter());
  const rows = filtered.map((e) => buildRemoteRowHtml(e)).join("");
  const table = filtered.length
    ? `${sftpTableHeadHtml()}<div class="sftp-table" data-testid="sftp-table">${rows}</div>`
    : `${sftpTableHeadHtml()}<div class="sftp-empty" data-testid="sftp-empty"><span>No matching files</span></div>`;
  body.innerHTML = `${sftpToolbarHtml(hostId, path)}${wrapSftpContentEnter(table)}`;
  bindSftpToolbar(hostId, path);
  const pane = body.closest(".sftp-pane") as HTMLElement | null;
  if (pane) delete pane.dataset.boundSftp;
  delete body.dataset.boundSftpCtx;
  bindSftpRows(hostId, body);
}

function paintCompanionRemotes(activeId: string | null): void {
  for (const id of [sftpBrowser.paneA, sftpBrowser.paneB]) {
    if (!id || isLocalEndpoint(id) || id === activeId) continue;
    paintParkedRemote(id);
  }
}

function layoutChanged(
  before: SftpBrowserState,
  after: SftpBrowserState,
): boolean {
  return (
    before.mode !== after.mode ||
    before.paneA !== after.paneA ||
    before.paneB !== after.paneB
  );
}

function paintActiveRemoteFromCache(hostId: string): void {
  const path = state.sftpPath || ".";
  if (state.sftpEntries.length) {
    renderSftpWorkspace(hostId, path, wrapSftpContentEnter(remoteTableHtml(state.sftpEntries)));
    mountRemoteVirtualList(state.sftpEntries);
    bindSftpRows(hostId, remotePaneEl());
    refreshRemoteSelection();
    updateTransferUi();
    return;
  }
  void loadSftp(hostId, path);
}

function activateRemoteSession(hostId: string): void {
  if (!hostId || hostId === state.sftpHostId) {
    renderSftpSidebar(state.sftpHostId);
    return;
  }
  snapshotActiveRemote();
  sftpSessions = openOrFocusRemote(sftpSessions, hostId);
  const restored = applyRemoteSession(hostId);
  if (!restored) {
    state.sftpHostId = hostId;
    state.sftpRoot = "/";
    state.sftpPath = ".";
    state.sftpSelected.clear();
    state.sftpEntries = [];
    resetSftpCwd();
  }
  const before = sftpBrowser;
  bindLayoutToRemote(hostId);
  setActivity("sftp");
  enterSftpMode();
  ensureSftpShell(layoutChanged(before, sftpBrowser));
  paintActiveRemoteFromCache(hostId);
  paintCompanionRemotes(hostId);
  renderSftpSidebar(hostId);
}

function closeRemoteSession(hostId: string): void {
  if (!hostId) return;
  const wasActive = state.sftpHostId === hostId;
  if (wasActive) snapshotActiveRemote();
  const { sessions, nextActive } = closeRemote(sftpSessions, hostId, state.sftpHostId);
  sftpSessions = sessions;
  sftpEntryCache.delete(hostId);
  sftpPaneLive.delete(hostId);
  sftpLoadInflight.delete(hostId);
  detachRemoteFromLayout(hostId);
  if (!wasActive) {
    ensureSftpShell(true);
    if (state.sftpHostId) {
      paintActiveRemoteFromCache(state.sftpHostId);
      paintCompanionRemotes(state.sftpHostId);
    } else if (sftpBrowser.paneA && !isLocalEndpoint(sftpBrowser.paneA)) {
      paintParkedRemote(sftpBrowser.paneA);
    }
    if (sftpBrowser.paneB && isLocalEndpoint(sftpBrowser.paneB)) void initLocalPane();
    renderSftpSidebar(state.sftpHostId);
    return;
  }
  if (!nextActive) {
    state.sftpHostId = null;
    state.sftpEntries = [];
    state.sftpSelected.clear();
    resetSftpCwd();
    sftpBrowser = openSingle(sftpBrowser, SFTP_LOCAL_ID);
    ensureSftpShell(true);
    renderSftpEmpty();
    return;
  }
  applyRemoteSession(nextActive);
  bindLayoutToRemote(nextActive);
  ensureSftpShell(true);
  paintActiveRemoteFromCache(nextActive);
  paintCompanionRemotes(nextActive);
  if (sftpBrowser.paneB && isLocalEndpoint(sftpBrowser.paneB)) void initLocalPane();
  renderSftpSidebar(nextActive);
}

/** Drop a closed remote from the visible layout; prefer remaining remote|local (#103). */
function detachRemoteFromLayout(hostId: string): void {
  const a = sftpBrowser.paneA;
  const b = sftpBrowser.paneB;
  if (a !== hostId && b !== hostId) return;
  const other = a === hostId ? b : a;
  if (other && !isLocalEndpoint(other)) {
    sftpBrowser = { mode: "split", paneA: other, paneB: SFTP_LOCAL_ID };
  } else {
    sftpBrowser = openSingle(sftpBrowser, other && isLocalEndpoint(other) ? SFTP_LOCAL_ID : SFTP_LOCAL_ID);
  }
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
  const previousRemote =
    state.sftpHostId && !isLocalEndpoint(state.sftpHostId) ? state.sftpHostId : null;
  snapshotActiveRemote();
  const existed = hasRemote(sftpSessions, hostId);
  sftpSessions = openOrFocusRemote(sftpSessions, hostId);
  if (existed) {
    applyRemoteSession(hostId);
  } else {
    state.sftpHostId = hostId;
    state.sftpRoot = "/";
    state.sftpPath = ".";
    state.sftpSelected.clear();
    state.sftpEntries = [];
    resetSftpCwd();
  }
  state.localSelected.clear();
  if (previousRemote && previousRemote !== hostId) {
    sftpBrowser = placeAdditionalRemote(sftpBrowser, hostId);
    // Ensure previous stays in layout when we started from single/local.
    if (sftpBrowser.paneA !== previousRemote && sftpBrowser.paneB !== previousRemote) {
      sftpBrowser = placeAdditionalRemote(
        openSingle(sftpBrowser, previousRemote),
        hostId,
      );
    }
  } else {
    bindLayoutToRemote(hostId);
  }
  setActivity("sftp");
  enterSftpMode();
  ensureSftpShell(true);
  if (existed && (sftpEntryCache.has(hostId) || state.sftpEntries.length)) {
    paintActiveRemoteFromCache(hostId);
    paintCompanionRemotes(hostId);
  } else {
    paintCompanionRemotes(hostId);
    void loadSftp(hostId, state.sftpPath || ".").then(() => {
      paintCompanionRemotes(hostId);
    });
  }
  renderSftpSidebar(hostId);
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

function localPaneEl(): HTMLElement | null {
  const view = ensureSftpView();
  const match = view.querySelector<HTMLElement>(
    `[data-endpoint="${SFTP_LOCAL_ID}"] .sftp-pane-body`,
  );
  if (match) return match;
  return (
    view.querySelector<HTMLElement>('[data-testid="sftp-pane-b"] .sftp-pane-body') ??
    view.querySelector<HTMLElement>('[data-testid="sftp-pane-b"]')
  );
}

function sftpEndpointLabel(endpoint: SftpEndpointId): string {
  if (isLocalEndpoint(endpoint)) return "This computer";
  if (endpoint.startsWith("wsl:")) return endpoint.slice(4) || "WSL";
  const host = state.hosts.find((h) => h.id === endpoint);
  return host?.name || host?.hostname || endpoint;
}

function sftpEndpointIcon(endpoint: SftpEndpointId): string {
  if (isLocalEndpoint(endpoint)) return icons.laptop;
  if (endpoint.startsWith("wsl:")) {
    return hostOsIcon(inferOsFromDistroName(endpoint.slice(4))).icon;
  }
  return icons.server;
}

function sftpHostMenuItems(selected: SftpEndpointId): string {
  const local = `<button type="button" class="sftp-host-option" role="option" data-value="${SFTP_LOCAL_ID}" aria-selected="${selected === SFTP_LOCAL_ID}"><span class="sftp-host-ico">${icons.laptop}</span><span>This computer</span></button>`;
  const wsl = state.wslDistros
    .map((d) => {
      const id = wslHostId(d.name);
      const sel = id === selected;
      const ico = hostOsIcon(inferOsFromDistroName(d.name)).icon;
      return `<button type="button" class="sftp-host-option" role="option" data-value="${escapeHtml(id)}" aria-selected="${sel}"><span class="sftp-host-ico">${ico}</span><span>${escapeHtml(d.name)}</span></button>`;
    })
    .join("");
  const hosts = state.hosts
    .map((h) => {
      const sel = h.id === selected;
      return `<button type="button" class="sftp-host-option" role="option" data-value="${escapeHtml(h.id)}" aria-selected="${sel}"><span class="sftp-host-ico">${icons.server}</span><span>${escapeHtml(h.name || h.hostname)}</span></button>`;
    })
    .join("");
  return local + wsl + hosts;
}

function paneChrome(slot: SftpPaneSlot, endpoint: SftpEndpointId): string {
  const label = sftpEndpointLabel(endpoint);
  const ico = sftpEndpointIcon(endpoint);
  return `<div class="sftp-pane-head">
      <div class="sftp-host-select" data-slot="${slot}" data-value="${escapeHtml(endpoint)}" data-testid="sftp-pane-${slot}-host">
        <button type="button" class="sftp-host-trigger" aria-haspopup="listbox" aria-expanded="false" aria-label="Host ${slot.toUpperCase()}">
          <span class="sftp-host-ico" aria-hidden="true">${ico}</span>
          <span class="sftp-host-label">${escapeHtml(label)}</span>
          <span class="sftp-host-chevron" aria-hidden="true">${icons.chevronDown}</span>
        </button>
        <div class="sftp-host-menu hidden" role="listbox">${sftpHostMenuItems(endpoint)}</div>
      </div>
    </div>
    <div class="sftp-pane-body"></div>`;
}

function sftpBatchBarHtml(): string {
  return `<div class="sftp-batch-bar hidden" data-testid="sftp-batch-bar">
        <span class="sftp-batch-count" data-testid="sftp-batch-count">0 selected</span>
        <div class="sftp-batch-actions">
          <button type="button" class="ghost" data-testid="sftp-batch-clear">Clear</button>
          <button type="button" class="primary" data-testid="sftp-batch-upload" title="Upload selection">Upload</button>
          <button type="button" class="ghost" data-testid="sftp-batch-download" title="Download selection">Download</button>
        </div>
      </div>`;
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
    </div>
    ${sftpBatchBarHtml()}`;
    bindBatchBarButtons();
  } else {
    const b = sftpBrowser.paneB ?? SFTP_LOCAL_ID;
    view.innerHTML = `
      <div class="sftp-transfer" data-testid="sftp-transfer"></div>
      <div class="sftp-dual">
        <section class="sftp-pane" data-testid="sftp-pane-a" data-slot="a" data-endpoint="${escapeHtml(sftpBrowser.paneA)}">${paneChrome("a", sftpBrowser.paneA)}</section>
        <div class="sftp-arrows" data-testid="sftp-arrows" aria-hidden="true">
          <button type="button" id="sftp-tx-up" class="sftp-arrow-btn" title="Upload selection" aria-label="Upload" data-testid="sftp-tx-up">${icons.upload}</button>
          <button type="button" id="sftp-tx-down" class="sftp-arrow-btn" title="Download selection" aria-label="Download" data-testid="sftp-tx-down">${icons.download}</button>
        </div>
        <section class="sftp-pane" data-testid="sftp-pane-b" data-slot="b" data-endpoint="${escapeHtml(b)}">${paneChrome("b", b)}</section>
      </div>
      ${sftpBatchBarHtml()}`;
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
    .querySelectorAll<HTMLElement>(".sftp-host-select")
    .forEach((wrap) => {
      if (wrap.dataset.bound === "1") return;
      wrap.dataset.bound = "1";
      const trigger = wrap.querySelector<HTMLButtonElement>(".sftp-host-trigger");
      const menu = wrap.querySelector<HTMLElement>(".sftp-host-menu");
      if (!trigger || !menu) return;

      const close = () => {
        menu.classList.add("hidden");
        trigger.setAttribute("aria-expanded", "false");
        wrap.classList.remove("open");
      };
      const open = () => {
        ensureSftpView().querySelectorAll<HTMLElement>(".sftp-host-select.open").forEach((other) => {
          if (other === wrap) return;
          other.classList.remove("open");
          other.querySelector(".sftp-host-menu")?.classList.add("hidden");
          other.querySelector(".sftp-host-trigger")?.setAttribute("aria-expanded", "false");
        });
        menu.classList.remove("hidden");
        trigger.setAttribute("aria-expanded", "true");
        wrap.classList.add("open");
      };

      trigger.onclick = (ev) => {
        ev.stopPropagation();
        if (wrap.classList.contains("open")) close();
        else open();
      };

      menu.querySelectorAll<HTMLButtonElement>(".sftp-host-option").forEach((opt) => {
        opt.onclick = (ev) => {
          ev.stopPropagation();
          const value = opt.dataset.value as SftpEndpointId;
          const slot = (wrap.dataset.slot || "a") as SftpPaneSlot;
          close();
          void changePaneEndpoint(slot, value);
        };
      });
    });

  if (!(document as Document & { __sftpHostSelectDocBound?: boolean }).__sftpHostSelectDocBound) {
    (document as Document & { __sftpHostSelectDocBound?: boolean }).__sftpHostSelectDocBound = true;
    const closeAll = () => {
      ensureSftpView()
        .querySelectorAll<HTMLElement>(".sftp-host-select.open")
        .forEach((wrap) => {
          wrap.classList.remove("open");
          wrap.querySelector(".sftp-host-menu")?.classList.add("hidden");
          wrap.querySelector(".sftp-host-trigger")?.setAttribute("aria-expanded", "false");
        });
    };
    document.addEventListener("click", closeAll);
    document.addEventListener("keydown", (ev) => {
      if (ev.key === "Escape") closeAll();
    });
  }
}

async function changePaneEndpoint(slot: SftpPaneSlot, endpoint: SftpEndpointId) {
  sftpBrowser = setPaneEndpoint(sftpBrowser, slot, endpoint);
  const section = paneSectionEl(slot);
  if (section) {
    section.dataset.endpoint = endpoint;
    const wrap = section.querySelector<HTMLElement>(".sftp-host-select");
    if (wrap) {
      wrap.dataset.value = endpoint;
      delete wrap.dataset.bound;
      const label = wrap.querySelector(".sftp-host-label");
      const ico = wrap.querySelector(".sftp-host-ico");
      const menu = wrap.querySelector(".sftp-host-menu");
      if (label) label.textContent = sftpEndpointLabel(endpoint);
      if (ico) ico.innerHTML = sftpEndpointIcon(endpoint);
      if (menu) menu.innerHTML = sftpHostMenuItems(endpoint);
      bindPaneHostSelects();
    }
  }
  if (isLocalEndpoint(endpoint)) {
    await initLocalPane();
  } else {
    snapshotActiveRemote();
    const existed = hasRemote(sftpSessions, endpoint);
    sftpSessions = openOrFocusRemote(sftpSessions, endpoint);
    if (existed) {
      applyRemoteSession(endpoint);
      paintActiveRemoteFromCache(endpoint);
    } else {
      state.sftpRoot = "/";
      state.sftpPath = ".";
      state.sftpSelected.clear();
      state.sftpEntries = [];
      resetSftpCwd();
      await loadSftp(endpoint, ".");
    }
  }
  updateTransferUi();
  renderSftpSidebar(state.sftpHostId);
}

async function activateSplit() {
  const keepA = paneBodyIsReady("a");
  const fragA = keepA ? takePaneBody("a") : null;
  sftpBrowser = enterSplit(
    sftpBrowser,
    [
      ...state.wslDistros.map((d) => wslHostId(d.name)),
      ...state.hosts.map((h) => h.id),
    ],
  );
  ensureSftpShell(true);
  renderSftpSidebar(state.sftpHostId);
  if (fragA && putPaneBody("a", fragA)) {
    rehydratePaneA();
  } else if (!isLocalEndpoint(sftpBrowser.paneA)) {
    await loadSftp(sftpBrowser.paneA, state.sftpPath || ".");
  } else {
    await initLocalPane();
  }
  // Companion pane only — do not reload the preserved primary pane.
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

function paneBodyIsReady(slot: SftpPaneSlot): boolean {
  const body = paneBodyEl(slot);
  if (!body) return false;
  return Boolean(
    body.querySelector(
      '[data-testid="sftp-toolbar"], [data-testid="local-toolbar"], [data-testid="sftp-table"], [data-testid="local-table"], [data-testid="sftp-empty"], [data-testid="local-empty"], [data-testid="sftp-error"]',
    ),
  );
}

/** Move pane body nodes out before a shell rebuild so they survive `innerHTML` replace. */
function takePaneBody(slot: SftpPaneSlot): DocumentFragment | null {
  const body = paneBodyEl(slot);
  if (!body?.hasChildNodes()) return null;
  const frag = document.createDocumentFragment();
  while (body.firstChild) frag.appendChild(body.firstChild);
  return frag;
}

function putPaneBody(slot: SftpPaneSlot, frag: DocumentFragment | null): boolean {
  const body = paneBodyEl(slot);
  if (!body || !frag?.childNodes.length) return false;
  body.appendChild(frag);
  return true;
}

/** Re-bind handlers / virtual list after moving a preserved pane body into a new shell. */
function rehydratePaneA(): void {
  const endpoint = sftpBrowser.paneA;
  if (isLocalEndpoint(endpoint)) {
    if (state.localCwd) {
      bindLocalToolbar();
      mountLocalVirtualList(state.localEntries);
      bindLocalRows();
    }
  } else if (state.sftpHostId) {
    bindSftpToolbar(state.sftpHostId, state.sftpPath || ".");
    if (state.sftpEntries.length) mountRemoteVirtualList(state.sftpEntries);
    bindSftpRows(state.sftpHostId, remotePaneEl());
  }
  updateTransferUi();
}

function deactivateSplit() {
  const keepA = paneBodyIsReady("a");
  const fragA = keepA ? takePaneBody("a") : null;
  sftpBrowser = exitSplit(sftpBrowser);
  ensureSftpShell(true);
  renderSftpSidebar(state.sftpHostId);
  if (fragA && putPaneBody("a", fragA)) {
    rehydratePaneA();
    return;
  }
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
    <span class="col-sel"></span><span class="col-ico" aria-hidden="true"></span><span class="col-name">Name</span><span class="col-size">Size</span><span class="col-mtime">Modified</span><span class="col-actions"></span>
  </div>`;
}

const SFTP_SKELETON_WIDTHS = [68, 52, 74, 46, 61, 57, 70];

function sftpListLoadingHtml(label: string, testId: string): string {
  const rows = SFTP_SKELETON_WIDTHS.map(
    (w, i) => `<div class="sftp-skeleton-row sftp-file-row" style="--sk-delay:${i * 60}ms;--sk-name:${w}%">
      <span class="sk sk-check"></span>
      <span class="sk sk-ico"></span>
      <span class="sk sk-name"></span>
      <span class="sk sk-size"></span>
      <span class="sk sk-mtime"></span>
      <span class="sk sk-action"></span>
    </div>`,
  ).join("");
  return `${sftpTableHeadHtml()}
    <div class="sftp-list-loading" data-testid="${escapeHtml(testId)}" role="status" aria-live="polite">
      <span class="sftp-spinner" aria-hidden="true"></span>
      <span>Loading ${escapeHtml(label)}…</span>
    </div>
    <div class="sftp-skeleton" aria-hidden="true">${rows}</div>`;
}

function wrapSftpContentEnter(html: string): string {
  return `<div class="sftp-content-enter">${html}</div>`;
}

function setPaneListingLoading(pane: HTMLElement | null, loading: boolean) {
  if (!pane) return;
  pane.classList.toggle("is-listing-loading", loading);
  pane.querySelectorAll<HTMLElement>('[data-testid="sftp-refresh"], [data-testid="local-refresh"]').forEach((btn) => {
    btn.classList.toggle("is-loading", loading);
    btn.setAttribute("aria-busy", loading ? "true" : "false");
  });
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
          <span class="trail col-actions">
            <button type="button" class="quick local-more" data-testid="local-more" title="Actions" aria-label="Actions">${icons.more}</button>
          </span>
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
  const isSplit = sftpBrowser.mode === "split";
  const splitLabel = isSplit ? "Close split" : "Split view";
  const splitBtn = isSplit
    ? `<button type="button" class="sftp-side-toggle" id="sftp-close-split-btn" data-testid="sftp-close-split-btn" aria-pressed="true" title="${escapeHtml(splitLabel)}">${icons.columns}<span>${escapeHtml(splitLabel)}</span></button>`
    : `<button type="button" class="sftp-side-toggle" id="sftp-split-btn" data-testid="sftp-split-btn" aria-pressed="false" title="${escapeHtml(splitLabel)}">${icons.columns}<span>${escapeHtml(splitLabel)}</span></button>`;
  const hiddenLabel = state.sftpShowHidden ? "Hide hidden files" : "Show hidden files";
  const hiddenIcon = state.sftpShowHidden ? icons.eye : icons.eyeOff;
  const openIds = openRemoteIds(sftpSessions);
  const openList =
    openIds.length === 0
      ? ""
      : `<div class="sftp-open-remotes" data-testid="sftp-open-remotes" role="list">
      <div class="sftp-open-remotes-label">Open remotes</div>
      ${openIds
        .map((id) => {
          const active = id === (hostId || state.sftpHostId);
          const label = sftpEndpointLabel(id);
          const ico = sftpEndpointIcon(id);
          return `<div class="sftp-open-remote${active ? " active" : ""}" role="listitem" data-testid="sftp-open-remote-${escapeHtml(id)}">
            <button type="button" class="sftp-open-remote-btn" data-activate-remote="${escapeHtml(id)}" title="${escapeHtml(label)}">
              <span class="sftp-host-ico" aria-hidden="true">${ico}</span>
              <span class="sftp-open-remote-name">${escapeHtml(label)}</span>
            </button>
            <button type="button" class="sftp-open-remote-close" data-close-remote="${escapeHtml(id)}" title="Close" aria-label="Close ${escapeHtml(label)}">${icons.close}</button>
          </div>`;
        })
        .join("")}
    </div>`;
  $("panel-sftp").innerHTML = `<div class="sftp-side" data-testid="sftp-side">
    ${splitBtn}
    <button type="button" class="sftp-side-toggle" id="sftp-toggle-hidden" data-testid="sftp-toggle-hidden" aria-pressed="${state.sftpShowHidden}" title="${escapeHtml(hiddenLabel)}">${hiddenIcon}<span>${state.sftpShowHidden ? "Hidden on" : "Hidden off"}</span></button>
    ${openList}
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
  $("panel-sftp").querySelectorAll<HTMLElement>("[data-activate-remote]").forEach((btn) => {
    btn.onclick = () => activateRemoteSession(btn.dataset.activateRemote || "");
  });
  $("panel-sftp").querySelectorAll<HTMLElement>("[data-close-remote]").forEach((btn) => {
    btn.onclick = (ev) => {
      ev.preventDefault();
      ev.stopPropagation();
      closeRemoteSession(btn.dataset.closeRemote || "");
    };
  });
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
  const root =
    paneBodyForEndpoint(hostId) ??
    (document.querySelector(`[data-endpoint="${CSS.escape(hostId)}"] .sftp-pane-body`) as HTMLElement | null) ??
    remotePaneEl();
  const q = <T extends HTMLElement>(sel: string) => root.querySelector<T>(sel);
  const up = q('[data-testid="sftp-up"]');
  if (up) {
    up.onclick = () => {
      const parent = parentSftpPath(path);
      if (parent != null) void loadSftp(hostId, parent);
    };
  }
  const refresh = q('[data-testid="sftp-refresh"]');
  if (refresh) refresh.onclick = () => void loadSftp(hostId, path);
  const mkdir = q('[data-testid="sftp-mkdir"]');
  if (mkdir) mkdir.onclick = () => sftpMkdirSheet(hostId, path);
  const upload = q('[data-testid="sftp-upload"]');
  if (upload) upload.onclick = () => void sftpUpload(hostId, path);
  const pathInput = q<HTMLInputElement>('[data-testid="sftp-path"]');
  if (pathInput) {
    pathInput.onkeydown = (ev) => {
      if (ev.key === "Enter") {
        ev.preventDefault();
        void navigateSftpPath(hostId, pathInput.value);
      }
    };
  }
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
    if (hostId !== state.sftpHostId) focusRemoteHostState(hostId);
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
  const host = () => {
    const section = el.closest<HTMLElement>("[data-endpoint]") ?? el;
    const ep = section.dataset.endpoint;
    if (ep && !isLocalEndpoint(ep)) return ep;
    return state.sftpHostId || _hostId;
  };
  const focusRemote = () => {
    state.sftpFocusSide = "remote";
    const hid = host();
    if (hid && hid !== state.sftpHostId) focusRemoteHostState(hid);
  };
  el.addEventListener("pointerdown", focusRemote);
  el.addEventListener("click", (ev) => {
    focusRemote();
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
    const hid = host();
    const path = row.dataset.sftp!;
    const entries = remoteEntriesFor(hid || _hostId);
    const selected = remoteSelectionFor(hid || _hostId);
    const items =
      selected.size > 0
        ? entries.filter((x) => selected.has(x.path))
        : [{ path, name: row.dataset.name || "", is_dir: row.dataset.dir === "true" }];
    setDragPayload(
      ev,
      "remote",
      items.map((x) => ({ path: x.path, name: x.name, is_dir: x.is_dir })),
      hid || _hostId,
    );
  });
  const openRemoteMenu = (ev: MouseEvent, row: HTMLElement | null) => {
    ev.preventDefault();
    ev.stopPropagation();
    focusRemote();
    const hid = host();
    if (!hid) return;
    const cwd = remotePathFor(hid);
    const target = row
      ? {
          side: "remote" as const,
          hostId: hid,
          path: row.dataset.sftp!,
          name: row.dataset.name || "",
          isDir: row.dataset.dir === "true",
          cwd,
        }
      : {
          side: "remote" as const,
          hostId: hid,
          path: cwd,
          name: "",
          isDir: true,
          cwd,
          empty: true,
        };
    showMenu(ev.clientX, ev.clientY, fileContextMenuItems(target));
  };
  const body =
    (root as HTMLElement).classList?.contains("sftp-pane-body")
      ? (root as HTMLElement)
      : (((root as HTMLElement).closest?.(".sftp-pane-body") as HTMLElement | null) ?? el);
  if (body.dataset.boundSftpCtx !== "1") {
    body.dataset.boundSftpCtx = "1";
    body.addEventListener("contextmenu", (ev) => {
      const row = (ev.target as HTMLElement).closest(".sftp-row") as HTMLElement | null;
      openRemoteMenu(ev, row && body.contains(row) ? row : null);
    });
  }
  el.addEventListener("click", (ev) => {
    const more = (ev.target as HTMLElement).closest(".sftp-more") as HTMLButtonElement | null;
    if (!more) return;
    const row = more.closest(".sftp-row") as HTMLElement | null;
    if (!row) return;
    openRemoteMenu(ev, row);
  });
}

/** Switch active remote bookkeeping without rebuilding the shell (#103). */
function focusRemoteHostState(hostId: string): void {
  if (!hostId || hostId === state.sftpHostId || isLocalEndpoint(hostId)) return;
  snapshotActiveRemote();
  sftpSessions = openOrFocusRemote(sftpSessions, hostId);
  if (!applyRemoteSession(hostId)) {
    state.sftpHostId = hostId;
    state.sftpRoot = "/";
    state.sftpPath = ".";
    state.sftpSelected.clear();
    state.sftpEntries = [];
    resetSftpCwd();
  }
  renderSftpSidebar(hostId);
}

type FileCtxTarget = {
  side: "local" | "remote";
  hostId?: string;
  path: string;
  name: string;
  isDir: boolean;
  cwd: string;
  empty?: boolean;
};

function fileClipboardItemsForTarget(target: FileCtxTarget): { path: string; name: string; is_dir: boolean }[] {
  if (target.empty) return [];
  if (target.side === "remote") {
    if (state.sftpSelected.has(target.path) && state.sftpSelected.size > 0) {
      return state.sftpEntries
        .filter((e) => state.sftpSelected.has(e.path))
        .map((e) => ({ path: e.path, name: e.name, is_dir: e.is_dir }));
    }
    return [{ path: target.path, name: target.name, is_dir: target.isDir }];
  }
  if (state.localSelected.has(target.path) && state.localSelected.size > 0) {
    return state.localEntries
      .filter((e) => state.localSelected.has(e.path))
      .map((e) => ({ path: e.path, name: e.name, is_dir: e.is_dir }));
  }
  return [{ path: target.path, name: target.name, is_dir: target.isDir }];
}

function setFileClipboard(
  side: "local" | "remote",
  items: { path: string; name: string; is_dir: boolean }[],
  mode: "copy" | "cut",
  hostId?: string,
) {
  if (!items.length) return;
  state.fileClipboard = {
    side,
    hostId: hostId ?? (side === "remote" ? state.sftpHostId : null),
    items,
    mode,
  };
}

function fileContextMenuItems(target: FileCtxTarget): MenuItem[] {
  const clip = state.fileClipboard;
  const canPaste = Boolean(clip?.items.length);
  const items = fileClipboardItemsForTarget(target);
  const hasTarget = !target.empty && Boolean(target.path);
  const remoteHost = target.hostId || state.sftpHostId || undefined;

  if (target.empty) {
    return [
      {
        label: "Paste",
        disabled: !canPaste,
        run: () => void fileClipboardPaste(target),
      },
      { sep: true },
      {
        label: "New folder",
        run: () => {
          if (target.side === "local") localMkdirSheet();
          else if (remoteHost) sftpMkdirSheet(remoteHost, target.cwd);
        },
      },
      {
        label: "Upload…",
        hidden: target.side !== "remote" || !remoteHost,
        run: () => {
          if (remoteHost) void sftpUpload(remoteHost, target.cwd);
        },
      },
    ];
  }

  const openItem: MenuItem = {
    label: "Open",
    run: () => {
      if (target.side === "remote" && remoteHost) {
        if (target.isDir) void loadSftp(remoteHost, target.path);
        else void sftpOpen(remoteHost, target.path, target.name);
      } else if (target.side === "local" && target.isDir) {
        state.localCwd = target.path;
        void loadLocal();
      }
    },
    hidden: target.side === "local" && !target.isDir,
  };

  return [
    openItem,
    {
      label: "Download",
      hidden: target.side !== "remote" || target.isDir,
      run: () => {
        if (remoteHost) void sftpDownload(remoteHost, target.path, target.name);
      },
    },
    { sep: true },
    {
      label: "Copy",
      disabled: !hasTarget,
      run: () => setFileClipboard(target.side, items, "copy", remoteHost),
    },
    {
      label: "Cut",
      disabled: !hasTarget,
      run: () => setFileClipboard(target.side, items, "cut", remoteHost),
    },
    {
      label: "Paste",
      disabled: !canPaste,
      run: () => void fileClipboardPaste(target),
    },
    { sep: true },
    {
      label: "Rename",
      run: () => {
        if (target.side === "remote" && remoteHost) {
          sftpRenameSheet(remoteHost, target.path, target.name, target.isDir);
        } else {
          localRenameSheet(target.path, target.name, target.isDir);
        }
      },
    },
    {
      label: "New folder",
      run: () => {
        if (target.side === "local") localMkdirSheet();
        else if (remoteHost) sftpMkdirSheet(remoteHost, target.cwd);
      },
    },
    { sep: true },
    {
      label: "Delete",
      danger: true,
      run: () => {
        if (target.side === "remote" && remoteHost) {
          sftpDeleteConfirm(remoteHost, target.path, target.name, target.isDir);
        } else {
          localDeleteConfirm(target.path, target.name, target.isDir);
        }
      },
    },
  ];
}

function pasteDestinationDir(target: FileCtxTarget): string {
  if (target.empty || target.isDir) return target.path || target.cwd;
  if (target.side === "local") return parentLocalPath(target.path) ?? target.cwd;
  return parentSftpPath(target.path) ?? target.cwd;
}

async function fileClipboardPaste(target: FileCtxTarget): Promise<void> {
  const clip = state.fileClipboard;
  if (!clip?.items.length) return;
  const destDir = pasteDestinationDir(target);
  const intoSide = target.side;

  if (clip.side === "local" && intoSide === "remote") {
    const hostId = target.hostId || state.sftpHostId;
    if (!hostId) return;
    const files: FileRef[] = [];
    await collectLocalTree(clip.items, files);
    await transferFiles({
      direction: "upload",
      hostId,
      remoteTargetDir: destDir,
      localTargetDir: state.localCwd,
      files,
    });
  } else if (clip.side === "remote" && intoSide === "local") {
    const hostId = clip.hostId || state.sftpHostId;
    if (!hostId) return;
    const files: FileRef[] = [];
    await collectRemoteTree(hostId, clip.items, files, remoteRootFor(hostId));
    await transferFiles({
      direction: "download",
      hostId,
      remoteTargetDir: remotePathFor(hostId),
      localTargetDir: destDir,
      files,
    });
  } else if (clip.side === "remote" && intoSide === "remote") {
    const srcHost = clip.hostId || state.sftpHostId;
    const destHost = target.hostId || state.sftpHostId;
    if (!srcHost || !destHost) return;
    if (srcHost !== destHost) {
      const files: FileRef[] = [];
      await collectRemoteTree(srcHost, clip.items, files, remoteRootFor(srcHost));
      await transferFiles({
        direction: "remote-copy",
        hostId: srcHost,
        destHostId: destHost,
        sourceRoot: remoteRootFor(srcHost),
        destRoot: remoteRootFor(destHost),
        remoteTargetDir: destDir,
        localTargetDir: "",
        files,
      });
    } else if (clip.mode === "cut") {
      await moveClipboardItems(clip, destDir, intoSide, destHost);
      if (state.sftpHostId) await loadSftp(state.sftpHostId, state.sftpPath || ".");
    } else {
      await copyClipboardItems(clip, destDir, intoSide, destHost);
      if (state.sftpHostId) await loadSftp(state.sftpHostId, state.sftpPath || ".");
    }
  } else if (clip.side === intoSide && clip.mode === "cut") {
    await moveClipboardItems(clip, destDir, intoSide, target.hostId);
    if (intoSide === "local") await loadLocal();
    else if (state.sftpHostId) await loadSftp(state.sftpHostId, state.sftpPath || ".");
  } else if (clip.side === intoSide && clip.mode === "copy") {
    await copyClipboardItems(clip, destDir, intoSide, target.hostId);
    if (intoSide === "local") await loadLocal();
    else if (state.sftpHostId) await loadSftp(state.sftpHostId, state.sftpPath || ".");
  }

  if (clip.mode === "cut") state.fileClipboard = null;
}

async function moveClipboardItems(
  clip: NonNullable<typeof state.fileClipboard>,
  destDir: string,
  side: "local" | "remote",
  hostId?: string,
): Promise<void> {
  for (const it of clip.items) {
    const parent = side === "local" ? parentLocalPath(it.path) : parentSftpPath(it.path);
    if (parent === destDir || it.path === destDir) continue;
    if (side === "local") {
      const to = joinLocalPath(destDir, it.name);
      await invoke("local_rename", { from: it.path, to });
      state.localSelected.delete(it.path);
    } else {
      const hid = hostId || clip.hostId || state.sftpHostId;
      if (!hid) continue;
      const to =
        destDir === "/" ? `/${it.name}` : destDir === "." ? it.name : `${destDir.replace(/\/+$/, "")}/${it.name}`;
      await invoke("sftp_rename", {
        hostId: hid,
        from: resolveUnderRoot(state.sftpRoot, it.path),
        to: resolveUnderRoot(state.sftpRoot, to),
        root: state.sftpRoot,
      });
      state.sftpSelected.delete(it.path);
    }
  }
}

async function copyClipboardItems(
  clip: NonNullable<typeof state.fileClipboard>,
  destDir: string,
  side: "local" | "remote",
  hostId?: string,
): Promise<void> {
  if (side === "local") {
    const files: FileRef[] = [];
    await collectLocalTree(clip.items, files);
    const created = new Set<string>();
    for (const f of files) {
      const relDir = f.rel.includes("/") ? f.rel.slice(0, f.rel.lastIndexOf("/")) : "";
      if (relDir && !created.has(relDir)) {
        created.add(relDir);
        await invoke("local_mkdir", { path: joinLocalPath(destDir, relToSep(relDir)) }).catch(() => undefined);
      }
      const data = await invoke<number[] | Uint8Array>("local_read", { path: f.src });
      await invoke("local_write", {
        path: joinLocalPath(destDir, relToSep(f.rel)),
        data: Array.from(data instanceof Uint8Array ? data : Uint8Array.from(data)),
      });
    }
    return;
  }
  const hid = hostId || clip.hostId || state.sftpHostId;
  if (!hid) return;
  const root = remoteRootFor(hid);
  const files: FileRef[] = [];
  await collectRemoteTree(hid, clip.items, files, root);
  const created = new Set<string>();
  for (const f of files) {
    const relDir = f.rel.includes("/") ? f.rel.slice(0, f.rel.lastIndexOf("/")) : "";
    if (relDir && !created.has(relDir)) {
      created.add(relDir);
      await invoke("sftp_mkdir", {
        hostId: hid,
        path: resolveUnderRoot(root, joinRemote(destDir, relDir)),
        root,
      }).catch(() => undefined);
    }
    const data = await invoke<number[] | Uint8Array>("sftp_read", {
      hostId: hid,
      path: f.src,
      root,
    });
    await invoke("sftp_write", {
      hostId: hid,
      path: resolveUnderRoot(root, joinRemote(destDir, f.rel)),
      data: Array.from(data instanceof Uint8Array ? data : Uint8Array.from(data)),
      root,
    });
  }
}

function fileClipboardTargetFromFocus(): FileCtxTarget | null {
  if (state.sftpFocusSide === "local" && state.localCwd) {
    return {
      side: "local",
      path: state.localCwd,
      name: "",
      isDir: true,
      cwd: state.localCwd,
      empty: true,
    };
  }
  if (state.sftpHostId) {
    return {
      side: "remote",
      hostId: state.sftpHostId,
      path: state.sftpPath || ".",
      name: "",
      isDir: true,
      cwd: state.sftpPath || ".",
      empty: true,
    };
  }
  return null;
}

function fileClipboardCopyFromSelection(): void {
  if (state.sftpFocusSide === "local" && state.localSelected.size) {
    const items = state.localEntries
      .filter((e) => state.localSelected.has(e.path))
      .map((e) => ({ path: e.path, name: e.name, is_dir: e.is_dir }));
    setFileClipboard("local", items, "copy");
    return;
  }
  if (state.sftpSelected.size && state.sftpHostId) {
    const items = state.sftpEntries
      .filter((e) => state.sftpSelected.has(e.path))
      .map((e) => ({ path: e.path, name: e.name, is_dir: e.is_dir }));
    setFileClipboard("remote", items, "copy", state.sftpHostId);
  }
}

function fileClipboardCutFromSelection(): void {
  if (state.sftpFocusSide === "local" && state.localSelected.size) {
    const items = state.localEntries
      .filter((e) => state.localSelected.has(e.path))
      .map((e) => ({ path: e.path, name: e.name, is_dir: e.is_dir }));
    setFileClipboard("local", items, "cut");
    return;
  }
  if (state.sftpSelected.size && state.sftpHostId) {
    const items = state.sftpEntries
      .filter((e) => state.sftpSelected.has(e.path))
      .map((e) => ({ path: e.path, name: e.name, is_dir: e.is_dir }));
    setFileClipboard("remote", items, "cut", state.sftpHostId);
  }
}

function toggleRemoteSelection(path: string, currentlySelected: boolean): void {
  if (currentlySelected) state.sftpSelected.delete(path);
  else state.sftpSelected.add(path);
  refreshRemoteSelection();
  updateTransferUi();
}

function refreshRemoteSelection(): void {
  const root = remotePaneEl();
  if (!root) return;
  root.querySelectorAll<HTMLElement>(".sftp-row").forEach((el) => {
    const sel = state.sftpSelected.has(el.dataset.sftp!);
    el.dataset.selected = String(sel);
    const cell = el.querySelector<HTMLElement>(".cell-sel");
    if (cell) {
      cell.setAttribute("aria-checked", String(sel));
      cell.innerHTML = sel ? icons.check : "";
    }
  });
}

function ensurePaneLive(hostId: string): SftpPaneLive {
  let live = sftpPaneLive.get(hostId);
  if (!live) {
    live = createSftpPaneLive(hostId);
    sftpPaneLive.set(hostId, live);
  }
  return live;
}

/** Resolve home cwd for a host without requiring global focus (#109). */
async function resolveHostCwd(
  hostId: string,
  existing: string,
): Promise<string> {
  if (existing) return existing;
  try {
    const cwd = await invoke<string>("sftp_realpath", { hostId, path: "." });
    return typeof cwd === "string" ? cwd.replace(/\/+$/, "") || cwd : "";
  } catch {
    return "";
  }
}

/** Paint listing into the pane that owns this endpoint (focused or parked). */
function paintHostListing(hostId: string, path: string, entries: SftpEntry[]): void {
  const isFocus = state.sftpHostId === hostId;
  if (isFocus) {
    state.sftpEntries = entries;
    state.sftpPath = path;
    renderSftpWorkspace(hostId, path, wrapSftpContentEnter(remoteTableHtml(entries)));
    if (entries.length) mountRemoteVirtualList(entries);
    bindSftpRows(hostId, remotePaneEl());
    setPaneListingLoading(remotePaneEl(), false);
    updateTransferUi();
    return;
  }
  if (sftpBrowser.paneA === hostId || sftpBrowser.paneB === hostId) {
    paintParkedRemote(hostId);
  }
}

/**
 * Load a remote listing. Per-host loadSeq — opening B never cancels A (#109).
 * `focus: false` updates that host's pane/cache without stealing sidebar focus.
 */
async function loadSftp(
  hostId: string,
  path: string,
  opts?: { focus?: boolean },
): Promise<void> {
  if (!hostId || isLocalEndpoint(hostId)) return;
  const focus = opts?.focus !== false;

  if (focus) {
    if (state.sftpHostId && state.sftpHostId !== hostId) snapshotActiveRemote();
    state.sftpHostId = hostId;
    sftpSessions = openOrFocusRemote(sftpSessions, hostId);
    if (sftpBrowser.mode === "single") {
      sftpBrowser = openSingle(sftpBrowser, hostId);
    } else if (sftpBrowser.paneA !== hostId && sftpBrowser.paneB !== hostId) {
      bindLayoutToRemote(hostId);
    }
    setSftpConn("connecting");
    if (!hostId.startsWith("wsl:") && !state.hosts.length) {
      setSftpConn("disconnected");
      renderSftpEmpty();
      return;
    }
    renderSftpSidebar(hostId);
  } else {
    sftpSessions = openOrFocusRemote(sftpSessions, hostId);
  }

  const snap = sftpSessions.byId[hostId] ?? defaultSessionSnap();
  let live = ensurePaneLive(hostId);
  const hintRaw = path && path.trim() ? path : live.path || snap.path || ".";
  const started = beginPaneLoad(live, hintRaw);
  live = started.live;
  sftpPaneLive.set(hostId, live);
  const seq = started.seq;

  const body = paneBodyForEndpoint(hostId);
  if (focus) {
    const hintLabel = sftpDisplayPath(state.sftpCwd || live.cwd, hintRaw);
    renderSftpWorkspace(hostId, hintRaw, sftpListLoadingHtml(hintLabel, "sftp-loading"));
    setPaneListingLoading(remotePaneEl(), true);
  } else if (body) {
    body.innerHTML = `${sftpToolbarHtml(hostId, hintRaw)}${sftpListLoadingHtml(
      sftpDisplayPath(live.cwd || snap.cwd || "", hintRaw),
      "sftp-loading",
    )}`;
    bindSftpToolbar(hostId, hintRaw);
  }

  // Mark inflight before any await so a concurrent open of B can see A's load (#109).
  let settleInflight!: () => void;
  const inflightGate = new Promise<void>((resolve) => {
    settleInflight = resolve;
  });
  sftpLoadInflight.set(hostId, inflightGate);

  const run = (async () => {
    // Resolve cwd/root from this host's live/snap only — never trust globals after awaits (#109).
    let sessionRoot = live.root || snap.root || "/";
    let sessionCwd = live.cwd || snap.cwd || "";
    sessionCwd = await resolveHostCwd(hostId, sessionCwd);
    if (!isPaneLoadCurrent(sftpPaneLive.get(hostId), seq)) return;

    let safePath: string;
    try {
      const raw = hintRaw && hintRaw.trim() ? hintRaw : ".";
      const anchored = raw.startsWith("/")
        ? raw
        : sessionCwd
          ? normalizeSftpPath(`${sessionCwd}/${raw}`)
          : raw;
      safePath = resolveUnderRoot(sessionRoot, anchored);
    } catch (err) {
      if (!isPaneLoadCurrent(sftpPaneLive.get(hostId), seq)) return;
      const failed = failPaneLoad(ensurePaneLive(hostId), seq, parseSftpError(err));
      if (failed) sftpPaneLive.set(hostId, failed);
      if (focus && state.sftpHostId === hostId) {
        setPaneListingLoading(remotePaneEl(), false);
        renderSftpError(hostId, state.sftpPath, err);
      } else if (body) {
        const typed = parseSftpError(err);
        body.innerHTML = `${sftpToolbarHtml(hostId, hintRaw)}<div class="sftp-error" data-testid="sftp-error" data-kind="${escapeHtml(typed.kind)}" role="alert">
          <strong>${escapeHtml(typed.kind)}</strong>
          <span>${escapeHtml(typed.message)}</span>
        </div>`;
        bindSftpToolbar(hostId, hintRaw);
      }
      return;
    }

    if (focus && state.sftpHostId === hostId) {
      state.sftpPath = safePath;
      state.sftpCwd = sessionCwd;
      state.sftpCwdHostId = hostId;
      state.sftpRoot = sessionRoot;
      renderSftpWorkspace(
        hostId,
        safePath,
        sftpListLoadingHtml(sftpDisplayPath(sessionCwd, safePath), "sftp-loading"),
      );
      setPaneListingLoading(remotePaneEl(), true);
    }

    try {
      const entries = await invoke<SftpEntry[]>("sftp_list", {
        hostId,
        path: safePath,
        root: sessionRoot,
      });
      if (!isPaneLoadCurrent(sftpPaneLive.get(hostId), seq)) return;

      sftpEntryCache.set(hostId, entries.slice());
      sftpSessions = rememberRemote(sftpSessions, hostId, {
        path: safePath,
        cwd: sessionCwd || "",
        root: sessionRoot || "/",
        selected: [...(sftpSessions.byId[hostId]?.selected ?? [])],
      });
      const finished = finishPaneLoad(ensurePaneLive(hostId), seq, {
        path: safePath,
        cwd: sessionCwd || "",
        root: sessionRoot || "/",
      });
      if (finished) sftpPaneLive.set(hostId, finished);

      if (focus && state.sftpHostId === hostId) {
        setSftpConn("connected");
        state.sftpCwd = sessionCwd;
        state.sftpCwdHostId = hostId;
        state.sftpRoot = sessionRoot;
        state.sftpSelected = new Set(sftpSessions.byId[hostId]?.selected ?? []);
        paintHostListing(hostId, safePath, entries);
        snapshotActiveRemote();
        paintCompanionRemotes(hostId);
      } else {
        paintHostListing(hostId, safePath, entries);
      }
    } catch (err) {
      if (!isPaneLoadCurrent(sftpPaneLive.get(hostId), seq)) return;
      const failed = failPaneLoad(ensurePaneLive(hostId), seq, parseSftpError(err));
      if (failed) sftpPaneLive.set(hostId, failed);
      if (focus && state.sftpHostId === hostId) {
        setPaneListingLoading(remotePaneEl(), false);
        renderSftpError(hostId, safePath, err);
      } else {
        const errBody = paneBodyForEndpoint(hostId);
        if (!errBody) return;
        const typed = parseSftpError(err);
        errBody.innerHTML = `${sftpToolbarHtml(hostId, safePath)}<div class="sftp-error" data-testid="sftp-error" data-kind="${escapeHtml(typed.kind)}" role="alert">
          <strong>${escapeHtml(typed.kind)}</strong>
          <span>${escapeHtml(typed.message)}</span>
        </div>`;
        bindSftpToolbar(hostId, safePath);
      }
    }
  })();

  try {
    await run;
  } finally {
    settleInflight();
    if (sftpLoadInflight.get(hostId) === inflightGate) sftpLoadInflight.delete(hostId);
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
  $("sftp-mkdir-cancel").onclick = () => hideModalOverlay();
  $("sftp-mkdir-ok").onclick = async () => {
    const name = ($input("sftp-mkdir-input") as HTMLInputElement).value.trim();
    hideModalOverlay();
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
  $("sftp-rename-cancel").onclick = () => hideModalOverlay();
  $("sftp-rename-ok").onclick = async () => {
    const nextName = ($input("sftp-rename-input") as HTMLInputElement).value.trim();
    if (!nextName || nextName.includes("/") || nextName === "." || nextName === "..") {
      renderSftpError(hostId, state.sftpPath, {
        kind: "SftpPathTraversal",
        message: "invalid name",
        path: nextName,
      });
      hideModalOverlay();
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
      hideModalOverlay();
      await loadSftp(hostId, state.sftpPath);
    } catch (err) {
      hideModalOverlay();
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
  $("sftp-del-cancel").onclick = () => hideModalOverlay();
  $("sftp-del-ok").onclick = async () => {
    try {
      const safe = resolveUnderRoot(state.sftpRoot, path);
      state.sftpSelected.delete(path);
      if (isDir) {
        await invoke("sftp_rmtree", { hostId, path: safe, root: state.sftpRoot });
      } else {
        await invoke("sftp_remove", { hostId, path: safe, isDir, root: state.sftpRoot });
      }
      hideModalOverlay();
      await loadSftp(hostId, state.sftpPath);
    } catch (err) {
      hideModalOverlay();
      renderSftpError(hostId, state.sftpPath, err);
    }
  };
}

// ─── Local pane ─────────────────────────────────────────────────────────────
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
  pane.innerHTML = `${localToolbarHtml(state.localCwd)}${sftpListLoadingHtml(state.localCwd, "local-loading")}`;
  bindLocalToolbar();
  setPaneListingLoading(pane, true);
  try {
    const entries = await invoke<LocalEntry[]>("local_list", { path: state.localCwd });
    state.localEntries = entries;
    pane.innerHTML = `${localToolbarHtml(state.localCwd)}${wrapSftpContentEnter(localTableHtml(entries))}`;
    bindLocalToolbar();
    mountLocalVirtualList(entries);
    bindLocalRows();
    setPaneListingLoading(pane, false);
  } catch (err) {
    pane.innerHTML = `${localToolbarHtml(state.localCwd)}<div class="sftp-error" data-testid="sftp-error"><strong>Local</strong><span>${escapeHtml(String(err))}</span></div>`;
    bindLocalToolbar();
    setPaneListingLoading(pane, false);
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
  if (!pane) return;
  const root = (pane.querySelector(".sftp-virtual-viewport") as HTMLElement | null) ?? pane;
  if (root.dataset.boundLocal === "1") return;
  root.dataset.boundLocal = "1";
  const focusLocal = () => {
    state.sftpFocusSide = "local";
  };
  root.addEventListener("pointerdown", focusLocal);
  root.addEventListener("click", (ev) => {
    focusLocal();
    const t = ev.target as HTMLElement;
    const row = t.closest(".local-row") as HTMLElement | null;
    if (!row || !root.contains(row)) return;
    if (t.closest(".local-more")) return;
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
  const openLocalMenu = (ev: MouseEvent, row: HTMLElement | null) => {
    ev.preventDefault();
    ev.stopPropagation();
    focusLocal();
    const target = row
      ? {
          side: "local" as const,
          path: row.dataset.path!,
          name: row.dataset.name || "",
          isDir: row.dataset.dir === "true",
          cwd: state.localCwd,
        }
      : {
          side: "local" as const,
          path: state.localCwd,
          name: "",
          isDir: true,
          cwd: state.localCwd,
          empty: true,
        };
    showMenu(ev.clientX, ev.clientY, fileContextMenuItems(target));
  };
  const body =
    pane.classList.contains("sftp-pane-body")
      ? pane
      : ((pane.closest(".sftp-pane-body") as HTMLElement | null) ?? root);
  if (body.dataset.boundLocalCtx !== "1") {
    body.dataset.boundLocalCtx = "1";
    body.addEventListener("contextmenu", (ev) => {
      const row = (ev.target as HTMLElement).closest(".local-row") as HTMLElement | null;
      openLocalMenu(ev, row && body.contains(row) ? row : null);
    });
  }
  root.addEventListener("click", (ev) => {
    const more = (ev.target as HTMLElement).closest(".local-more") as HTMLButtonElement | null;
    if (!more) return;
    const row = more.closest(".local-row") as HTMLElement | null;
    if (!row) return;
    openLocalMenu(ev, row);
  });
}

function toggleLocalSelection(path: string, currentlySelected: boolean): void {
  if (currentlySelected) state.localSelected.delete(path);
  else state.localSelected.add(path);
  refreshLocalSelection();
  updateTransferUi();
}

function refreshLocalSelection(): void {
  const root = localPaneEl();
  if (!root) return;
  root.querySelectorAll<HTMLElement>(".local-row").forEach((el) => {
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
  $("local-mkdir-cancel").onclick = () => hideModalOverlay();
  $("local-mkdir-ok").onclick = async () => {
    const name = ($input("local-mkdir-input") as HTMLInputElement).value.trim();
    hideModalOverlay();
    if (!name || name.includes("/") || name.includes("\\")) return;
    try {
      await invoke("local_mkdir", { path: joinLocalPath(state.localCwd, name) });
      await loadLocal();
    } catch (err) {
      console.error("local_mkdir failed", err);
    }
  };
}

function localRenameSheet(path: string, name: string, _isDir: boolean): void {
  openSheet(`
    <h2>Rename</h2>
    <p class="lead">${escapeHtml(name)}</p>
    <label class="field">New name<input id="local-rename-input" data-testid="local-rename-input" value="${escapeHtml(name)}" spellcheck="false" /></label>
    <div class="row">
      <button type="button" id="local-rename-cancel">Cancel</button>
      <button type="button" class="primary" id="local-rename-ok" data-testid="local-rename-ok">Rename</button>
    </div>`);
  $("local-rename-cancel").onclick = () => hideModalOverlay();
  $("local-rename-ok").onclick = async () => {
    const nextName = ($input("local-rename-input") as HTMLInputElement).value.trim();
    hideModalOverlay();
    if (!nextName || nextName.includes("/") || nextName.includes("\\")) return;
    try {
      const parent = parentLocalPath(path) ?? state.localCwd;
      await invoke("local_rename", { from: path, to: joinLocalPath(parent, nextName) });
      state.localSelected.delete(path);
      await loadLocal();
    } catch (err) {
      console.error("local_rename failed", err);
    }
  };
}

function localDeleteConfirm(path: string, name: string, isDir: boolean): void {
  openSheet(`
    <h2>Delete ${isDir ? "folder" : "file"}?</h2>
    <p class="lead">This cannot be undone.</p>
    <p class="form-error">${escapeHtml(name)}</p>
    <div class="row">
      <button type="button" id="local-del-cancel" data-testid="local-del-cancel">Cancel</button>
      <button type="button" class="danger" id="local-del-ok" data-testid="local-del-confirm">Delete</button>
    </div>`);
  $("local-del-cancel").onclick = () => hideModalOverlay();
  $("local-del-ok").onclick = async () => {
    try {
      await invoke("local_remove", { path, isDir });
      state.localSelected.delete(path);
      hideModalOverlay();
      await loadLocal();
    } catch (err) {
      hideModalOverlay();
      console.error("local_remove failed", err);
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
  const lane = transferLane(sftpBrowser.paneA, sftpBrowser.paneB);
  const canTx = lane != null;
  const localN = state.localSelected.size;
  const remoteN = state.sftpSelected.size;
  const paneARemoteN = remoteSelectedCount(
    sftpBrowser.paneA && !isLocalEndpoint(sftpBrowser.paneA) ? sftpBrowser.paneA : null,
  );
  const paneBRemoteN = remoteSelectedCount(
    sftpBrowser.paneB && !isLocalEndpoint(sftpBrowser.paneB) ? sftpBrowser.paneB : null,
  );

  if (lane?.kind === "remote-remote") {
    if (up) {
      up.disabled = paneARemoteN === 0;
      up.title = "Copy selection from left remote to right remote";
    }
    if (down) {
      down.disabled = paneBRemoteN === 0;
      down.title = "Copy selection from right remote to left remote";
    }
  } else {
    if (up) {
      up.disabled = !canTx || localN === 0;
      up.title = "Upload selection";
    }
    if (down) {
      down.disabled = !canTx || remoteN === 0;
      down.title = "Download selection";
    }
  }

  const batch = document.querySelector<HTMLElement>('[data-testid="sftp-batch-bar"]');
  const selectedTotal =
    lane?.kind === "remote-remote" ? paneARemoteN + paneBRemoteN + localN : localN + remoteN;
  const showBatch = selectedTotal > 0;
  if (batch) {
    batch.classList.toggle("hidden", !showBatch);
    const count = batch.querySelector<HTMLElement>('[data-testid="sftp-batch-count"]');
    if (count) count.textContent = `${selectedTotal} selected`;
    const upload = batch.querySelector<HTMLButtonElement>('[data-testid="sftp-batch-upload"]');
    const download = batch.querySelector<HTMLButtonElement>('[data-testid="sftp-batch-download"]');
    if (lane?.kind === "remote-remote") {
      if (upload) {
        upload.disabled = paneARemoteN === 0;
        upload.title = "Copy selection from left remote to right remote";
      }
      if (download) {
        download.disabled = paneBRemoteN === 0;
        download.title = "Copy selection from right remote to left remote";
      }
    } else {
      const canUpload = canTx && batchUploadEnabled(localN);
      const canDownload = batchDownloadEnabled(remoteN);
      if (upload) {
        upload.disabled = !canUpload;
        upload.title = canTx
          ? "Upload selection to remote pane"
          : "Open Split view to upload from This computer";
      }
      if (download) {
        download.disabled = !canDownload;
        download.title = canTx
          ? "Download selection to local pane"
          : "Download selection to your home folder";
      }
    }
  }
}

function bindBatchBarButtons(): void {
  const view = ensureSftpView();
  const clearSelection = () => {
    state.localSelected.clear();
    state.sftpSelected.clear();
    refreshLocalSelection();
    refreshRemoteSelection();
    for (const bar of document.querySelectorAll<HTMLElement>('[data-testid="sftp-batch-bar"]')) {
      bar.classList.add("hidden");
    }
    updateTransferUi();
  };
  // Always (re)bind current buttons — shell rebuilds replace nodes.
  const clear = view.querySelector<HTMLButtonElement>('[data-testid="sftp-batch-clear"]');
  const upload = view.querySelector<HTMLButtonElement>('[data-testid="sftp-batch-upload"]');
  const download = view.querySelector<HTMLButtonElement>('[data-testid="sftp-batch-download"]');
  if (clear) clear.onclick = () => clearSelection();
  if (upload) upload.onclick = () => void transferSelected("upload");
  if (download) download.onclick = () => void transferSelected("download");

  if (view.dataset.boundBatchBar !== "1") {
    view.dataset.boundBatchBar = "1";
    view.addEventListener(
      "click",
      (ev) => {
        const t = ev.target as HTMLElement;
        if (t.closest('[data-testid="sftp-batch-clear"]')) {
          ev.preventDefault();
          clearSelection();
          return;
        }
        if (t.closest('[data-testid="sftp-batch-upload"]')) {
          ev.preventDefault();
          void transferSelected("upload");
          return;
        }
        if (t.closest('[data-testid="sftp-batch-download"]')) {
          ev.preventDefault();
          void transferSelected("download");
        }
      },
      true,
    );
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
  const lane = transferLane(sftpBrowser.paneA, sftpBrowser.paneB);

  if (lane?.kind === "remote-remote") {
    const sourceId = direction === "upload" ? lane.hostA : lane.hostB;
    const destId = direction === "upload" ? lane.hostB : lane.hostA;
    const selected = remoteSelectionFor(sourceId);
    const items = remoteEntriesFor(sourceId).filter((e) => selected.has(e.path));
    if (!items.length) return;
    const files: FileRef[] = [];
    await collectRemoteTree(sourceId, items, files, remoteRootFor(sourceId));
    await transferFiles({
      direction: "remote-copy",
      hostId: sourceId,
      destHostId: destId,
      sourceRoot: remoteRootFor(sourceId),
      destRoot: remoteRootFor(destId),
      remoteTargetDir: remotePathFor(destId),
      localTargetDir: "",
      files,
    });
    return;
  }

  const hostId = state.sftpHostId;
  if (!hostId) return;
  if (direction === "upload") {
    if (!canTransferBetween(sftpBrowser.paneA, sftpBrowser.paneB)) return;
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
    return;
  }

  const items = state.sftpEntries.filter((e) => state.sftpSelected.has(e.path));
  if (!items.length) return;
  let localTarget = state.localCwd;
  if (!localTarget || !canTransferBetween(sftpBrowser.paneA, sftpBrowser.paneB)) {
    try {
      localTarget = state.localCwd || (await invoke<string>("local_home"));
      if (!state.localCwd) {
        state.localCwd = localTarget;
        state.localRoot = localTarget;
      }
    } catch {
      for (const it of items) {
        if (!it.is_dir) await sftpDownload(hostId, it.path, it.name);
      }
      state.sftpSelected.clear();
      refreshRemoteSelection();
      updateTransferUi();
      return;
    }
  }
  const files: FileRef[] = [];
  await collectRemoteTree(hostId, items, files);
  await transferFiles({
    direction: "download",
    hostId,
    remoteTargetDir: state.sftpPath,
    localTargetDir: localTarget,
    files,
  });
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
  root: string = remoteRootFor(hostId),
): Promise<void> {
  for (const it of items) {
    if (it.is_dir) await collectRemoteDir(hostId, it.path, it.name, acc, root);
    else acc.push({ src: it.path, rel: it.name, name: it.name });
  }
}

async function collectRemoteDir(
  hostId: string,
  remotePath: string,
  relDir: string,
  acc: FileRef[],
  root: string = remoteRootFor(hostId),
): Promise<void> {
  const entries = await invoke<SftpEntry[]>("sftp_list", {
    hostId,
    path: remotePath,
    root,
  });
  for (const e of entries) {
    const rel = relDir ? `${relDir}/${e.name}` : e.name;
    if (e.is_dir) await collectRemoteDir(hostId, e.path, rel, acc, root);
    else acc.push({ src: e.path, rel, name: e.name });
  }
}

async function transferFiles(p: {
  direction: "upload" | "download" | "remote-copy";
  hostId: string;
  destHostId?: string;
  sourceRoot?: string;
  destRoot?: string;
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
    label:
      p.direction === "upload"
        ? "Uploading"
        : p.direction === "download"
          ? "Downloading"
          : "Copying",
    active: true,
    cancel: false,
  };
  setTransfer(job);
  const created = new Set<string>();
  const sourceRoot = p.sourceRoot ?? remoteRootFor(p.hostId);
  const destRoot = p.destRoot ?? (p.destHostId ? remoteRootFor(p.destHostId) : sourceRoot);
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
            root: sourceRoot,
          }).catch(() => undefined);
        } else if (p.direction === "download") {
          await invoke("local_mkdir", { path: joinLocalPath(p.localTargetDir, relToSep(relDir)) });
        } else if (p.destHostId) {
          await invoke("sftp_mkdir", {
            hostId: p.destHostId,
            path: joinRemote(p.remoteTargetDir, relDir),
            root: destRoot,
          }).catch(() => undefined);
        }
      }
      if (p.direction === "upload") {
        const data = await invoke<number[] | Uint8Array>("local_read", { path: f.src });
        await invoke("sftp_write", {
          hostId: p.hostId,
          path: joinRemote(p.remoteTargetDir, f.rel),
          data: Array.from(data instanceof Uint8Array ? data : Uint8Array.from(data)),
          root: sourceRoot,
        });
      } else if (p.direction === "download") {
        const data = await invoke<number[] | Uint8Array>("sftp_read", {
          hostId: p.hostId,
          path: f.src,
          root: sourceRoot,
        });
        await invoke("local_write", {
          path: joinLocalPath(p.localTargetDir, relToSep(f.rel)),
          data: Array.from(data instanceof Uint8Array ? data : Uint8Array.from(data)),
        });
      } else if (p.destHostId) {
        const data = await invoke<number[] | Uint8Array>("sftp_read", {
          hostId: p.hostId,
          path: f.src,
          root: sourceRoot,
        });
        await invoke("sftp_write", {
          hostId: p.destHostId,
          path: joinRemote(p.remoteTargetDir, f.rel),
          data: Array.from(data instanceof Uint8Array ? data : Uint8Array.from(data)),
          root: destRoot,
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
  snapshotActiveRemote();
  if (p.direction === "upload") {
    await loadSftp(p.hostId, state.sftpPath);
    paintCompanionRemotes(p.hostId);
  } else if (p.direction === "remote-copy" && p.destHostId) {
    await loadSftp(p.destHostId, p.remoteTargetDir);
    paintCompanionRemotes(p.destHostId);
  } else if (sftpBrowser.mode === "split" || document.querySelector('[data-testid="local-toolbar"]')) {
    await loadLocal();
  }
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
function setDragPayload(
  ev: DragEvent,
  side: "local" | "remote",
  items: DragItem[],
  hostId?: string,
): void {
  if (!ev.dataTransfer) return;
  ev.dataTransfer.setData(DRAG_MIME, JSON.stringify({ side, items, hostId: hostId ?? null }));
  ev.dataTransfer.effectAllowed = "copy";
}

function readDragPayload(
  ev: DragEvent,
): { side: "local" | "remote"; items: DragItem[]; hostId?: string | null } | null {
  const raw = ev.dataTransfer?.getData(DRAG_MIME);
  if (!raw) return null;
  try {
    const parsed = JSON.parse(raw) as {
      side: "local" | "remote";
      items: DragItem[];
      hostId?: string | null;
    };
    if (parsed && Array.isArray(parsed.items)) return parsed;
  } catch {
    /* ignore */
  }
  return null;
}

function renderDndTargets(): void {
  const view = ensureSftpView();
  for (const pane of view.querySelectorAll<HTMLElement>(".sftp-pane[data-endpoint]")) {
    const ep = pane.dataset.endpoint;
    if (!ep) continue;
    if (isLocalEndpoint(ep)) wireDrop(pane, "local", ep);
    else wireDrop(pane, "remote", ep);
  }
}

function wireDrop(pane: HTMLElement, side: "local" | "remote", endpointId: string): void {
  if (pane.dataset.boundDrop === "1") return;
  pane.dataset.boundDrop = "1";
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
    if (payload) void handleDrop(payload, side, endpointId);
  });
}

async function handleDrop(
  payload: { side: "local" | "remote"; items: DragItem[]; hostId?: string | null },
  targetSide: "local" | "remote",
  targetEndpoint: string,
): Promise<void> {
  if (!payload.items.length) return;

  if (targetSide === "remote" && payload.side === "local") {
    const hostId = isLocalEndpoint(targetEndpoint) ? state.sftpHostId : targetEndpoint;
    if (!hostId) return;
    const files: FileRef[] = [];
    await collectLocalTree(payload.items, files);
    await transferFiles({
      direction: "upload",
      hostId,
      remoteTargetDir: remotePathFor(hostId),
      localTargetDir: state.localCwd,
      files,
    });
    return;
  }

  if (targetSide === "local" && payload.side === "remote") {
    const hostId = payload.hostId || state.sftpHostId;
    if (!hostId) return;
    const files: FileRef[] = [];
    await collectRemoteTree(hostId, payload.items, files, remoteRootFor(hostId));
    await transferFiles({
      direction: "download",
      hostId,
      remoteTargetDir: remotePathFor(hostId),
      localTargetDir: state.localCwd,
      files,
    });
    return;
  }

  if (targetSide === "remote" && payload.side === "remote") {
    const sourceId = payload.hostId || state.sftpHostId;
    const destId = targetEndpoint;
    if (!sourceId || !destId || isLocalEndpoint(destId) || sourceId === destId) return;
    const files: FileRef[] = [];
    await collectRemoteTree(sourceId, payload.items, files, remoteRootFor(sourceId));
    await transferFiles({
      direction: "remote-copy",
      hostId: sourceId,
      destHostId: destId,
      sourceRoot: remoteRootFor(sourceId),
      destRoot: remoteRootFor(destId),
      remoteTargetDir: remotePathFor(destId),
      localTargetDir: "",
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
  (window as any).__terminusTabTest = {
    startTabRename,
    commitTabRename,
    cancelTabRename,
    resetTabName,
    getPanes: () =>
      state.panes.map((p) => ({
        id: p.id,
        title: paneTitle(p),
        customTitle: p.customTitle,
        kind: p.session?.kind ?? p.pending?.kind,
      })),
  };
  window.addEventListener("terminus-e2e-refresh", () => {
    void refreshSide();
    void refreshSync();
  });
  window.addEventListener("terminus-e2e-open-vault", () => {
    void openVault();
  });
  window.addEventListener("terminus-e2e-open-sftp", ((ev: CustomEvent<{ hostId: string }>) => {
    const id = ev.detail?.hostId;
    if (id) openSftpFor(id);
  }) as EventListener);
}
