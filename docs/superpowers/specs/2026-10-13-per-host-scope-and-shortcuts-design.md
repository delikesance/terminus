# Per-host view scope and keyboard shortcuts overhaul

Date: 2026-10-13. Status: reviewed 2026-10-16 against the code, decisions below; ready to plan. Delivery is split R1a / R1b / R1c (see the end).

## Goal

1. Each host remembers its own active view (Terminal / Files / Tunnels / Snippets / History) and Ctrl+Tab cycles only the sessions of the current host.
2. A coherent, platform-appropriate default shortcut set that reaches hosts, views and sessions from the keyboard, and a Settings page to remap the essential actions.

## What exists today (verified)

- Files, Snippets, History and Tunnels data are already per host: `screen/workspace/mod.rs` follows the selected machine (`sftp_parked.follow`, `snippets::visible_for(items, machine.id)`, `sync_history_view`, `sync_tunnels`).
- What is global: `Workspace.view` (`terminus-ui/src/shell/workspace.rs`) is a single value for the window, and `Screen::open_host_session` (`screen/sessions/open.rs:118`) forces Terminal on every host pick (sidebar row, palette).
- Sessions: `ContextManager` holds one flat list of tabs; each pane carries `host_id`. `switch_to_next/prev` walk the whole list, so Ctrl+Tab jumps across hosts. Pills already show only the current host's tabs, and `host_tab_to_restore` already picks a host's last tab.
- Bindings: `frontends/rioterm/src/bindings/mod.rs` (`default_key_bindings`, per-OS `platform_key_bindings`, `config_key_bindings` merges `[bindings]` from `config.toml`). Linux/Windows have no defaults for `SelectTab(n)`; macOS uses Cmd+1..8. Linux/Windows already bind Ctrl+Shift+T/W, Ctrl+Tab / Ctrl+Shift+Tab, and Ctrl+Shift+[ / ] to `SelectPrevTab`/`SelectNextTab` **and** to `SelectPrevSplit`/`SelectNextSplit` (same combination, two actions: an existing duplicate). Ctrl+Shift+Up/Down and Ctrl+Alt+1..5 are unbound; only Ctrl+Shift+Alt+arrows (`MoveDivider*`) exist.
- Every tab change except closing goes through `switch_visible_context` (`screen/sessions/tabs.rs`), which ends in `sync_sidebar_selection`; `close_tab_at` calls `sync_sidebar_selection` directly. `Workspace::machine_selected` has no production caller (tests only); production code uses `Screen::show_view`.
- `ContextManager::switch_to_next/prev`, `select_tab` and `select_last_tab` start with an `is_native` branch (macOS native tabs) that sends a window event instead of moving `current_index`.
- Settings pages: `SettingsPage` (`General`, `Keys` = SSH keys, `Sync`, `Appearance`, `Updates`, `Uploads`); config writes go through `views/settings/config_edit.rs::set_value`.

## Part A: per-host scope

### View memory

`Workspace` gains `per_host: HashMap<String, WorkspaceView>`, keyed by sidebar row id (`local`, `wsl:<distro>`, host uuid).

- `leave_host(host_id)`: stores the current machine view for that host.
- `enter_host(host_id)`: shows the stored view (default Terminal); replaces `machine_selected`.
- Single sync point: whenever the current tab's host differs from the previous one (`switch_visible_context`, `close_tab_at`, `open_host_session`), `Screen` calls `leave_host(old)` then `enter_host(new)`. Switching between tabs of the same host never touches the view. Host identity is `host_id.unwrap_or(LOCAL_ID)`, as in `host_tab_to_restore`.
- Closing the last tab of a host drops its entry: the host falls back to the tab now in front (its own remembered view, Terminal by default). Deleting a host (`delete_host_closing_sessions`) drops its entry too.
- A host without an open tab can still be on Files/Tunnels/Snippets/History (picked via the sidebar, not through `open_host_session`); its entry stays until the next session open on that host, which lands on Terminal (new session rule above).
- Settings and Home stay window-global; `back_to_machine` returns to the current host's view.
- `open_host_session` calls `enter_host` instead of forcing Terminal. When the call creates a new session (host had no tab) the view becomes Terminal, since the user asked for a shell. Reusing an existing tab restores the remembered view.
- Memory lasts for the process lifetime only; no persistence.
- The module doc ("the view is global to the window") is rewritten to match.

### Session cycling

- `ContextManager::switch_to_next/prev` cycle only among tabs of the current tab's host (`tab_host_id` normalised: `None` = `LOCAL_ID`), wrapping inside that subset; no-op when the host has one tab. The `is_native` branch is kept unchanged.
- `SelectTab(n)` and `SelectLastTab` index into the current host's tabs, matching the pills (`n` past the end is a no-op). This is a behaviour change on macOS, where Cmd+1..8 index the whole list today; accepted, since the pills only show the current host's tabs.
- Host switching is a separate action (Part B) using `host_tab_to_restore`.

