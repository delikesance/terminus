# Per-host view scope and keyboard shortcuts overhaul

Date: 2026-10-13. Status: awaiting review.

## Goal

1. Each host remembers its own active view (Terminal / Files / Tunnels / Snippets / History) and Ctrl+Tab cycles only the sessions of the current host.
2. A coherent, platform-appropriate default shortcut set that reaches hosts, views and sessions from the keyboard, and a Settings page to remap the essential actions.

## What exists today (verified)

- Files, Snippets, History and Tunnels data are already per host: `screen/workspace/mod.rs` follows the selected machine (`sftp_parked.follow`, `snippets::visible_for(items, machine.id)`, `sync_history_view`, `sync_tunnels`).
- What is global: `Workspace.view` (`terminus-ui/src/shell/workspace.rs`) is a single value for the window, and `Screen::open_host_session` (`screen/sessions/open.rs:118`) forces Terminal on every host pick (sidebar row, palette).
- Sessions: `ContextManager` holds one flat list of tabs; each pane carries `host_id`. `switch_to_next/prev` walk the whole list, so Ctrl+Tab jumps across hosts. Pills already show only the current host's tabs, and `host_tab_to_restore` already picks a host's last tab.
- Bindings: `frontends/rioterm/src/bindings/mod.rs` (`default_key_bindings`, per-OS `platform_key_bindings`, `config_key_bindings` merges `[bindings]` from `config.toml`). Linux/Windows have no defaults for `SelectTab(n)`; macOS uses Cmd.
- Settings pages: `SettingsPage` (`General`, `Keys` = SSH keys, `Sync`, `Appearance`, `Updates`, `Uploads`); config writes go through `views/settings/config_edit.rs::set_value`.

## Part A: per-host scope

### View memory

`Workspace` gains `per_host: HashMap<String, WorkspaceView>`, keyed by sidebar row id (`local`, `wsl:<distro>`, host uuid).

- `leave_host(host_id)`: stores the current machine view for that host.
- `enter_host(host_id)`: shows the stored view (default Terminal); replaces `machine_selected`.
- Settings and Home stay window-global; `back_to_machine` returns to the current host's view.
- `open_host_session` calls `enter_host` instead of forcing Terminal. When the call creates a new session (host had no tab) the view becomes Terminal, since the user asked for a shell. Reusing an existing tab restores the remembered view.
- Memory lasts for the process lifetime only; no persistence.
- The module doc ("the view is global to the window") is rewritten to match.

### Session cycling

- `ContextManager::switch_to_next/prev` cycle only among tabs whose `tab_host_id` equals the current tab's host, wrapping inside that subset; no-op when the host has one tab.
- `SelectTab(n)` and `SelectLastTab` index into the current host's tabs, matching the pills.
- Host switching is a separate action (Part B) using `host_tab_to_restore`.

## Part B: shortcuts

### New actions

Added to `Action` (`bindings/mod.rs`), to the `From<String>` parser (usable in `config.toml`) and to the command palette (`renderer/command_palette/action.rs`):

- `ShowView(Terminal | Files | Tunnels | Snippets | History)` calls `Screen::show_view`.
- `SelectNextHost` / `SelectPrevHost` follow sidebar order and bring forward `host_tab_to_restore`, restoring that host's remembered view (Part A). A host without an open tab is skipped.

### Defaults

One table per platform in `bindings/`; Linux and Windows no longer rely on Super.

| Action | Linux / Windows | macOS |
|---|---|---|
| View 1..5 | Alt+1..5 | Cmd+Alt+1..5 |
| Previous / next host | Ctrl+Alt+Up / Down | Cmd+Alt+Up / Down |
| Next / previous session (current host) | Ctrl+Tab / Ctrl+Shift+Tab | same |
| New / close session | Ctrl+Shift+T / W | Cmd+T / W |
| Palette, Settings, search, splits, zoom, copy/paste | unchanged | unchanged |

Known trade-offs: Alt+digit is also used by readline/tmux/irssi in the PTY, and Ctrl+Alt+arrows by some desktop environments (GNOME workspaces). App bindings take precedence over the PTY like the others; both are remappable.

### Settings > Shortcuts

New `SettingsPage::Shortcuts` (the existing `Keys` page keeps the label "SSH keys").

- Fixed catalogue of about 20 actions in `terminus-ui` (id, label, group), no GPU dependency. The shown combination is read from the live bindings (`shortcut_label`).
- Click a row to enter capture mode. Esc cancels, Backspace resets to the platform default.
- Conflict: a combination already bound to another action marks the row with the conflicting action and is not saved.
- Rejected at capture: modifier-only combinations and bare Ctrl+C/Ctrl+V-style combinations that belong to the PTY.
- Save writes the entry to `[bindings]` in `config.toml` via `config_edit::set_value`; the existing hot reload applies it. Reset removes the entry so the platform default applies again.

## Out of scope

Persisting the per-host view across restarts, remapping VI-mode/scroll/hint bindings, a separate bindings file.

## Testing (red then green)

- `Workspace`: A to B to A restores each host's view; Settings/Home round-trip returns to the current host's view.
- `ContextManager`: tabs [hostA, hostB, hostA]: next from the first lands on the third and never on hostB; wrap; single-tab no-op; `SelectTab(n)` host-relative.
- Defaults: per-platform table has no duplicate combinations; `ShowView` covers all five views; `config.toml` string to `Action` round-trip.
- Shortcuts page (`terminus-ui`, no GPU): capture, conflict, rejected combos, reset; `config_edit` write/remove round-trip.
- `screen/sessions/tests`: host pick restores the remembered view; new session lands on Terminal.

## Files expected to change

`crates/terminus-ui/src/shell/workspace.rs`, `crates/terminus-ui/src/views/settings/` (new Shortcuts view, split so no file passes 300 lines), `crates/terminus-ui/src/shell/mod.rs`; `frontends/rioterm/src/context/mod.rs`, `bindings/mod.rs` (split the per-platform tables if it grows), `screen/keys.rs`, `screen/sessions/open.rs`, `screen/workspace/settings.rs`, `renderer/command_palette/action.rs`, a painter for the Shortcuts page under `renderer/`.
