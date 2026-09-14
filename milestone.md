# Terminus on Rio: UI/UX & Sugarloaf Frontend Architecture & Milestone Plan

## Executive Summary
This document establishes the UI/UX architecture, GPU-accelerated rendering design, and end-to-end implementation milestone plan for **Terminus on Rio**.

Terminus transforms Rio from a minimalist GPU-accelerated terminal emulator into a world-class, high-performance SSH client, server manager, and SFTP workstation. By building directly atop Rio's **Sugarloaf** rendering engine (Vulkan, Metal, WebGPU) and **Taffy** flexbox layout system, Terminus provides rich GUI capabilities—Activity Bar, Host Tree Sidebar, Enhanced Command Palette, Modal Dialogs, and Dual-Pane SFTP—while preserving Rio's blistering 120+ FPS rendering and sub-millisecond input latency.

---

## 1. High-Level Architecture & Layout Pipeline

```
+---------------------------------------------------------------------------------------------------+
| Rio Window (winit / rio-window)                                                                  |
| +-----------------------------------------------------------------------------------------------+ |
| | Root Taffy Layout (Flex Direction: Row)                                                       | |
| | +--------------+ +--------------------+ +---------------------------------------------------+ | |
| | | Activity Bar | | Sidebar Viewport   | | Main Content Viewport                             | | |
| | | (Fixed 48px) | | (260px, Resizable) | | (Flex-Grow: 1.0)                                  | | |
| | |              | |                    | | +-----------------------------------------------+ | | |
| | | [Hosts]      | | [Search / Filter]  | | | Island Navigation Tab Bar (Top/Bottom)        | | | |
| | | [Tunnels]    | |                    | | +-----------------------------------------------+ | | |
| | | [SFTP]       | | ▼ Production (3)   | | | Terminal Split Grid / SFTP Dual-Pane View     | | | |
| | | [Snippets]   | |   • prod-db-01     | | |                                               | | | |
| | |              | |   • prod-web-01    | | |  [ Pane 0: PTY / SSH ] | [ Pane 1: PTY / SSH ]| | | |
| | | [Settings]   | |   • k8s-master     | | |                                               | | | |
| | | [Vault Lock] | | ▶ Staging (5)      | | |                                               | | | |
| | +--------------+ +--------------------+ +---------------------------------------------------+ | |
| +-----------------------------------------------------------------------------------------------+ |
|                                                                                                   |
| GPU Compositor / Sugarloaf Layering Hierarchy:                                                    |
| Layer 0: Background Clear / Image                                                                 |
| Layer 1: Terminal Grid Shaders (Vulkan / Metal / WebGPU Pipelines)                                |
| Layer 2: Inactive Split Dim Rects (Order = 3)                                                     |
| Layer 3: Island Tab Bar & Scrollbars (Order = 5..10)                                              |
| Layer 4: Activity Bar & Sidebar Tree UI (Order = 12..15)                                          |
| Layer 5: Modal Dialogs, TOFU Prompts, Vault Unlock & Command Palette (Order = 20..25)             |
| Layer 6: Tooltips, Context Menus & Drag-and-Drop Overlay (Order = 30)                             |
+---------------------------------------------------------------------------------------------------+
```

---

## 2. Deep-Dive Design Specifications

### 2.1 Activity Bar & Host Tree Sidebar

#### 2.1.1 Layout & Taffy Integration
The root layout in `frontends/rioterm/src/layout/mod.rs` is refactored from a single `ContextGrid` into a top-level horizontal split:
- **`ActivityBarNode`**: Static width `48.0` logical pixels, height `100%`.
- **`SidebarNode`**: Dynamic width (default `260.0` logical pixels, collapsible to `0.0`, min `180.0`, max `500.0`), height `100%`. Contains a draggable border handle (3px) on its right edge.
- **`ContentNode`**: `FlexGrow(1.0)`, hosting the existing `ContextGrid` (island tabs + terminal split panes).

```rust
pub enum ActivityView {
    Hosts,
    PortForwards,
    Sftp,
    Snippets,
    Settings,
}

pub struct ActivityBarState {
    pub active_view: Option<ActivityView>,
    pub hovered_view: Option<ActivityView>,
    pub is_vault_locked: bool,
    pub active_connections_count: usize,
    pub active_tunnels_count: usize,
    pub active_transfers_count: usize,
}

pub struct SidebarState {
    pub is_open: bool,
    pub width: f32,
    pub filter_query: String,
    pub filter_active: bool,
    pub scroll_offset: usize,
    pub selected_node_id: Option<String>,
    pub hovered_node_id: Option<String>,
    pub expanded_folders: rustc_hash::FxHashSet<String>,
    pub is_dragging_border: bool,
    pub is_dirty: bool,
}
```

