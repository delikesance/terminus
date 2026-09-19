//! `Screen` sessions surface, split out of `screen/mod.rs`.

use super::Screen;
use crate::context;
use crate::context::next_rich_text_id;
use crate::crosswords::pos::Column;
use crate::hosts;
use crate::layout::ContextDimension;
use crate::renderer::island;
use crate::renderer::utils::padding_top_from_config;
use rio_backend::clipboard::Clipboard;
use rio_backend::config::layout::Margin;
use rio_backend::config::Shell;
use rio_backend::crosswords::pos::Line;
use rio_backend::event::EventProxy;
use terminus_ui::sidebar::Badge;

impl Screen<'_> {
    pub fn split_right_with_config(&mut self, config: rio_backend::config::Config) {
        // Allocate panel id; position lands on `ContextDimension`
        // through the Taffy layout pass (`apply_taffy_layout`).
        let _ = self.renderer.margin.top
            + self.renderer.island.as_ref().map_or(0.0, |i| i.height());
        let _ = config.margin.left;
        let rich_text_id = next_rich_text_id();
        self.context_manager.split_from_config(
            rich_text_id,
            false,
            config,
            &mut self.sugarloaf,
        );

        self.mark_dirty();
    }

    pub fn split_right(&mut self) {
        let rich_text_id = next_rich_text_id();
        self.context_manager
            .split(rich_text_id, false, &mut self.sugarloaf);

        self.mark_dirty();
    }

    pub fn split_down(&mut self) {
        let rich_text_id = next_rich_text_id();
        self.context_manager
            .split(rich_text_id, true, &mut self.sugarloaf);

        self.mark_dirty();
    }

    pub fn move_divider_up(&mut self) {
        let amount = 20.0; // Default movement amount
        if self
            .context_manager
            .move_divider_up(amount, &mut self.sugarloaf)
        {
            self.mark_dirty();
            // Divider displacement is layout, not cursor travel.
            self.renderer.trail_cursor.snap();
        }
    }

    pub fn move_divider_down(&mut self) {
        let amount = 20.0; // Default movement amount
        if self
            .context_manager
            .move_divider_down(amount, &mut self.sugarloaf)
        {
            self.mark_dirty();
            // Divider displacement is layout, not cursor travel.
            self.renderer.trail_cursor.snap();
        }
    }

    pub fn move_divider_left(&mut self) {
        let amount = 40.0; // Default movement amount
        if self
            .context_manager
            .move_divider_left(amount, &mut self.sugarloaf)
        {
            self.mark_dirty();
            // Divider displacement is layout, not cursor travel.
            self.renderer.trail_cursor.snap();
        }
    }

    pub fn move_divider_right(&mut self) {
        let amount = 40.0; // Default movement amount
        if self
            .context_manager
            .move_divider_right(amount, &mut self.sugarloaf)
        {
            self.mark_dirty();
            // Divider displacement is layout, not cursor travel.
            self.renderer.trail_cursor.snap();
        }
    }

    pub fn create_tab(&mut self, clipboard: &mut Clipboard) {
        let host_id = self
            .context_manager
            .current()
            .host_id
            .clone()
            .unwrap_or_else(|| hosts::LOCAL_ID.to_string());
        let (shell, env) = match self.shell_for_row(&host_id) {
            Ok(launch) => launch,
            Err(err) => {
                self.chrome.panel.error = Some(err);
                return;
            }
        };
        let _ = self.create_tab_with_shell(clipboard, shell, env, Some(host_id));
    }

    /// Create a tab whose session runs `shell` instead of the app's own.
    ///
    /// Everything else is `create_tab`: the tab still lands beside the
    /// current one, and the margin is still recalculated first so the two
    /// tabs agree on their grid. Only the shell differs — which is what
    /// makes "open this distro" a session rather than a second local shell.
    pub fn create_tab_with_shell(
        &mut self,
        clipboard: &mut Clipboard,
        shell: Option<Shell>,
        env: Option<Vec<(String, String)>>,
        host_id: Option<String>,
    ) -> Result<(), String> {
        let redirect = true;

        // We resize the current tab ahead to prepare the
        // dimensions to be copied to next tab.
        let num_tabs = self.ctx().len();
        let old_index = self.context_manager.current_index();
        self.resize_top_or_bottom_line(num_tabs + 1);

        // Update the old tab's rich text positions to reflect the new margin
        // (on Linux/Windows when hide_if_single transitions from hidden to visible)
        #[cfg(not(target_os = "macos"))]
        self.context_manager.contexts_mut()[old_index]
            .update_dimensions(&mut self.sugarloaf);

        // Allocate panel id; the layout pass handles positioning via
        // `ContextDimension` once the new tab's grid is built.
        let _ = self.context_manager.current_grid().scaled_margin.left;
        let _ = self.renderer.margin.top
            + self.renderer.island.as_ref().map_or(0.0, |i| i.height());
        let rich_text_id = next_rich_text_id();
        // A shell that cannot spawn leaves the tab count as it was, so the
        // resize above is undone rather than leaving a gap behind.
        let opened = self.context_manager.add_context_with_shell(
            redirect,
            rich_text_id,
            shell,
            env,
            host_id,
        );
        if opened.is_err() {
            self.resize_top_or_bottom_line(num_tabs);
        }
        opened?;

        let new_index = self.context_manager.current_index();
        self.context_manager.switch_context_visibility(
            &mut self.sugarloaf,
            old_index,
            new_index,
        );

        self.cancel_search(clipboard);
        // The tab that just came to the front decides which row is lit: a
        // local tab clears a distro highlight, a distro keeps its own.
        self.sync_sidebar_selection();
        self.mark_dirty();
        Ok(())
    }

    /// Open the session a sidebar row stands for.
    ///
    /// SSH hosts use the OpenSSH CLI MVP (`ssh_shell`); see that helper's docs
    /// and `milestone.md` (Option A / 1.4-debt).
    ///
    /// Returns the message to show the user when it cannot open: the panel
    /// has one error line, and a row that does nothing at all is the one
    /// outcome this must never produce.
    pub fn open_host_session(
        &mut self,
        id: &str,
        clipboard: &mut Clipboard,
    ) -> Result<(), String> {
        // If this host already has open sessions, focus the last one (else first).
        {
            let len = self.context_manager.len();
            let mut last: Option<usize> = None;
            let mut first: Option<usize> = None;
            for i in 0..len {
                let host = self
                    .context_manager
                    .contexts_mut()
                    .get(i)
                    .and_then(|g| g.current().host_id.clone())
                    .unwrap_or_else(|| hosts::LOCAL_ID.to_string());
                if host == id {
                    if first.is_none() {
                        first = Some(i);
                    }
                    last = Some(i);
                }
            }
            if let Some(idx) = last.or(first) {
                if idx != self.context_manager.current_index() {
                    self.stop_hint_mode_if_active();
                    self.cancel_search(clipboard);
                    self.clear_selection();
                    let old = self.context_manager.current_index();
                    self.context_manager.set_current(idx);
                    self.switch_visible_context(old, idx);
                    self.mark_dirty();
                }
                self.sync_sidebar_selection();
                // Refresh OS/distro icon even when reusing an open session.
                self.host_store.detect_os(id);
                return Ok(());
            }
        }

        // Reuse the pinned home tab instead of opening a duplicate local.
        if id == hosts::LOCAL_ID {
            if let Some(idx) = self.context_manager.find_home_tab() {
                if idx != self.context_manager.current_index() {
                    self.stop_hint_mode_if_active();
                    self.cancel_search(clipboard);
                    self.clear_selection();
                    let old = self.context_manager.current_index();
                    self.context_manager.set_current(idx);
                    self.switch_visible_context(old, idx);
                    self.mark_dirty();
                }
                self.sync_sidebar_selection();
                return Ok(());
            }
        }

        let (shell, env) = match self.shell_for_row(id) {
            Ok(launch) => launch,
            Err(err) if err.contains("Unlock the vault") => {
                self.open_vault_unlock_for(terminus_ui::PendingVaultAction::OpenHost(
                    id.to_string(),
                ));
                return Ok(());
            }
            Err(err) => return Err(err),
        };

        let label = self
            .chrome
            .panel
            .rows
            .iter()
            .filter_map(terminus_ui::sidebar::Row::host)
            .find(|item| item.id == id)
            .map(|item| item.name.clone())
            .unwrap_or_else(|| id.to_string());

        // Local shells come up instantly; WSL distros and SSH hosts can
        // sit on a blank PTY for a while, so they get the connecting
        // animation until the session writes something (or times out).
        let animate = id != hosts::LOCAL_ID;
        if animate {
            self.begin_session_connecting(id);
        }

        match self.create_tab_with_shell(clipboard, shell, env, Some(id.to_string())) {
            Ok(()) => {
                let tab_index = self.context_manager.current_index();
                let os_id = self
                    .chrome
                    .panel
                    .rows
                    .iter()
                    .filter_map(terminus_ui::sidebar::Row::host)
                    .find(|item| item.id == id)
                    .and_then(|item| item.os_id.clone());
                self.context_manager
                    .set_custom_title(tab_index, Some(label));
                self.context_manager.set_tab_os_id(tab_index, os_id);
                // Background: classify remote OS and update sidebar / tab glyphs.
                self.host_store.detect_os(id);
                Ok(())
            }
            Err(err) => {
                if animate {
                    self.end_session_connecting();
                }
                Err(format!("{label}: {err}"))
            }
        }
    }

    pub(super) fn begin_session_connecting(&mut self, id: &str) {
        self.chrome.panel.begin_connecting(id);
        let now = std::time::Instant::now();
        self.connecting_started = Some(now);
        self.connecting_step_at = Some(now);
        self.connecting_success_at = None;

        let host = self
            .chrome
            .panel
            .rows
            .iter()
            .filter_map(terminus_ui::sidebar::Row::host)
            .find(|item| item.id == id);

        let (title, endpoint, kind) = match host {
            Some(item) if item.badge == Badge::Wsl => (
                item.name.clone(),
                format!("WSL · {}", item.endpoint),
                terminus_ui::ConnectKind::Wsl,
            ),
            Some(item) => (
                item.name.clone(),
                format!("SSH {}", item.endpoint),
                terminus_ui::ConnectKind::Ssh,
            ),
            None if id.starts_with(hosts::WSL_PREFIX) => (
                id.trim_start_matches(hosts::WSL_PREFIX).to_string(),
                "WSL".to_string(),
                terminus_ui::ConnectKind::Wsl,
            ),
            None => (
                id.to_string(),
                format!("SSH {id}"),
                terminus_ui::ConnectKind::Ssh,
            ),
        };

        self.chrome.connection = Some(match kind {
            terminus_ui::ConnectKind::Wsl => {
                terminus_ui::ConnectionSequence::start_wsl(id, title, endpoint)
            }
            terminus_ui::ConnectKind::Ssh => {
                terminus_ui::ConnectionSequence::start_ssh(id, title, endpoint)
            }
        });
    }

    pub(super) fn end_session_connecting(&mut self) {
        self.chrome.panel.end_connecting();
        self.chrome.connection = None;
        self.connecting_started = None;
        self.connecting_step_at = None;
        self.connecting_success_at = None;
    }

    /// Public dismiss from the connection modal's Close button.
    pub fn force_end_connecting(&mut self) {
        self.end_session_connecting();
    }

    /// Advance / retire the connection modal for this frame.
    ///
    /// Returns whether the chrome still needs continuous redraws.
    pub(super) fn tick_session_connecting(&mut self) -> bool {
        if self.chrome.connection.is_none() {
            self.connecting_started = None;
            return false;
        }
        let Some(started) = self.connecting_started else {
            return false;
        };

        let elapsed = started.elapsed();
        const STEP_MS: std::time::Duration = std::time::Duration::from_millis(850);
        const SUCCESS_HOLD: std::time::Duration = std::time::Duration::from_millis(1200);
        const MAX: std::time::Duration = std::time::Duration::from_secs(20);

        // User closed the modal.
        if self.chrome.connection.is_none() {
            return false;
        }

        if elapsed >= MAX {
            self.end_session_connecting();
            return false;
        }

        // Hold the success frame, then dismiss.
        if let Some(ok_at) = self.connecting_success_at {
            if ok_at.elapsed() >= SUCCESS_HOLD {
                self.end_session_connecting();
                return false;
            }
            return true;
        }

        let id = self
            .chrome
            .connection
            .as_ref()
            .map(|c| c.host_id.clone())
            .unwrap_or_default();

        let ctx = self.context_manager.current();
        if ctx.host_id.as_deref() != Some(id.as_str()) && elapsed.as_millis() > 200 {
            self.end_session_connecting();
            return false;
        }

        // Advance steps on a timer so the line fills like the mock.
        let step_at = self.connecting_step_at.unwrap_or(started);
        if step_at.elapsed() >= STEP_MS {
            if let Some(conn) = self.chrome.connection.as_mut() {
                if conn.advance() {
                    self.connecting_step_at = Some(std::time::Instant::now());
                }
            }
        }

        // Ready when the PTY has spoken, or we've finished the last step
        // and waited a beat for WSL shells that are slow to paint.
        let on_last = self
            .chrome
            .connection
            .as_ref()
            .is_some_and(|c| c.step + 1 >= terminus_ui::STEP_COUNT);
        let ready = terminal_has_printable_output(ctx)
            || (on_last && step_at.elapsed() >= STEP_MS);

        if ready {
            if let Some(conn) = self.chrome.connection.as_mut() {
                conn.mark_success();
            }
            self.chrome.panel.end_connecting();
            self.connecting_success_at = Some(std::time::Instant::now());
        }

        true
    }

    /// Looping `0..1` phase for the active-node pulse.
    pub(super) fn connecting_phase(&self) -> Option<f32> {
        let started = self.connecting_started?;
        if self.chrome.connection.is_none() {
            return None;
        }
        Some(terminus_ui::loading_phase(started.elapsed().as_secs_f32()))
    }

    /// Point the sidebar highlight at the active session (and its host).
    pub(super) fn sync_sidebar_selection(&mut self) {
        let tab_index = self.context_manager.current_index();
        let id = self
            .context_manager
            .current()
            .host_id
            .clone()
            .unwrap_or_else(|| hosts::LOCAL_ID.to_string());

        self.chrome.panel.follow_session(tab_index, &id);
    }

    /// Advance host→group snap tween; returns a persist action when done.
    pub(super) fn tick_host_drag_animation(
        &mut self,
    ) -> Option<terminus_ui::chrome::ChromeAction> {
        if !self
            .chrome
            .panel
            .host_drag
            .as_ref()
            .is_some_and(|d| d.is_snapping())
        {
            self.host_drag_anim_at = None;
            return None;
        }
        let now = std::time::Instant::now();
        let dt = self
            .host_drag_anim_at
            .map(|t| now.saturating_duration_since(t).as_secs_f32())
            .unwrap_or(1.0 / 60.0)
            .min(0.05);
        self.host_drag_anim_at = Some(now);
        self.chrome.tick_host_drag(dt)
    }

    pub fn apply_host_drag_action(&mut self, action: terminus_ui::chrome::ChromeAction) {
        match action {
            terminus_ui::chrome::ChromeAction::SetHostGroup { host_id, group_id } => {
                self.host_store
                    .set_host_group(&host_id, group_id.as_deref());
                let _ = self.pump_chrome();
            }
            terminus_ui::chrome::ChromeAction::ReorderHost {
                host_id,
                before_host_id,
                before_group_id,
            } => {
                self.host_store.reorder_host(
                    &host_id,
                    before_host_id.as_deref(),
                    before_group_id.as_deref(),
                );
                let _ = self.pump_chrome();
            }
            terminus_ui::chrome::ChromeAction::ReorderGroup {
                group_id,
                before_group_id,
                before_host_id,
            } => {
                self.host_store.reorder_group(
                    &group_id,
                    before_group_id.as_deref(),
                    before_host_id.as_deref(),
                );
                let _ = self.pump_chrome();
            }
            _ => {}
        }
    }

    /// Focus an already-open session by tab index.
    pub fn focus_session(&mut self, tab_index: usize, clipboard: &mut Clipboard) {
        if tab_index >= self.context_manager.len() {
            return;
        }
        if tab_index == self.context_manager.current_index() {
            self.sync_sidebar_selection();
            return;
        }
        self.stop_hint_mode_if_active();
        self.cancel_search(clipboard);
        self.clear_selection();
        let old = self.context_manager.current_index();
        self.context_manager.set_current(tab_index);
        self.switch_visible_context(old, tab_index);
        self.mark_dirty();
    }

    /// Open another session for a host (sidebar "+" control).
    pub fn add_host_session(
        &mut self,
        id: &str,
        clipboard: &mut Clipboard,
    ) -> Result<(), String> {
        let (shell, env) = match self.shell_for_row(id) {
            Ok(launch) => launch,
            Err(err) if err.contains("Unlock the vault") => {
                self.open_vault_unlock_for(
                    terminus_ui::PendingVaultAction::AddHostSession(id.to_string()),
                );
                return Ok(());
            }
            Err(err) => return Err(err),
        };
        let label = self
            .chrome
            .panel
            .rows
            .iter()
            .filter_map(terminus_ui::sidebar::Row::host)
            .find(|item| item.id == id)
            .map(|item| item.name.clone())
            .unwrap_or_else(|| id.to_string());

        let animate = id != hosts::LOCAL_ID;
        if animate {
            self.begin_session_connecting(id);
        }

        match self.create_tab_with_shell(clipboard, shell, env, Some(id.to_string())) {
            Ok(()) => {
                let tab_index = self.context_manager.current_index();
                let os_id = self
                    .chrome
                    .panel
                    .rows
                    .iter()
                    .filter_map(terminus_ui::sidebar::Row::host)
                    .find(|item| item.id == id)
                    .and_then(|item| item.os_id.clone());
                self.context_manager
                    .set_custom_title(tab_index, Some(label));
                self.context_manager.set_tab_os_id(tab_index, os_id);
                self.host_store.detect_os(id);
                Ok(())
            }
            Err(err) => {
                if animate {
                    self.end_session_connecting();
                }
                Err(format!("{label}: {err}"))
            }
        }
    }

    /// Show tab `new_index`, hide `old_index`, and move the sidebar highlight
    /// to whatever that tab came from.
    ///
    /// Every tab switch goes through here. Changing which tab is in front is
    /// exactly when the highlight goes stale, and there are a dozen places
    /// that switch — one of them is the reason this is a method and not a
    /// line repeated at each call site.
    pub(super) fn switch_visible_context(&mut self, old_index: usize, new_index: usize) {
        self.context_manager.switch_context_visibility(
            &mut self.sugarloaf,
            old_index,
            new_index,
        );
        self.sync_sidebar_selection();
    }

    pub fn close_split_or_tab(&mut self, clipboard: &mut Clipboard) {
        if self.context_manager.current_grid_len() > 1 {
            self.clear_selection();
            self.context_manager
                .remove_current_grid(&mut self.sugarloaf);
            self.mark_dirty();
        } else {
            self.close_tab(clipboard);
        }
    }

    pub fn close_tab(&mut self, clipboard: &mut Clipboard) {
        self.close_tab_at(self.context_manager.current_index(), clipboard);
    }

    /// Close the tab at `index` (active or not). Refuses the pinned home tab.
    pub fn close_tab_at(&mut self, index: usize, clipboard: &mut Clipboard) {
        if index >= self.context_manager.len() {
            return;
        }
        if self.context_manager.is_pinned(index) {
            return;
        }
        self.clear_selection();
        if index != self.context_manager.current_index() {
            self.context_manager.set_current(index);
        }
        self.context_manager
            .close_current_context(&mut self.sugarloaf);
        // Closing the last tab of a distro has to move the highlight off it:
        // the row is re-derived from the tab now in front, which is the local
        // one when no host tab is left.
        self.sync_sidebar_selection();
        if let Some(ref mut island) = self.renderer.island {
            island.dismiss_color_picker();
            let _ = island.set_tab_hover(None, false);
        }

        self.cancel_search(clipboard);
        let num_tabs = self.ctx().len();
        self.resize_top_or_bottom_line(num_tabs);
        self.mark_dirty();
    }

    pub fn resize_top_or_bottom_line(&mut self, num_tabs: usize) {
        let padding_y_top = padding_top_from_config(
            &self.renderer.navigation,
            self.renderer.margin.top,
            num_tabs,
            self.renderer.macos_use_unified_titlebar,
        );
        let padding_y_bottom = self.renderer.margin.bottom;

        // Keep the rail/drawer under the tab strip. Stale top_inset lets the
        // panel eat clicks on the islands (close / switch).
        self.chrome.top_inset = padding_y_top;

        let scale = self.sugarloaf.scale_factor();
        let scaled_top = padding_y_top * scale;
        let scaled_bottom = padding_y_bottom * scale;

        // Compare against the grid's scaled margin, the value the
        // layout actually uses. The per panel dimension margin is
        // zeroed by the taffy pass so it cannot be used as a guard.
        let current_margin = self.context_manager.current_grid().scaled_margin;
        if current_margin.top == scaled_top && current_margin.bottom == scaled_bottom {
            return;
        }

        let current_dim = self.context_manager.current().dimension;
        if current_dim.font_size <= 0.0 {
            return;
        }

        let s = self.sugarloaf.style_mut();
        s.font_size = current_dim.font_size;
        s.line_height = current_dim.line_height;

        // Every tab shares the window, so every grid needs the new
        // margin and a layout pass, not just the current one.
        for context_grid in self.context_manager.contexts_mut() {
            let margin = context_grid.scaled_margin;
            context_grid.update_scaled_margin(Margin::new(
                scaled_top,
                margin.right,
                scaled_bottom,
                margin.left,
            ));
            context_grid.update_dimensions(&mut self.sugarloaf);
        }

        // The tab strip appearing or vanishing shifts every panel;
        // a background tab can close without a route change, so this
        // reflow is not covered by the route-switch teleport.
        self.renderer.trail_cursor.snap();
    }
}

/// True when the session's grid already shows something other than blank
/// cells — the cue that the connecting overlay can retire.
pub(super) fn terminal_has_printable_output(ctx: &context::Context<EventProxy>) -> bool {
    use crate::crosswords::pos::{Column, Line};
    let terminal = ctx.terminal.lock();
    let lines = terminal.screen_lines().min(16);
    let cols = terminal.columns().min(120);
    for row in 0..lines {
        let line = Line(row as i32);
        for col in 0..cols {
            let c = terminal.grid[line][Column(col)].c();
            if !c.is_whitespace() && c != '\0' {
                return true;
            }
        }
    }
    false
}
