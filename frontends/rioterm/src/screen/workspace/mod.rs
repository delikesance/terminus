//! `Screen` side of the workspace views: keeps the shell state (selected
//! machine, session pills, view data) in step with the tabs and the store,
//! routes view input, and carries out the actions views ask for.
//!
//! Each view's actions are handled in its own file here
//! (`workspace/<view>.rs`), so the agents filling the views never edit this one.

mod files;
mod history;
mod home;
mod settings;
mod snippets;
mod tunnels;

use super::Screen;
use crate::hosts;
use rio_backend::clipboard::Clipboard;
use terminus_ui::screens::{ViewAction, ViewInput, ViewOutcome};
use terminus_ui::shell::pills::{remote_session_label, session_label};
use terminus_ui::shell::{MachineInfo, SessionPill, WorkspaceView};

impl Screen<'_> {
    /// Refresh the shell from the tabs and the store. Cheap; called every
    /// frame before painting.
    pub(crate) fn sync_shell(&mut self) {
        let (w, h) = self.chrome_viewport();
        self.chrome.set_window_size(w, h);

        let current = self.context_manager.current_index();
        let host_of = |screen: &mut Self, i: usize| {
            screen
                .context_manager
                .contexts_mut()
                .get(i)
                .and_then(|g| g.current().host_id.clone())
                .unwrap_or_else(|| hosts::LOCAL_ID.to_string())
        };
        let machine_id = host_of(self, current);
        let row = self
            .chrome
            .panel
            .rows
            .iter()
            .filter_map(terminus_ui::sidebar::Row::host)
            .find(|h| h.id == machine_id)
            .cloned();
        let machine = MachineInfo {
            id: machine_id.clone(),
            name: row
                .as_ref()
                .map(|r| r.name.clone())
                .unwrap_or_else(|| "This computer".to_string()),
            address: row.as_ref().map(|r| r.endpoint.clone()).unwrap_or_default(),
        };

        let mut pills = Vec::new();
        for i in 0..self.context_manager.len() {
            if host_of(self, i) != machine_id {
                continue;
            }
            let active = i == current;
            let new_output = !active
                && self.context_manager.contexts_mut().get(i).is_some_and(|g| {
                    g.current().terminal.lock().peek_damage_event().is_some()
                });
            let label = if machine_id == hosts::LOCAL_ID {
                session_label(
                    self.context_manager.custom_title(i),
                    self.context_manager.title(i).map(|t| t.content.as_str()),
                    &machine.name,
                    pills.len(),
                )
            } else {
                // Remote: only the title the remote shell set counts; the
                // tab's template title describes the local `ssh` process.
                let osc_title = self
                    .context_manager
                    .contexts_mut()
                    .get(i)
                    .map(|g| g.current().terminal.lock().title.clone());
                remote_session_label(
                    self.context_manager.custom_title(i),
                    osc_title.as_deref(),
                    &machine.name,
                    pills.len(),
                )
            };
            pills.push(SessionPill {
                tab_index: i,
                label,
                active,
                new_output,
                closable: !self.context_manager.is_pinned(i),
            });
        }

        let can_browse = row.as_ref().is_some_and(|r| r.stored);
        let shell = &mut self.chrome.shell;
        shell.pills = pills;
        shell.sync_ok = self.host_store.sync_connected();
        shell.machine = Some(machine.clone());

        // Files follows the selected machine: each one keeps its own
        // browser (parked ones keep running).
        if self.sftp_parked.follow(
            &mut self.sftp,
            crate::sftp_ui::ActiveSftp::owner,
            &machine.id,
        ) {
            self.settle_failed_files_browser();
            self.mark_dirty();
        }

        let s = &mut self.chrome.screens;
        s.files.set_machine(&machine.id, &machine.name, can_browse);
        s.files.session_open = self.sftp.is_some();
        if s.snippets.items != self.host_store.snippet_items {
            s.snippets.items = self.host_store.snippet_items.clone();
            s.snippets.hover = None;
            let content = self.chrome.shell.content_rect();
            s.snippets.scroll = s.snippets.scroll.min(s.snippets.max_scroll(content));
        }

        // Files on an SSH host opens the browser directly (mock); the
        // empty state stays for machines without SFTP and failures.
        let files_shown = self.chrome.shell.view() == WorkspaceView::Files;
        if self.chrome.screens.files.take_auto_open(files_shown) {
            self.open_files_browser();
        }

        self.sync_settings_view();
        self.sync_history_view(&machine.id);
        self.sync_tunnels(&machine, row.as_ref().is_some_and(|r| r.stored));
    }

    /// Whether a pointer event at `(x, y)` belongs to the view on screen
    /// (not the terminal, not the legacy SFTP pane bridged into Files).
    pub fn view_owns(&self, x: f32, y: f32) -> bool {
        self.chrome.shell.view_owns(x, y) && !self.sftp_bridged()
    }

    /// Pointer shape over the views whose hit tests need measured text
    /// (Settings, History) or live in the frontend (Tunnels). `None`
    /// leaves the decision to [`Self::chrome_cursor_at`].
    pub fn view_cursor_at(
        &mut self,
        x: f32,
        y: f32,
    ) -> Option<rio_window::window::CursorIcon> {
        if !self.view_owns(x, y)
            || self.chrome_overlay_dialog_open()
            || self.chrome.confirm.is_some()
            || self.chrome.context_menu.is_some()
        {
            return None;
        }
        let content = self.chrome.shell.content_rect();
        let cursor = match self.chrome.shell.view() {
            WorkspaceView::Settings(_) => crate::renderer::views::settings::cursor_at(
                &mut self.sugarloaf,
                content,
                &self.settings_view,
                x,
                y,
            ),
            WorkspaceView::History => crate::renderer::views::history::cursor_at(
                &mut self.sugarloaf,
                content,
                &self.history_view,
                x,
                y,
            ),
            WorkspaceView::Tunnels => self
                .tunnels
                .as_ref()
                .filter(|c| c.host_id().is_some())?
                .state()
                .cursor_at(content, x, y),
            _ => return None,
        };
        Some(match cursor {
            terminus_ui::ChromeCursor::Pointer => rio_window::window::CursorIcon::Pointer,
            terminus_ui::ChromeCursor::Text => rio_window::window::CursorIcon::Text,
            terminus_ui::ChromeCursor::Default => rio_window::window::CursorIcon::Default,
        })
    }

    /// Files shows the legacy SFTP pane while a session is open.
    pub(crate) fn sftp_bridged(&self) -> bool {
        self.sftp.is_some() && self.chrome.shell.view() == WorkspaceView::Files
    }

    /// Switch view (header tab, palette, keyboard). Returns whether it
    /// changed.
    pub fn show_view(&mut self, view: WorkspaceView) -> bool {
        let changed = self.chrome.show_view(view);
        if changed {
            self.clear_selection();
            self.mark_dirty();
        }
        changed
    }

    /// Route one input to the view. Returns whether to repaint.
    pub fn view_input(&mut self, input: ViewInput, clipboard: &mut Clipboard) -> bool {
        let view = self.chrome.shell.view();
        // Views whose input needs the frontend (text measure, processes).
        let frontend = match view {
            WorkspaceView::Settings(_) => {
                Some(self.settings_view_input(&input, clipboard))
            }
            WorkspaceView::History => Some(self.history_view_input(&input)),
            WorkspaceView::Tunnels => Some(self.tunnels_view_input(&input)),
            _ => None,
        };
        let outcome = match frontend {
            Some(ViewOutcome::Ignored)
                if view.is_machine_view()
                    && matches!(
                        input,
                        ViewInput::Key {
                            key: terminus_ui::screens::ViewKey::Escape,
                            ..
                        }
                    ) =>
            {
                // Esc a view ignores goes back to the terminal.
                if self.show_view(WorkspaceView::Terminal) {
                    ViewOutcome::Redraw
                } else {
                    ViewOutcome::Ignored
                }
            }
            Some(outcome) => outcome,
            None => self.chrome.view_input(&input),
        };
        match outcome {
            ViewOutcome::Ignored | ViewOutcome::Consumed => false,
            ViewOutcome::Redraw => {
                // UI-only change: the callers' `request_redraw` repaints
                // only a dirty context.
                self.mark_dirty();
                true
            }
            ViewOutcome::Action(action) => {
                self.apply_view_action(action, clipboard);
                true
            }
        }
    }

    /// Committed text (IME, dead keys) for the view on screen. Text never
    /// triggers an action that needs the clipboard. Returns whether to
    /// repaint.
    pub fn view_text(&mut self, text: &str) -> bool {
        if self.sftp_bridged() {
            return false;
        }
        let mut clipboard = rio_backend::clipboard::Clipboard::new_nop();
        let input = ViewInput::Key {
            key: terminus_ui::screens::ViewKey::Text(text.to_string()),
            mods: Default::default(),
        };
        self.view_input(input, &mut clipboard)
    }

    pub fn apply_view_action(&mut self, action: ViewAction, clipboard: &mut Clipboard) {
        match action {
            ViewAction::Files(a) => self.files_view_action(a, clipboard),
            ViewAction::Snippets(a) => self.snippets_view_action(a, clipboard),
            ViewAction::Home(a) => self.home_view_action(a, clipboard),
        }
        self.mark_dirty();
    }

    /// Keys go to the view, not the PTY: a non-terminal view is shown.
    /// (Files with an open SFTP session handles its keys first in
    /// `process_key_event`; what it leaves is dropped here, never typed
    /// into the terminal.)
    pub(crate) fn view_takes_keys(&self) -> bool {
        !self.chrome.shell.view().shows_terminal()
    }

    /// Decode a key press for the view and route it.
    pub(crate) fn view_key_input(
        &mut self,
        key: &rio_window::event::KeyEvent,
        clipboard: &mut Clipboard,
    ) {
        use rio_window::keyboard::{Key, NamedKey};
        use terminus_ui::screens::{ViewKey, ViewMods};
        if key.state != rio_window::event::ElementState::Pressed {
            return;
        }
        let m = self.modifiers.state();
        let mods = ViewMods {
            ctrl: m.control_key(),
            shift: m.shift_key(),
            alt: m.alt_key(),
            logo: m.super_key(),
        };
        // Ctrl K: the mock's palette shortcut, live while a view owns keys.
        if mods.ctrl
            && !mods.shift
            && !mods.alt
            && terminus_ui::shell::sidebar::ctrl_k_opens_palette(
                self.chrome.shell.view().shows_terminal(),
                cfg!(target_os = "macos"),
            )
            && matches!(key.logical_key.as_ref(), Key::Character(c) if c.eq_ignore_ascii_case("k"))
        {
            self.open_palette();
            return;
        }
        if self.sftp_bridged() {
            return;
        }
        let vk = match key.logical_key.as_ref() {
            Key::Named(NamedKey::Enter) => ViewKey::Enter,
            Key::Named(NamedKey::Escape) => ViewKey::Escape,
            Key::Named(NamedKey::Backspace) => ViewKey::Backspace,
            Key::Named(NamedKey::Delete) => ViewKey::Delete,
            Key::Named(NamedKey::Tab) => ViewKey::Tab,
            Key::Named(NamedKey::ArrowUp) => ViewKey::Up,
            Key::Named(NamedKey::ArrowDown) => ViewKey::Down,
            Key::Named(NamedKey::ArrowLeft) => ViewKey::Left,
            Key::Named(NamedKey::ArrowRight) => ViewKey::Right,
            Key::Named(NamedKey::Home) => ViewKey::Home,
            Key::Named(NamedKey::End) => ViewKey::End,
            Key::Named(NamedKey::PageUp) => ViewKey::PageUp,
            Key::Named(NamedKey::PageDown) => ViewKey::PageDown,
            Key::Named(NamedKey::F2) => ViewKey::F2,
            _ => match key.text.as_ref() {
                Some(t) if crate::renderer::is_printable_text(t) => {
                    ViewKey::Text(t.to_string())
                }
                _ => return,
            },
        };
        if self.view_input(ViewInput::Key { key: vk, mods }, clipboard) {
            self.mark_dirty();
        }
    }

    /// Open the command palette ("Search or run…", Ctrl K).
    pub fn open_palette(&mut self) {
        if !self.renderer.command_palette.is_enabled() {
            let hosts = self.palette_host_items();
            self.renderer.command_palette.set_hosts(hosts);
            let shortcuts = self.palette_shortcuts();
            self.renderer.command_palette.set_shortcuts(shortcuts);
            self.renderer.command_palette.set_enabled(true);
            self.mark_dirty();
        }
    }

    /// Open the palette on its server list.
    pub fn open_palette_hosts(&mut self) {
        self.open_palette();
        let hosts = self.palette_host_items();
        self.renderer.command_palette.enter_hosts_mode(hosts);
    }
}