#### 2.1.2 Rendering Architecture & 120+ FPS Zero-Regression Strategy
1. **Isolated Damage Regions**: The sidebar rendering does not dirty terminal grid buffers. When terminal output rushes in at 100k lines/s, `terminal.snapshot_visible` and `render_with_grids` execute only inside the `ContentNode` layout rect.
2. **Immediate Mode UI Batching**: The sidebar is drawn using Sugarloaf primitives:
   - Sidebar container: `sugarloaf.rect(None, x, y, width, height, theme.sidebar_bg, 0.0, ORDER_SIDEBAR_BG)` (Order 12).
   - Group headers: Fold arrow `▼`/`▶`, group name, host count badge.
   - Host row items: Protocol glyph (SSH/Telnet/Serial), Host label (bold 13px), address subtitle (11px dim), latency tag (`24ms` pill in muted green/yellow), status dot (Green = connected, Pulsating Amber = connecting, Gray = idle, Red = disconnected).
   - Hover highlight: `sugarloaf.rounded_rect(None, row_x, row_y, row_w, row_h, theme.hover_bg, 0.1, 4.0, ORDER_SIDEBAR_ITEM)` (Order 13).
3. **Viewport Scissoring & Virtualization**:
   - Total host tree may contain 1,000+ items across nested groups.
   - The renderer virtualizes the list: computes total content height, calculates visible slice `[start_idx..end_idx]` based on `scroll_offset` and `viewport_height / item_height` (item height = 36px), rendering only ~25-30 visible rows per frame.
   - Uses `scrollbar.rs` thumb calculation for smooth mousewheel and trackpad scrolling.

---

### 2.2 Enhanced Command Palette & Quick Switcher

Rio currently possesses a GPU command palette (`renderer/command_palette.rs`) supporting `PaletteMode::Commands` and `PaletteMode::Fonts`. Terminus elevates this into a unified spotlight launcher.

#### 2.2.1 Unified Mode Architecture

```rust
pub enum PaletteMode {
    Commands,
    Hosts,
    Snippets,
    History,
    Fonts(Vec<String>),
}

pub struct HostPaletteItem {
    pub id: String,
    pub label: String,
    pub host: String,
    pub user: String,
    pub port: u16,
    pub group: Option<String>,
    pub tags: Vec<String>,
    pub last_connected: Option<chrono::DateTime<chrono::Utc>>,
}

pub struct SnippetPaletteItem {
    pub id: String,
    pub title: String,
    pub command: String,
    pub tags: Vec<String>,
    pub description: Option<String>,
}
```

#### 2.2.2 Mode Triggers & UX Workflow
- **Default / `>`**: Command mode. Actions: "New Tab", "Split Horizontal", "Unlock Vault", "Open SFTP Session", "Edit Hosts", "Toggle Sidebar", "Reload Config".
- **`@` or `/` or Quick-Launch (`Ctrl/Cmd+P`)**: SSH Host search mode.
  - Subtitle displays `user@host:port (Group/Tag)`.
  - Actions:
    - `Enter`: Connect in current tab (or new tab if current is active).
    - `Alt+Enter` / `Option+Enter`: Connect in Split Right.
    - `Ctrl+Shift+Enter`: Connect in Split Down.
    - `Shift+Enter`: Open SFTP session directly for this host.
- **`$`**: Snippet search mode.
  - Previews snippet template.
  - `Enter`: Inserts snippet into current focused terminal PTY (with parameter modal if placeholders like `{{env}}` exist).
- **`?`**: Cross-session History search mode.
  - Fast search through command history logs.

#### 2.2.3 Sub-millisecond Fuzzy Matching
- Integration of `nucleo-matcher` or optimized Smith-Waterman matching algorithm.
- Matches across `label`, `host`, `user`, `group`, and `tags` with prefix score boost.
- Matching character indices are passed to Sugarloaf for bold accent highlighting in the rendered palette text.

---

### 2.3 High-Performance SFTP Dual-Pane GUI

The SFTP view can be opened as a dedicated Tab or as a side-by-side split alongside an active SSH session.

