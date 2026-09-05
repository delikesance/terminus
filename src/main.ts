import "./styles.css";
import { icons } from "./icons";
import { applyChrome, type Theme } from "./theme";
import { resolveMonoFont } from "./fonts";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { computeAffectedGroups, findOrphanedHosts, applySoftDelete, detachHost } from "./groupSoftDelete";
import { initTestBridge } from "./testBridge";

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

type HostRuntime = {
  host_id: string;
  connection: string;
  open_count: number;
};

type Identity = {
  id: string;
  name: string;
  public_key?: string | null;
  private_key?: string | null;
  passphrase?: string | null;
};
type Snippet = { id: string; title: string; content: string; tags: string[]; shortcut?: string | null };
type HistoryEntry = { id: string; command: string; cwd?: string | null; session_kind: string; created_at: string };
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
type SyncStatus = { configured: boolean; url?: string | null; last_sync?: string | null; last_error?: string | null; state?: string };
type SftpEntry = { name: string; path: string; is_dir: boolean; size: number };

type Pane = {
  id: string;
  session?: SessionInfo;
  pending?: { title: string; kind: string; hostId?: string };
  exited?: boolean;
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
};

const state = {
  hosts: [] as Host[],
  hostsRuntime: [] as HostRuntime[],
  groups: [] as Group[],
  identities: [] as Identity[],
  snippets: [] as Snippet[],
  history: [] as HistoryEntry[],
  themes: [] as Theme[],
  appearance: null as Appearance | null,
  keybindings: {} as Record<string, string>,
  panes: [] as Pane[],
  activePane: null as string | null,
  sftpHostId: null as string | null,
  customCss: document.createElement("style"),
  expandedGroups: new Set<string>(JSON.parse(localStorage.getItem("terminus-expanded-groups") || "[]")),
};

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
  const [themes, appearance, keybindings] = await Promise.all([
    invoke<Theme[]>("themes_list"),
    invoke<Appearance>("appearance_get"),
    invoke<Record<string, string>>("keybindings_get"),
  ]);
  state.themes = themes;
  state.appearance = appearance;
  state.keybindings = keybindings;
  if (state.appearance) {
    if (state.appearance.renderer === "webgl" || state.appearance.renderer === "canvas") {
      state.appearance.renderer = "auto";
    }
    if (
      !localStorage.getItem("terminus-ux-v2") &&
      ["obsidian", "terminus", "mocha"].includes(state.appearance.theme_id)
    ) {
      state.appearance.theme_id = "graphite";
      localStorage.setItem("terminus-ux-v2", "1");
      void invoke("appearance_set", { appearance: state.appearance });
    }
    state.appearance.font_family = resolveMonoFont();
    state.appearance.letter_spacing = 0;
    state.appearance.line_height = 1.0;
    state.appearance.renderer = "canvas";
    void invoke("appearance_set", { appearance: state.appearance });
  }
  applyAppearance();
  void Promise.all([refreshSide(), refreshSync()]);
  await listen<{ id: string }>("session://output", (ev) => {
    scheduleFrame(ev.payload.id);
  });
  await listen<{ id: string }>("session://exit", (ev) => {
    markExited(ev.payload.id);
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

const pendingFrames = new Set<string>();
const forcedFrames = new Set<string>();
let frameTick = 0;
let layoutTick = 0;

function scheduleFrame(sessionId: string, force = false) {
  pendingFrames.add(sessionId);
  if (force) forcedFrames.add(sessionId);
  if (frameTick) return;
  frameTick = requestAnimationFrame(flushFrames);
}

async function flushFrames() {
  frameTick = 0;
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

async function paintFrame(sessionId: string, force = false) {
  const pane = state.panes.find((p) => p.session?.id === sessionId);
  if (!pane || pane.exited) return;
  const raw = toBytes(await invoke("session_frame", { id: sessionId, force }).catch(() => new Uint8Array()));
  if (raw.byteLength < 16) return;
  const view = new DataView(raw.buffer, raw.byteOffset, raw.byteLength);
  const width = view.getUint32(0, true);
  const height = view.getUint32(4, true);
  const nextW = view.getUint32(8, true) || pane.cellW;
  const nextH = view.getUint32(12, true) || pane.cellH;
  const cellChanged = nextW !== pane.cellW || nextH !== pane.cellH;
  pane.cellW = nextW;
  pane.cellH = nextH;
  pane.rasterScale = displayScale();
  if (!width || !height) return;
  const pixels = raw.subarray(16);
  if (pixels.byteLength < width * height * 4) return;
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
  const gen = pane.paintGen;
  try {
    const bitmap = await createImageBitmap(image);
    if (gen !== pane.paintGen) {
      bitmap.close();
      return;
    }
    pane.ctx.setTransform(1, 0, 0, 1, 0, 0);
    pane.ctx.imageSmoothingEnabled = false;
    pane.ctx.drawImage(bitmap, 0, 0);
    bitmap.close();
  } catch {
    if (gen !== pane.paintGen) return;
    pane.ctx.putImageData(image, 0, 0);
  }
  if (cellChanged) scheduleLayout();
}

function applyAppearance() {
  const a = state.appearance;
  if (!a) return;
  const theme = state.themes.find((t) => t.id === a.theme_id) ?? state.themes[0];
  if (theme) applyChrome(theme);
  document.documentElement.style.setProperty("--font-mono", a.font_family);
  $("status-theme").textContent = theme?.name ?? a.theme_id;
  state.customCss.textContent = a.custom_css;
  scheduleLayout();
  for (const pane of state.panes) {
    if (pane.session) scheduleFrame(pane.session.id, true);
  }
}

async function refreshSide() {
  const [hosts, hostsRuntime, groups, identities, snippets, history] = await Promise.all([
    invoke<Host[]>("hosts_list"),
    invoke<HostRuntime[]>("hosts_runtime").catch(() => [] as HostRuntime[]),
    invoke<Group[]>("groups_list").catch(() => [] as Group[]),
    invoke<Identity[]>("identities_list"),
    invoke<Snippet[]>("snippets_list"),
    invoke<HistoryEntry[]>("history_search", { query: "", limit: 80 }),
  ]);
  state.hosts = hosts;
  state.hostsRuntime = hostsRuntime;
  state.groups = groups;
  state.identities = identities;
  state.snippets = snippets;
  state.history = history;
  renderHosts();
  renderSnippets();
  renderHistory();
}

async function refreshSync() {
  const status = await invoke<SyncStatus>("sync_status");
  const state = status.state ?? (status.configured ? (status.last_error ? "error" : "idle") : "unconfigured");
  
  const stateConfig: Record<string, { label: string; icon: string; color: string }> = {
    unconfigured: { label: "Sync non configuré", icon: icons.cloud, color: "var(--tertiary)" },
    idle: { label: "À jour", icon: icons.cloud, color: "var(--green)" },
    syncing: { label: "Synchronisation...", icon: icons.cloud, color: "var(--blue)" },
    offline: { label: "Hors ligne", icon: icons.cloud, color: "var(--yellow)" },
    error: { label: "Erreur de sync", icon: icons.cloud, color: "var(--red)" },
  };
  
  const config = stateConfig[state] ?? stateConfig.idle;
  $("status-sync").innerHTML = `<span style="color: ${config.color};">${config.icon}</span><span>${config.label}</span>`;
  $("status-sync").setAttribute("data-testid", "sync-badge");
  $("status-sync").setAttribute("data-state", state);
  $("status-sync").style.color = config.color;
}

function hostPanes(hostId?: string | null) {
  return state.panes.filter((p) => (hostId ? p.session?.host_id === hostId || p.pending?.hostId === hostId : p.session?.kind === "local" || p.pending?.kind === "local"));
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
    const openCount = runtime?.open_count ?? 0;
    const isActive = hostPanes(h.id).some((p) => p.id === state.activePane);
    
    const connectionDot = (conn: string): string => {
      const colors: Record<string, string> = {
        local: "var(--blue)",
        connected: "var(--green)",
        disconnected: "rgba(235, 235, 245, 0.3)",
        connecting: "var(--yellow)",
        error: "var(--red)",
      };
      const color = colors[conn] ?? colors.disconnected;
      return `<span class="connection-dot" style="background: ${color}; box-shadow: 0 0 0 3px color-mix(in srgb, ${color} 22%, transparent);" data-testid="connection-dot" data-state="${conn}"></span>`;
    };
    
    return `<div class="item ${openCount > 0 ? "open" : ""} ${isActive ? "active-host" : ""}" data-host="${h.id}" data-testid="host-${h.id}" title="${escapeHtml(h.name || h.hostname)} — ${escapeHtml(h.username)}@${escapeHtml(h.hostname)}${h.port !== 22 ? `:${h.port}` : ""}">
        <span class="leading">${icons.server}</span>
        <div class="body"><strong>${escapeHtml(h.name || h.hostname)}</strong><small>${escapeHtml(h.username)}@${escapeHtml(h.hostname)}${h.port !== 22 ? `:${h.port}` : ""}</small></div>
        <span class="trail">
          ${connectionDot(connection)}
          ${openCount > 0 ? `<span class="sess-count" data-focus="${h.id}" data-testid="open-count-pill">${openCount}</span>` : ""}
          <button type="button" class="quick" data-new="${h.id}" title="New session">${icons.plus}</button>
          ${h.auth_method === "password" ? icons.password : icons.key}
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
        ${totalHosts > 0 ? `<small>${totalHosts}</small>` : ""}
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
  const localConnectionDot = `<span class="connection-dot" style="background: var(--blue); box-shadow: 0 0 0 3px color-mix(in srgb, var(--blue) 22%, transparent);" data-testid="connection-dot" data-state="local"></span>`;
  let panelHtml = `<div class="item pinned ${localOpen ? "open" : ""} ${active?.session?.kind === "local" || active?.pending?.kind === "local" ? "active-host" : ""}" data-local="1" data-testid="host-local">
      <span class="leading">${icons.laptop}</span>
      <div class="body"><strong>This computer</strong><small>${localOpen ? `${localOpen} open shell${localOpen > 1 ? "s" : ""}` : "Local shell"}</small></div>
      <span class="trail">
        ${localConnectionDot}
        ${localOpen ? `<span class="sess-count" data-testid="open-count-pill">${localOpen}</span>` : ""}
        <button type="button" class="quick" data-new-local="1" title="New session">${icons.plus}</button>
      </span>
    </div>`;
  
  // Render root groups
  for (const group of rootGroups) {
    panelHtml += renderGroup(group);
  }
  
  // Render ungrouped hosts (formula: !deleted_at && !group_id)
  const ungroupedAll = state.hosts.filter((h) => !h.deleted_at && !h.group_id);
  const ungrouped = filteredHosts.filter((h) => !h.deleted_at && !h.group_id);
  if (ungrouped.length > 0) {
    const ungroupedExpanded = state.expandedGroups.has("__ungrouped__") || q.length > 0;
    panelHtml += `<div class="group-row ${ungroupedExpanded ? "expanded" : ""}" data-group="__ungrouped__" data-testid="group-ungrouped">
      <span class="chevron">${icons.chevronRight}</span>
      <span class="leading">${icons.server}</span>
      <div class="body">
        <strong data-testid="group-label">Ungrouped<span class="group-badge" data-testid="group-count">${ungroupedAll.length}</span></strong>
      </div>
    </div>`;
    
    if (ungroupedExpanded) {
      panelHtml += `<div class="group-children">`;
      for (const host of ungrouped) {
        panelHtml += hostRow(host);
      }
      panelHtml += `</div>`;
    }
  }
  
  $("panel-hosts").innerHTML = panelHtml;
  
  // Show empty state if no hosts
  if (!state.hosts.length) {
    $("panel-hosts").insertAdjacentHTML(
      "beforeend",
      `<div class="empty">${icons.server}<span>Add a host to connect over SSH.</span></div>`,
    );
  } else if (!filteredHosts.length) {
    $("panel-hosts").insertAdjacentHTML(
      "beforeend",
      `<div class="empty">${icons.search}<span>No hosts match that search.</span></div>`,
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
        { label: "Browse files", run: () => openSftpFor(host.id) },
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
  $("panel-snippets").innerHTML =
    `<div class="item" id="new-snippet"><span class="leading">${icons.plus}</span><div class="body"><strong>New snippet</strong><small>Insert text into the terminal</small></div></div>` +
    (state.snippets
      .map(
        (s) => `<div class="item" data-snip="${s.id}"><span class="leading">${icons.snippet}</span><div class="body"><strong>${escapeHtml(s.title)}</strong><small>${escapeHtml(s.content)}</small></div></div>`,
      )
      .join("") || `<div class="empty">${icons.snippet}<span>No snippets yet.</span></div>`);
  $("new-snippet").onclick = () => editSnippet();
  $("panel-snippets").querySelectorAll<HTMLElement>("[data-snip]").forEach((el) => {
    el.onclick = () => sendText(state.snippets.find((s) => s.id === el.dataset.snip)?.content ?? "");
  });
}

function renderHistory() {
  $("panel-history").innerHTML = state.history
    .map((h) => `<div class="item" data-hist="${h.id}"><span class="leading">${icons.clock}</span><div class="body"><strong>${escapeHtml(h.command)}</strong><small>${h.session_kind} · ${h.created_at.slice(11, 19)}</small></div></div>`)
    .join("") || `<div class="empty">${icons.clock}<span>Commands you run will appear here.</span></div>`;
  $("panel-history").querySelectorAll<HTMLElement>("[data-hist]").forEach((el) => {
    el.onclick = () => sendText((state.history.find((h) => h.id === el.dataset.hist)?.command ?? "") + "\r");
  });
}

function escapeHtml(value: string) {
  return value.replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c]!));
}

function bindUi() {
  $("btn-sidebar").innerHTML = icons.sidebar;
  $("btn-new-local").innerHTML = icons.plus;
  $("btn-palette").innerHTML = icons.search;
  $("btn-settings").innerHTML = icons.settings;
  $("search-ico").innerHTML = icons.search;
  $("palette-ico").innerHTML = icons.search;
  $("btn-new-host").innerHTML = `${icons.plus}<span>New host</span>`;
  $("btn-new-group").innerHTML = `${icons.folder}<span>New group</span>`;
  $("tabs-prev").innerHTML = icons.chevronLeft;
  $("tabs-next").innerHTML = icons.chevronRight;
  const navLabels: Record<string, [string, string]> = {
    hosts: ["Hosts", "Search hosts..."],
    snippets: ["Snips", "Search snippets..."],
    history: ["History", "Search history..."],
    sftp: ["Files", "Search files..."],
  };
  const navIcons: Record<string, string> = {
    hosts: icons.server,
    snippets: icons.snippet,
    history: icons.clock,
    sftp: icons.folder,
  };
  document.querySelectorAll<HTMLButtonElement>(".side-nav button").forEach((btn) => {
    const panel = btn.dataset.panel ?? "";
    const [label] = navLabels[panel] ?? [panel, "Search..."];
    const icon = navIcons[panel] ?? "";
    btn.innerHTML = `${icon}<span>${label}</span>`;
    btn.setAttribute("aria-label", label);
    btn.onclick = () => {
      document.querySelectorAll(".side-nav button").forEach((b) => b.classList.remove("active"));
      btn.classList.add("active");
      document.querySelectorAll(".side-panel").forEach((p) => p.classList.add("hidden"));
      $(`panel-${btn.dataset.panel}`).classList.remove("hidden");
      const hint = navLabels[btn.dataset.panel ?? ""]?.[1];
      if (hint) ($("host-filter") as HTMLInputElement).placeholder = hint;
    };
  });
  $("host-filter").oninput = () => renderHosts();
  $("btn-new-host").onclick = () => editHost();
  $("btn-new-group").onclick = () => editGroup();
  $("btn-new-local").onclick = () => openLocal();
  $("btn-settings").onclick = () => openSettings();
  $("btn-palette").onclick = () => togglePalette();
  $("btn-sidebar").onclick = () => toggleSidebar();
  $("btn-sidebar").setAttribute("aria-expanded", "false");
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
    if (ev.target === $("modal")) $("modal").classList.add("hidden");
  };
  window.addEventListener("resize", () => scheduleLayout());
  window.visualViewport?.addEventListener("resize", () => scheduleLayout());
  window.addEventListener("keydown", onGlobalKey, true);
}

function closeOverlays() {
  $("modal").classList.add("hidden");
  $("palette").classList.add("hidden");
  hideMenu();
}

function toggleSidebar(force?: boolean) {
  if (force === true) $("app").classList.add("sidebar-open");
  else if (force === false) $("app").classList.remove("sidebar-open");
  else $("app").classList.toggle("sidebar-open");
  $("btn-sidebar").setAttribute("aria-expanded", $("app").classList.contains("sidebar-open") ? "true" : "false");
  scheduleLayout();
}

function fitWorkspace() {
  scheduleLayout();
}

function onGlobalKey(ev: KeyboardEvent) {
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
  const pane = reuse ?? createPendingPane("This computer", "local");
  try {
    const size = paneSize(pane);
    const info = await invoke<SessionInfo>("session_open_local", {
      cols: size.cols,
      rows: size.rows,
      scale: displayScale(),
    });
    attachSession(info, pane);
  } catch (err) {
    failPane(pane, "Couldn't open a local shell", String(err));
  }
}

async function openSsh(hostId: string, reuse?: Pane) {
  const host = state.hosts.find((h) => h.id === hostId);
  const pane = reuse ?? createPendingPane(host?.name || host?.hostname || "SSH", "ssh", hostId);
  try {
    const size = paneSize(pane);
    const info = await invoke<SessionInfo>("session_open_ssh", {
      hostId,
      cols: size.cols,
      rows: size.rows,
      scale: displayScale(),
    });
    attachSession(info, pane);
  } catch (err) {
    failPane(pane, `Couldn't reach ${host?.name || host?.hostname || "host"}`, String(err));
    openSheet(`<h2>SSH failed</h2><p class="form-error">${escapeHtml(String(err))}</p><div class="row"><button class="primary" id="ssh-fail-ok">Close</button></div>`);
    $("ssh-fail-ok").onclick = () => $("modal").classList.add("hidden");
  }
}

function attachSession(info: SessionInfo, pane = createPane()) {
  pane.session = info;
  pane.pending = undefined;
  pane.exited = false;
  pane.el.tabIndex = 0;
  hideBanner(pane);
  pane.el.onkeydown = (ev) => {
    if (!$("modal").classList.contains("hidden") || !$("palette").classList.contains("hidden")) return;
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

function createPane(): Pane {
  const el = document.createElement("div");
  el.className = "pane";
  const canvas = document.createElement("canvas");
  canvas.className = "term-canvas";
  const banner = document.createElement("div");
  banner.className = "pane-banner hidden";
  el.append(canvas, banner);
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
  };
  state.panes.push(pane);
  el.onclick = () => {
    selectPane(pane.id);
    el.focus();
  };
  renderTabs();
  return pane;
}

function createPendingPane(title: string, kind: string, hostId?: string): Pane {
  const pane = createPane();
  pane.pending = { title, kind, hostId };
  showBanner(pane, kind === "ssh" ? `Connecting to ${title}…` : "Opening local shell…");
  selectPane(pane.id);
  renderHosts();
  return pane;
}

function selectPane(id: string) {
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
  const pad = state.appearance?.padding ?? 8;
  const workspace = $("workspace");
  const width = pane.el.clientWidth || workspace.clientWidth;
  const height = pane.el.clientHeight || workspace.clientHeight;
  const innerW = Math.max(0, width - pad * 2);
  const innerH = Math.max(0, height - pad * 2);
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
  const pad = state.appearance?.padding ?? 8;
  pane.el.style.padding = `${pad}px`;
  pane.el.style.opacity = String(state.appearance?.opacity ?? 1);
  if (!pane.session || pane.exited) return;
  const size = paneSize(pane);
  if (size.tooSmall) return;
  const scale = displayScale();
  if (pane.cols === size.cols && pane.rows === size.rows && Math.abs(pane.rasterScale - scale) < 0.001) {
    return;
  }
  pane.cols = size.cols;
  pane.rows = size.rows;
  pane.rasterScale = scale;
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

function renderTabs() {
  const root = $("tabs");
  const ids = state.panes.map((p) => p.id);
  const existing = [...root.querySelectorAll<HTMLElement>("[data-tab]")];
  const sameOrder = existing.length === ids.length && existing.every((el, i) => el.dataset.tab === ids[i]);
  if (!sameOrder) {
    root.innerHTML = state.panes
      .map((p) => {
        const ssh = p.session?.kind === "ssh" || p.pending?.kind === "ssh";
        return `<button type="button" role="tab" data-tab="${p.id}"><span class="tab-ico">${ssh ? icons.server : icons.laptop}</span><span class="live-dot"></span><span class="label"></span><span class="x" data-close="${p.id}">${icons.close}</span></button>`;
      })
      .join("");
    root.querySelectorAll<HTMLElement>("[data-tab]").forEach((el) => {
      el.draggable = true;
      el.onclick = (ev) => {
        const close = (ev.target as HTMLElement).closest("[data-close]") as HTMLElement | null;
        if (close) {
          ev.stopPropagation();
          closePane(close.dataset.close!);
        } else {
          selectPane(el.dataset.tab!);
        }
      };
      el.onauxclick = (ev) => {
        if (ev.button !== 1) return;
        ev.preventDefault();
        closePane(el.dataset.tab!);
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
          { label: "Close", run: () => closePane(pane.id) },
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
  if (pane.session) await invoke("session_close", { id: pane.session.id }).catch(() => undefined);
  pane.el.remove();
  const idx = state.panes.findIndex((p) => p.id === id);
  state.panes = state.panes.filter((p) => p.id !== id);
  if (state.activePane === id) {
    const next = state.panes[idx] ?? state.panes[idx - 1] ?? state.panes[0];
    state.activePane = next?.id ?? null;
    if (next) selectPane(next.id);
  }
  renderTabs();
  renderHosts();
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
  if (state.activePane) closePane(state.activePane);
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
  if (pane.session) await invoke("session_close", { id: pane.session.id }).catch(() => undefined);
  pane.session = undefined;
  pane.exited = false;
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
      const ico = i.hint === "session" ? icons.laptop : i.hint === "app" ? icons.settings : i.hint === "cloud" ? icons.cloud : i.label.startsWith("Snippet") ? icons.snippet : icons.server;
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
    host.name = ($("f-name") as HTMLInputElement).value;
    host.hostname = ($("f-host") as HTMLInputElement).value;
    host.port = Number(($("f-port") as HTMLInputElement).value);
    host.username = ($("f-user") as HTMLInputElement).value;
    host.auth_method = authValue();
    host.password = ($("f-pass") as HTMLInputElement).value;
    host.notes = ($("f-notes") as HTMLTextAreaElement).value;
    host.group_id = ($("f-group") as HTMLSelectElement).value || null;
    host.updated_at = new Date().toISOString();
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
      <label class="cell"><span>Sync secrets<small class="hint">Passwords and keys</small></span>
        <span class="toggle"><input id="sync-secrets" type="checkbox" /><span class="track"></span></span>
      </label>
    </div>
    <div class="row">
      <button id="sync-now">Sync now</button>
      <button class="primary" id="a-save">Save</button>
    </div>
    <div class="meta" id="sync-msg">${status.last_error ?? ""}</div>`);
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
    appearance.renderer = "auto";
    appearance.font_family = ($("a-font") as HTMLInputElement).value;
    appearance.font_size = Number(($("a-size") as HTMLInputElement).value);
    appearance.line_height = Number(($("a-lh") as HTMLInputElement).value);
    appearance.scrollback = Number(($("a-scroll") as HTMLInputElement).value);
    appearance.padding = Number(($("a-pad") as HTMLInputElement).value);
    appearance.opacity = Number(($("a-op") as HTMLInputElement).value);
    appearance.custom_css = ($("a-css") as HTMLTextAreaElement).value;
    await invoke("appearance_set", { appearance });
    const url = ($("sync-url") as HTMLInputElement).value.trim();
    if (url) {
      try {
        await invoke("sync_configure", {
          config: { url, sync_secrets: ($("sync-secrets") as HTMLInputElement).checked },
        });
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

function openSheet(html: string) {
  $("modal-sheet").innerHTML = `<button type="button" class="sheet-close" id="sheet-close" title="Close">${icons.close}</button>${html}`;
  $("modal").classList.remove("hidden");
  $("sheet-close").onclick = () => $("modal").classList.add("hidden");
}

function openSftpFor(hostId: string) {
  state.sftpHostId = hostId;
  document.querySelectorAll(".side-nav button").forEach((b) => b.classList.remove("active"));
  document.querySelector<HTMLButtonElement>('[data-panel="sftp"]')?.classList.add("active");
  document.querySelectorAll(".side-panel").forEach((p) => p.classList.add("hidden"));
  $("panel-sftp").classList.remove("hidden");
  toggleSidebar(true);
  void loadSftp(hostId, ".");
}

async function loadSftp(hostId: string, path: string) {
  state.sftpHostId = hostId;
  const picker = `<div class="sftp-picker"><select id="sftp-host">${state.hosts
    .map(
      (h) =>
        `<option value="${escapeHtml(h.id)}" ${h.id === hostId ? "selected" : ""}>${escapeHtml(h.name || h.hostname)}</option>`,
    )
    .join("")}</select></div>`;
  if (!state.hosts.length) {
    $("panel-sftp").innerHTML = `<div class="empty">${icons.folder}<span>Add a host to browse files.</span></div>`;
    return;
  }
  $("panel-sftp").innerHTML = `${picker}<div class="empty">${icons.folder}<span>Loading ${escapeHtml(path)}…</span></div>`;
  $("sftp-host").onchange = () => loadSftp(($("sftp-host") as HTMLSelectElement).value, ".");
  try {
    const entries = await invoke<SftpEntry[]>("sftp_list", { hostId, path });
    const parent = path === "." || path === "/" ? "" : path.replace(/\/?[^/]+\/?$/, "") || ".";
    $("panel-sftp").innerHTML =
      picker +
      `<div class="item" data-sftp="${escapeHtml(parent || ".")}" data-dir="true"><span class="leading">${icons.folder}</span><div class="body"><strong>${escapeHtml(path)}</strong><small>${parent ? "Go up" : "Current folder"}</small></div></div>` +
      entries
        .map(
          (e) =>
            `<div class="item" data-sftp="${escapeHtml(e.path)}" data-dir="${e.is_dir}"><span class="leading">${e.is_dir ? icons.folder : icons.file}</span><div class="body"><strong>${escapeHtml(e.name)}</strong><small>${e.is_dir ? "Folder" : `${e.size} B`}</small></div></div>`,
        )
        .join("");
    $("sftp-host").onchange = () => loadSftp(($("sftp-host") as HTMLSelectElement).value, ".");
    $("panel-sftp").querySelectorAll<HTMLElement>("[data-sftp]").forEach((el) => {
      el.onclick = () => {
        if (el.dataset.dir === "true") loadSftp(hostId, el.dataset.sftp!);
      };
    });
  } catch (err) {
    $("panel-sftp").innerHTML = `${picker}<div class="empty">${icons.folder}<span>${escapeHtml(String(err))}</span></div>`;
    $("sftp-host").onchange = () => loadSftp(($("sftp-host") as HTMLSelectElement).value, ".");
  }
}

document.querySelector('[data-panel="sftp"]')?.addEventListener("click", () => {
  const activeHost = activePane()?.session?.host_id ?? activePane()?.pending?.hostId;
  const hostId = state.sftpHostId ?? activeHost ?? state.hosts[0]?.id;
  if (hostId) void loadSftp(hostId, ".");
  else $("panel-sftp").innerHTML = `<div class="empty">${icons.folder}<span>Add a host to browse files.</span></div>`;
});

boot().catch((err) => {
  console.error(err);
  openSheet(`<h2>Couldn't start</h2><p class="form-error">${escapeHtml(String(err))}</p>`);
});

// Initialize test bridge for E2E (dev/test/e2e only, not production)
if (import.meta.env.DEV || import.meta.env.MODE === 'test' || import.meta.env.VITE_E2E) {
  initTestBridge();
}