## Part B: shortcuts

### New actions

Added to `Action` (`bindings/mod.rs`), to the `From<String>` parser (usable in `config.toml`) and to the command palette (`renderer/command_palette/action.rs`):

- `ShowView(Terminal | Files | Tunnels | Snippets | History)` calls `Screen::show_view`.
- `SelectNextHost` / `SelectPrevHost` follow sidebar order and bring forward `host_tab_to_restore`, restoring that host's remembered view (Part A). A host without an open tab is skipped on purpose: the shortcut moves between running sessions; opening a host stays a sidebar/palette action. No-op when fewer than two hosts have a tab.

### Defaults

One table per platform in `bindings/`; Linux and Windows no longer rely on Super.

| Action | Linux / Windows | macOS |
|---|---|---|
| View 1..5 | Ctrl+Alt+1..5 | Cmd+Alt+1..5 |
| Previous / next host | Ctrl+Shift+Up / Down | Cmd+Alt+Up / Down |
| Next / previous session (current host) | Ctrl+Tab / Ctrl+Shift+Tab | same |
| New / close session | Ctrl+Shift+T / W (already bound) | Cmd+T / W (already bound) |
| Palette, Settings, search, splits, zoom, copy/paste | unchanged | unchanged |

Rationale (checked in `bindings/mod.rs`): Alt+digit and Ctrl+Alt+arrows are avoided because readline/tmux use the former and GNOME uses the latter for workspaces. Ctrl+Shift+Up/Down and Ctrl+Alt+1..5 are unbound today. Ctrl+Shift+Up/Down is a terminal-emulator convention for scrolling in some emulators; here it is an app binding like the others and is remappable. Not verified on a real GNOME/Windows desktop.

Prerequisite: the existing duplicate Ctrl+Shift+[ / ] (tab *and* split on Linux/Windows) is fixed first, by dropping the tab pair (Ctrl+Tab remains); the splits keep Ctrl+Shift+[ / ]. The "no duplicate combination" test enforces it.

### Settings > Shortcuts

New `SettingsPage::Shortcuts` (the existing `Keys` page keeps the label "SSH keys").

- Fixed catalogue of about 20 actions in `terminus-ui` (id, label, group), no GPU dependency. The shown combination is read from the live bindings (`shortcut_label`).
- Click a row to enter capture mode. Esc cancels, Backspace resets to the platform default.
- Conflict: a combination already bound to another action marks the row with the conflicting action and is not saved.
- Rejected at capture: modifier-only combinations; Esc and Backspace (reserved for cancel/reset); and, as a closed list, Ctrl+A, B, C, D, E, K, L, R, U, W, Z without Shift or another modifier (shell line editing and signals). Shifted/other-modified variants stay allowed.
- Capture mode swallows the key event before binding lookup, so the captured combination never triggers its current action.
- Save writes the entry to `[bindings]` in `config.toml` via `config_edit::set_value`; the existing hot reload applies it. Reset removes the entry so the platform default applies again.

## Out of scope

Persisting the per-host view across restarts, remapping VI-mode/scroll/hint bindings, a separate bindings file.

## Testing (red then green)

- `Workspace`: A to B to A restores each host's view; Settings/Home round-trip returns to the current host's view.
- `ContextManager`: tabs [hostA, hostB, hostA]: next from the first lands on the third and never on hostB; wrap; single-tab no-op; `SelectTab(n)` host-relative.
- Defaults: per-platform table has no duplicate combinations; `ShowView` covers all five views; `config.toml` string to `Action` round-trip.
- Shortcuts page (`terminus-ui`, no GPU): capture, conflict, rejected combos, reset; `config_edit` write/remove round-trip.
- `screen/sessions/tests`: host pick restores the remembered view; new session lands on Terminal.

## Delivery

- **R1a**: Part A (view memory, sync point) and per-host cyclage / `SelectTab`. `terminus-ui` + `context/mod.rs` + `screen/sessions/*`, unit tests only.
- **R1b**: `ShowView`, `SelectNextHost`/`SelectPrevHost`, defaults table, duplicate fix, palette entries.
- **R1c**: Settings > Shortcuts page (new painter, `config_edit`). Not blocking R1a/R1b.

Each step is its own commit; `milestone.md` (R1) is updated in the same commit as a status change.

## Files expected to change

`crates/terminus-ui/src/shell/workspace.rs`, `crates/terminus-ui/src/views/settings/` (new Shortcuts view, split so no file passes 300 lines), `crates/terminus-ui/src/shell/mod.rs`; `frontends/rioterm/src/context/mod.rs`, `bindings/mod.rs` (split the per-platform tables if it grows), `screen/keys.rs`, `screen/sessions/open.rs`, `screen/workspace/settings.rs`, `renderer/command_palette/action.rs`, a painter for the Shortcuts page under `renderer/`.