```
+---------------------------------------------------------------------------------------------------+
| SFTP Dual Pane - prod-web-01 [192.168.1.50]                                                      |
| +-----------------------------------------------+ +---------------------------------------------+ |
| | LOCAL: /home/user/projects/web                | | REMOTE: /var/www/html/app                   | |
| | [ .. ] [ New Folder ] [ Filter: ________ ]    | | [ .. ] [ New Folder ] [ Filter: ________ ]  | |
| +-----------------------------------------------+ +---------------------------------------------+ |
| | Name            | Size    | Mode   | Modified | | Name            | Size   | Mode   | Modified | |
| | [D] src/        | --      | 0755   | 10:45 AM | | [D] public/     | --     | 0755   | 09:12 AM | |
| | [D] dist/       | --      | 0755   | 11:20 AM | | [D] storage/    | --     | 0777   | 08:30 AM | |
| | [F] config.json | 2.4 KB  | 0644   | 10:15 AM | | [F] .env        | 1.1 KB | 0600   | 09:00 AM | |
| | [F] app.bundle  | 14.8 MB | 0755   | 11:20 AM | | [F] index.php   | 4.2 KB | 0644   | 09:10 AM | |
| | > [Selected]    |         |        |          | |                 |        |        |          | |
| +-----------------------------------------------+ +---------------------------------------------+ |
| | Transfer Queue (1 active, 2 queued):                                          [Speed: 45 MB/s]| |
| | Uploading app.bundle -> /var/www/html/app/app.bundle [=====================>--------] 72%     | |
+---------------------------------------------------------------------------------------------------+
```

#### 2.3.1 GUI Component Specifications
- **Header & Breadcrumbs**: Clickable path segments with hover pills; quick navigation dropdown.
- **File List View**:
  - Columnar grid rendered via Sugarloaf: Icon (folder, binary, code, config, archive, generic), Name, File Size (B/KB/MB/GB), Permissions (`drwxr-xr-x` / octal `0755`), Owner/Group, Modified Timestamp.
  - Sorting: Clickable column headers (Name, Size, Date) with sort order arrow `▲`/`▼`.
  - Multi-selection: `Ctrl/Cmd+Click`, `Shift+Click`, or Vim visual mode (`v` + `j`/`k`).
- **Drag-and-Drop & File Actions**:
  - Drag from Local pane to Remote pane (triggers SFTP upload).
  - Drag from Remote pane to Local pane (triggers SFTP download).
  - Drag files directly from OS file explorer into Remote pane.
  - Keyboard shortcuts: `F5` / `y` (Transfer/Copy), `F6` (Move), `F7` (New Folder), `F8` / `Del` (Delete), `F2` (Rename), `Enter` (Enter folder / Edit file), `Space` (Toggle selection).
- **Background Transfer Engine & Progress Drawer**:
  - Tokio background worker managing async SFTP channels (`russh-sftp` / `ssh2`).
  - Chunked streaming with backpressure control and SHA-256 integrity verification.
  - Transfer Drawer at bottom of view: Animated Sugarloaf progress bar (rounded quad with gradient fill), transfer rate (MB/s), ETA countdown, pause/cancel controls.

---

### 2.4 Modal Dialogs Architecture

Modals are managed by Rio's top-level router (`frontends/rioterm/src/router/mod.rs`) and rendered over the active window with depth layering (`ORDER = 20..25`).

#### 2.4.1 Modal State Machine

```rust
pub enum Modal {
    IslandRename,
    CommandPalette,
    ConfirmQuit,
    Assistant,
    // Terminus Core Modals:
    TofuHostKeyApproval(Box<TofuHostKeyPrompt>),
    VaultUnlock(Box<VaultUnlockPrompt>),
    HostEditor(Box<HostEditorState>),
    SnippetEditor(Box<SnippetEditorState>),
    PortForwardEditor(Box<PortForwardEditorState>),
}
```

#### 2.4.2 Modal Specifications

1. **TOFU SSH Host Key Approval Prompt (`Modal::TofuHostKeyApproval`)**:
   - **Trigger**: Initial connection to unverified host or fingerprint mismatch.
   - **UI**: Centered dialog (width: 520px) with backdrop blur.
   - **Elements**:
     - Status Header: Green shield ("New SSH Host Key") or High-Alert Red Banner ("WARNING: REMOTE HOST IDENTIFICATION HAS CHANGED!").
     - Connection Info: `user@hostname:port`.
     - Key Fingerprint: Key type (ED25519 / RSA / ECDSA), SHA-256 fingerprint hash.
     - Visual Key Host Art (SSH randomart ascii box).
     - Action Buttons: `[Trust & Save]` (Enter), `[Connect Once]`, `[Cancel & Disconnect]` (Esc).

2. **Vault Unlock Modal (`Modal::VaultUnlock`)**:
   - **Trigger**: Accessing password/key credentials when master vault is locked or session expires.
   - **UI**: Centered security dialog (width: 420px).
   - **Elements**:
     - Vault Icon + "Unlock Terminus Vault".
     - Masked Passphrase Input (renders bullet glyphs `••••••••`).
     - Biometric Unlock Button (Touch ID / Windows Hello / Secret Service PAM).
     - Auto-lock configuration preference ("Lock after 15 minutes idle").
     - Action Buttons: `[Unlock]` (Enter), `[Cancel]` (Esc).

3. **Host Add/Edit Modal (`Modal::HostEditor`)**:
   - **Trigger**: New host creation, editing existing host from sidebar/palette.
   - **UI**: Multi-tabbed configuration dialog (width: 680px, height: 560px).
   - **Tabs**:
     - *General*: Label, Group/Folder, Host address, Port (default: 22), Username, Tags.
     - *Authentication*: Method radio pills (Password, Private Key, SSH Agent, Certificate, PKCS#11, 1Password CLI), Key path picker, Passphrase.
     - *Proxy / Jump*: Direct, SSH Jump Host (`ProxyJump`), SOCKS5/HTTP Proxy.
     - *Port Forwards*: Inline table of local/remote port tunnels.
     - *Terminal*: Shell override, startup command/script, startup folder, custom theme override.
     - *Advanced*: Keepalive interval, TCP nodelay, Cipher list, X11 forwarding.
   - **Actions**: `[Test Connection]` (runs async handshake with latency tester), `[Save]`, `[Cancel]`.

---

### 2.5 Theme Synchronization: Terminus's 7 Built-In Themes

Terminus includes 7 carefully crafted themes that map cleanly into Rio's color engine (`rio-backend/src/config/colors/` and `rio-backend/src/config/theme/`).

#### 2.5.1 Theme Definitions & Palette Specifications

| Theme Name | Style / Tone | Background | Foreground | Accent / Active | Sidebar BG | Borders | Status Colors (OK/Warn/Err) |
|---|---|---|---|---|---|---|---|
| **1. Terminus (Default)** | Deep Modern Navy | `#0f172a` | `#f8fafc` | `#6366f1` / `#38bdf8` | `#0b1120` | `#1e293b` | `#10b981` / `#f59e0b` / `#ef4444` |
| **2. Graphite** | Elegant Dark Slate | `#1e1e24` | `#e0e0e6` | `#00e8c6` / `#708090` | `#17171c` | `#2a2a34` | `#00e8c6` / `#ffd166` / `#ff5c7c` |
| **3. Mocha** | Warm Espresso Dark | `#181412` | `#ece0d1` | `#e09f3e` / `#d4a373` | `#120f0d` | `#2c221e` | `#a7c957` / `#e09f3e` / `#bc4749` |
| **4. Obsidian** | Pitch Black AMOLED | `#050505` | `#e6edf3` | `#10b981` / `#58a6ff` | `#000000` | `#1a1a1a` | `#2ea043` / `#d29922` / `#f85149` |
| **5. Phosphor** | Retro Cyberpunk Green| `#0a0f0d` | `#33ff77` | `#00ff66` / `#22c55e` | `#050807` | `#14291e` | `#00ff66` / `#ffb703` / `#ff0055` |
| **6. Midnight** | Celestial Night Blue | `#0b0f19` | `#e2e8f0` | `#8b5cf6` / `#60a5fa` | `#070a11` | `#1e2538` | `#34d399` / `#fbbf24` / `#f87171` |
| **7. Paper** | High-Legibility Light| `#f8fafc` | `#0f172a` | `#2563eb` / `#0284c7` | `#f1f5f9` | `#cbd5e1` | `#16a34a` | `#d97706` | `#dc2626` |

#### 2.5.2 Extended Theme Token Architecture
We extend Rio's `Colors` struct with Terminus UI tokens while maintaining full backwards compatibility with standard Rio configuration files:

```rust
#[derive(Debug, Clone, Copy, Deserialize, PartialEq)]
pub struct TerminusUiColors {
    pub sidebar_background: [f32; 4],
    pub sidebar_foreground: [f32; 4],
    pub sidebar_border: [f32; 4],
    pub sidebar_item_hover: [f32; 4],
    pub sidebar_item_active: [f32; 4],
    pub activity_bar_background: [f32; 4],
    pub activity_bar_icon: [f32; 4],
    pub activity_bar_icon_active: [f32; 4],
    pub activity_bar_badge_bg: [f32; 4],
    pub activity_bar_badge_fg: [f32; 4],
    pub modal_backdrop: [f32; 4],
    pub modal_background: [f32; 4],
    pub modal_border: [f32; 4],
    pub modal_input_bg: [f32; 4],
    pub modal_input_border: [f32; 4],
    pub modal_input_focus: [f32; 4],
    pub status_connected: [f32; 4],
    pub status_connecting: [f32; 4],
    pub status_disconnected: [f32; 4],
    pub status_forwarding: [f32; 4],
    pub sftp_pane_background: [f32; 4],
    pub sftp_header_background: [f32; 4],
    pub sftp_row_hover: [f32; 4],
    pub sftp_row_selected: [f32; 4],
    pub sftp_progress_bar: [f32; 4],
}
```

#### 2.5.3 Hot-Reloading & Live Theme Switcher
- Instant live theme switching without dropping terminal sessions, resetting PTYs, or re-initializing the GPU device.
- Triggerable via Command Palette (`Toggle Appearance Theme` or `Set Theme: <Name>`), keyboard binding, or file watcher on `~/.config/terminus/config.toml`.

---

## 3. Step-by-Step Implementation Milestone Plan

### Milestone 1: Layout Engine Refactoring & UI Primitives Foundation
- **Goal**: Establish the extensible Taffy layout structure and Sugarloaf immediate-mode UI rendering primitives.
- **Tasks**:
  1. Refactor `frontends/rioterm/src/layout/mod.rs` to support root horizontal flex layout (`ActivityBar` + `Sidebar` + `ContentGrid`).
  2. Implement `PanelBorder` resizing logic for the sidebar with mouse cursor state management (`CursorIcon::ColResize`).
  3. Expand `sugarloaf` immediate-mode helpers: clipped rect rendering, badge drawing helper, text alignment utilities.
  4. Implement `TerminusUiColors` struct and theme token resolution in `rio-vt` / `rio-backend`.
- **Verification & Acceptance**:
  - Unit tests in `layout/compute_tests.rs` verifying Taffy node allocations on window resize and sidebar toggle.
  - Render test ensuring terminal grid size recomputes column/line metrics cleanly when sidebar opens/closes.

### Milestone 2: Activity Bar & Extensible Host Tree Sidebar
- **Goal**: Deliver a fully interactive, GPU-accelerated sidebar tree for SSH hosts, groups, and status badges.
- **Tasks**:
  1. Build `ActivityBar` renderer in `frontends/rioterm/src/renderer/activity_bar.rs` (Order 12).
  2. Build `HostTreeSidebar` renderer in `frontends/rioterm/src/renderer/sidebar.rs` (Order 12..14).
  3. Implement virtualized host tree list with group collapsing, search filtering, and latency badge rendering.
  4. Implement mouse hit-testing for tree items, expand/collapse carets, and right-click context menu.
  5. Connect sidebar clicks to connection dispatching in `context_manager`.
- **Verification & Acceptance**:
  - 1,000 mock hosts load in < 5ms with zero frame drop during scrolling.
  - Terminal grid continues rendering at 120 FPS during continuous PTY throughput with open sidebar.

### Milestone 3: Enhanced Command Palette & Launcher
- **Goal**: Expand Rio's command palette into a unified Spotlight launcher for Commands, Hosts, Snippets, and History.
- **Tasks**:
  1. Refactor `renderer/command_palette.rs` to support `PaletteMode::Hosts`, `PaletteMode::Snippets`, and `PaletteMode::History`.
  2. Integrate high-performance fuzzy matching with match index highlighting.
  3. Implement host quick-action shortcuts (`Enter`, `Alt+Enter`, `Shift+Enter`).
  4. Implement snippet search and direct PTY insertion pipeline.
  5. Add search mode prefix parsing (`>`, `@`, `$`, `?`).
- **Verification & Acceptance**:
  - Fuzzy searching over 5,000 hosts returns filtered results in < 1ms.
  - Selecting a host with `Alt+Enter` creates a vertical split and launches the SSH session.

### Milestone 4: Security Architecture & Modal Dialogs System
- **Goal**: Implement high-priority security prompts and configuration modals.
- **Tasks**:
  1. Implement modal state routing in `frontends/rioterm/src/router/mod.rs` with focus trapping and backdrop dimming.
  2. Implement **TOFU Host Key Approval Prompt** (`Modal::TofuHostKeyApproval`) with fingerprint display and MITM warning banner.
  3. Implement **Vault Unlock Modal** (`Modal::VaultUnlock`) with masked password input, auto-lock timer, and biometrics hook.
  4. Implement **Host Add/Edit Modal** (`Modal::HostEditor`) with tabbed forms, credential selector, jump host config, and live connection tester.
- **Verification & Acceptance**:
  - TOFU dialog intercepts new SSH connections and blocks handshake until user approval.
  - Locked vault blocks access to stored credentials until valid master password is typed.

### Milestone 5: SFTP Dual-Pane GUI & Async Transfer Engine
- **Goal**: Deliver a high-speed, dual-pane SFTP file manager with animated transfer progress.
- **Tasks**:
  1. Create `frontends/rioterm/src/renderer/sftp/` dual-pane layout renderer (local filesystem vs remote SFTP filesystem).
  2. Implement columnar file list with icon rendering, size/mode formatting, and column sorting.
  3. Implement keyboard navigation (Vim bindings, selection, copy, move, delete) and mouse drag-and-drop.
  4. Build Tokio async background transfer engine with chunked streaming and rate calculation.
  5. Build Sugarloaf animated transfer progress drawer with progress bars, speed, and ETA.
- **Verification & Acceptance**:
  - Bi-directional file transfer of 1GB file completes with accurate progress tracking, speed calculation, and zero UI stutter.
  - Drag-and-drop file transfer between local and remote panes functions seamlessly.

### Milestone 6: Theme Synchronization & 7 Built-In Themes
- **Goal**: Implement Terminus's 7 built-in themes and dynamic theme hot-reloading.
- **Tasks**:
  1. Encode the 7 built-in themes (Terminus, Graphite, Mocha, Obsidian, Phosphor, Midnight, Paper) in `rio-backend/src/config/theme/`.
  2. Wire extended UI theme colors into Sugarloaf renderer passes.
  3. Implement theme selection menu in Command Palette and config file watcher.
  4. Support adaptive light/dark mode auto-switching according to OS appearance.
- **Verification & Acceptance**:
  - Theme switching executes immediately across all tabs, splits, sidebars, and modals without artifacts.
  - Contrast ratios for all 7 themes meet WCAG AA standards.

### Milestone 7: Performance Benchmarking, Auditing & Release Polish
- **Goal**: Verify rendering performance, stability, and platform consistency.
- **Tasks**:
  1. Benchmark frame times under heavy terminal throughput (100k lines/s + active SFTP transfer + open sidebar) to guarantee consistent 120+ FPS.
  2. Audit memory usage under long-running SSH sessions and transfer queues.
  3. Cross-platform validation across Linux (Vulkan/X11/Wayland), macOS (Metal), and Windows (wgpu/DirectX 12).
  4. Package release binaries and complete end-user documentation.
- **Verification & Acceptance**:
  - Render frame times remain < 8.33ms (120 FPS budget) under load.
  - Zero memory leaks detected across 24-hour continuous burn-in test.

---

## 4. Architecture Verification Checklist

| Architectural Layer | Target Criterion | Verification Method | Status |
|---|---|---|---|
| **Taffy Flexbox Layout** | Resizable 3-pane root layout with zero sub-pixel fringes | `cargo test -p rioterm compute_tests` | Designed |
| **Sugarloaf Renderer** | 120+ FPS immediate UI rendering, zero PTY throughput regression | Micro-benchmarking with Tracy / Frame Profiler | Designed |
| **Host Tree Sidebar** | Virtualized 1,000+ host list rendering in < 8ms | UI frame stress test with mock hosts | Designed |
| **Command Palette** | Fuzzy search sub-1ms response over 5,000 items | Benchmark tests with `nucleo-matcher` | Designed |
| **Modal Router** | Strict focus trap & keyboard routing priority | Modal state machine unit tests | Designed |
| **SFTP Engine** | Async non-blocking file streaming with live progress UI | 1GB transfer benchmark with SHA-256 validation | Designed |
| **Themes** | 7 built-in themes with 100% token coverage & hot-reload | Dynamic theme switch test without PTY restarts | Designed |
