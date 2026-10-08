//! `Screen` sessions surface, split out of `screen/mod.rs`.

use super::Screen;
use crate::context;
use crate::context::next_rich_text_id;
use crate::hosts;
use crate::renderer::utils::padding_top_from_config;
use rio_backend::clipboard::Clipboard;
use rio_backend::config::layout::Margin;
use rio_backend::config::Shell;
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
        self.create_tab_in(clipboard, shell, env, host_id, None)
    }

    /// `create_tab_with_shell`, starting the shell in `start_dir` rather
    /// than wherever a new tab would start.
    pub(super) fn create_tab_in(
        &mut self,
        clipboard: &mut Clipboard,
        shell: Option<Shell>,
        env: Option<Vec<(String, String)>>,
        host_id: Option<String>,
        start_dir: Option<String>,
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
            start_dir,
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
        // Picking a machine brings its terminal forward (design: a sidebar
        // row always lands on Terminal).
        self.show_view(terminus_ui::shell::WorkspaceView::Terminal);
        // If this host already has open sessions, go back to the one the
        // user was last on rather than whichever sits last in the strip.
        if let Some(idx) = self.context_manager.host_tab_to_restore(id) {
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

        let label = self.host_row_label(id);

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
                self.dress_host_tab(tab_index, id, label);
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

    /// The sidebar's name for a row id, or the id itself.
    pub(super) fn host_row_label(&self, id: &str) -> String {
        self.chrome
            .panel
            .rows
            .iter()
            .filter_map(terminus_ui::sidebar::Row::host)
            .find(|item| item.id == id)
            .map(|item| item.name.clone())
            .unwrap_or_else(|| id.to_string())
    }

    /// Give a tab just opened for host row `id` its label, OS glyph and
    /// endpoint.
    pub(super) fn dress_host_tab(&mut self, tab_index: usize, id: &str, label: String) {
        let row = self
            .chrome
            .panel
            .rows
            .iter()
            .filter_map(terminus_ui::sidebar::Row::host)
            .find(|item| item.id == id);
        let os_id = row.and_then(|item| item.os_id.clone());
        let endpoint = row
            .map(|item| item.endpoint.clone())
            .filter(|e| !e.is_empty());
        self.context_manager
            .set_custom_title(tab_index, Some(label));
        self.context_manager.set_tab_os_id(tab_index, os_id);
        self.context_manager.set_tab_host_label(tab_index, endpoint);
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
                // Distro rows already read "WSL · running · default".
                if item.endpoint.starts_with("WSL") {
                    item.endpoint.clone()
                } else {
                    format!("WSL · {}", item.endpoint)
                },
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

    /// A session's process exited (`RioEvent::CloseTerminal`). When it
    /// was still coming up — the connection progress is for its machine —
    /// the tab would just vanish: retire the progress and say why in the
    /// sidebar, with the first line the session printed (ssh's or
    /// wsl.exe's own error).
    pub fn note_session_exit(&mut self, route_id: usize) {
        let Some(host) = self.chrome.connection.as_ref().map(|c| c.host_id.clone())
        else {
            return;
        };
        let Some(item) = self.context_manager.get_by_route_id(route_id) else {
            return;
        };
        let ctx = item.context();
        if ctx.host_id.as_deref() != Some(host.as_str()) {
            return;
        }
        let output = printable_lines(ctx);
        let Some(message) = self
            .chrome
            .connection
            .as_ref()
            .map(|c| c.failure_message(&output))
        else {
            return;
        };
        self.end_session_connecting();
        self.chrome.panel.error = Some(message);
    }

    /// A session's process exited with `status` (`RioEvent::ChildExited`,
    /// which arrives just before its `CloseTerminal`).
    ///
    /// When an SSH host session's `ssh` gave up on its link (exit 255) or
    /// the local `ssh` was killed by a signal, it is kept behind a card that
    /// says so and offers Reconnect, instead of vanishing. In a split tab
    /// only that pane is kept: the card covers the dead pane and its
    /// siblings stay live. Every other end closes as before: exit 0, no
    /// reported status, and any other exit code, which is the remote
    /// shell's last command status after the user left (`exit`, Ctrl-D).
    ///
    /// A user-initiated close never reaches the card: dropping a `Context`
    /// SIGHUPs its ssh, but the context is already out of the grid by then
    /// and route ids are never reused, so the late `ChildExited` finds no
    /// route (`tab_of_route`). Returns whether the session was kept.
    pub fn note_child_exit(&mut self, route_id: usize, status: Option<i32>) -> bool {
        let end = session_end(status);
        if !end.keeps_tab() {
            return false;
        }
        let Some(tab) = self.tab_of_route(route_id) else {
            return false;
        };
        let whole_tab = self.context_manager.contexts_mut()[tab].len() == 1;
        let Some(host_id) = self.context_manager.contexts_mut()[tab]
            .get_by_route_id(route_id)
            .and_then(|item| item.context().host_id.clone())
        else {
            return false;
        };
        // Only stored SSH hosts: the local shell and WSL distros are not
        // links that can drop.
        if !self
            .host_store
            .hosts()
            .iter()
            .any(|host| host.id == host_id)
        {
            return false;
        }
        // Still coming up: the connection progress reports that failure.
        if self
            .chrome
            .connection
            .as_ref()
            .is_some_and(|conn| conn.host_id == host_id)
        {
            return false;
        }
        let name = self.host_row_label(&host_id);
        let Some(item) =
            self.context_manager.contexts_mut()[tab].get_by_route_id(route_id)
        else {
            return false;
        };
        let output = lines_up_to_cursor(item.context(), 8);
        let Some(card) = terminus_ui::LostSession::ended(
            route_id, &host_id, &name, &output, end, whole_tab,
        ) else {
            return false;
        };
        item.context_mut().connection_lost = Some(card);
        tracing::info!("ssh session for {host_id} ended: {}", end.status_text());
        self.mark_dirty();
        true
    }

    /// Whether `route_id` is a tab kept open after its connection dropped.
    pub fn is_connection_lost(&mut self, route_id: usize) -> bool {
        self.context_manager
            .get_by_route_id(route_id)
            .is_some_and(|item| item.context().connection_lost.is_some())
    }

    /// Index of the tab holding terminal `route_id`.
    fn tab_of_route(&mut self, route_id: usize) -> Option<usize> {
        self.context_manager
            .contexts_mut()
            .iter_mut()
            .position(|grid| grid.get_by_route_id(route_id).is_some())
    }

    /// Mirror the front tab's lost-connection card into the chrome, keeping
    /// its hover/focus while the same session stays shown. Runs every frame,
    /// so a tab switch (from any of the places that switch) shows the right
    /// card or none, and a split tab's card follows its pane through
    /// resizes and divider drags.
    pub(super) fn sync_lost_session(&mut self) {
        let scale = self.sugarloaf.scale_factor();
        let grid = self.context_manager.current_grid_mut();
        let margin = grid.get_scaled_margin();
        let split = grid.len() > 1;
        let pane = grid.lost_pane();
        let front = pane.and_then(|pane| {
            let lost = grid
                .get_by_route_id(pane.route_id)?
                .context_mut()
                .connection_lost
                .as_mut()?;
            // The close button says what it closes now, split or not.
            lost.set_whole_tab(!split);
            Some(lost.clone())
        });
        let same = match (front.as_ref(), self.chrome.lost.as_ref()) {
            (Some(want), Some(shown)) => want.route_id == shown.route_id,
            (None, None) => true,
            _ => false,
        };
        if !same {
            self.chrome.lost = front;
        } else if let Some(shown) = self.chrome.lost.as_mut() {
            shown.set_whole_tab(!split);
        }
        // A whole-tab card covers the content; a split pane's covers the pane.
        self.chrome.lost_pane = pane.filter(|_| split).map(|pane| {
            pane_chrome_rect(pane.layout_rect, margin.left, margin.top, scale)
        });
        self.chrome.lost_pane_unfocused = pane.is_some_and(|pane| !pane.focused);
    }

    /// Run the lost-connection card's Reconnect / Close tab.
    pub fn run_lost_session_action(
        &mut self,
        action: terminus_ui::ChromeAction,
        clipboard: &mut Clipboard,
    ) {
        match action {
            terminus_ui::ChromeAction::ReconnectSession(route_id) => {
                if let Err(err) = self.reconnect_lost_session(route_id, clipboard) {
                    self.chrome.panel.error = Some(err);
                }
            }
            terminus_ui::ChromeAction::CloseLostSession(route_id) => {
                if let Some(tab) = self.tab_of_route(route_id) {
                    if self.context_manager.contexts_mut()[tab].len() > 1 {
                        self.close_lost_pane(route_id);
                    } else {
                        self.close_tab_at(tab, clipboard);
                    }
                }
            }
            _ => return,
        }
        self.sync_lost_session();
    }

    /// Reopen the host of dead tab `route_id` in its place: a fresh session
    /// (with the connection progress) lands beside it, takes over its name,
    /// and the dead tab closes, so the new one sits where the old one was.
    pub fn reconnect_lost_session(
        &mut self,
        route_id: usize,
        clipboard: &mut Clipboard,
    ) -> Result<(), String> {
        let Some(tab) = self.tab_of_route(route_id) else {
            return Ok(());
        };
        let Some(host_id) = self.context_manager.contexts_mut()[tab]
            .get_by_route_id(route_id)
            .and_then(|item| item.context().connection_lost.as_ref())
            .map(|lost| lost.host_id.clone())
        else {
            return Ok(());
        };
        // One pane of a split tab: its siblings stay, the fresh session
        // opens as a tab of its own and only the dead pane goes.
        let split = self.context_manager.contexts_mut()[tab].len() > 1;
        let title = self.context_manager.custom_title(tab).map(str::to_string);
        if tab != self.context_manager.current_index() {
            self.focus_session(tab, clipboard);
        }
        let before = self.context_manager.len();
        self.add_host_session(&host_id, clipboard)?;
        // No new tab: the vault prompt took over and opens one on unlock;
        // the dead tab stays until then.
        if self.context_manager.len() == before {
            return Ok(());
        }
        if split {
            self.close_lost_pane(route_id);
            return Ok(());
        }
        // A name the user gave the tab survives the reconnect.
        let fresh = self.context_manager.current_index();
        if title.is_some() {
            self.context_manager.set_custom_title(fresh, title);
        }
        // New tabs open last: close the dead one, then move the new one
        // (still last) into its slot.
        self.close_tab_at(tab, clipboard);
        let last = self.context_manager.len() - 1;
        self.focus_session(last, clipboard);
        if tab < last {
            self.context_manager.move_current_tab_to(tab);
            let now = self.context_manager.current_index();
            self.switch_visible_context(last, now);
        }
        self.mark_dirty();
        Ok(())
    }

    /// Remove just the dead pane `route_id` from its split tab; the other
    /// panes (and the tab) stay.
    fn close_lost_pane(&mut self, route_id: usize) {
        self.clear_selection();
        let _ = self
            .context_manager
            .should_close_context_manager(route_id, &mut self.sugarloaf);
        self.mark_dirty();
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

        // User closed the modal.
        if self.chrome.connection.is_none() {
            return false;
        }

        let give_up = self
            .chrome
            .connection
            .as_ref()
            .map(terminus_ui::ConnectionSequence::give_up_after)
            .unwrap_or_default();
        if elapsed >= give_up {
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

        // Ready when the PTY has spoken, or (WSL shells, slow to paint)
        // the last step has been held for a beat.
        let on_last = self
            .chrome
            .connection
            .as_ref()
            .is_some_and(|c| c.step + 1 >= terminus_ui::STEP_COUNT);
        let printed = terminal_has_printable_output(ctx);
        let held = on_last && step_at.elapsed() >= STEP_MS;
        let ready = self
            .chrome
            .connection
            .as_ref()
            .is_some_and(|c| c.is_ready(printed, held));

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
        self.chrome.connection.as_ref()?;
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

    /// Advance a host drag (edge auto-scroll; legacy snap tween); returns
    /// a persist action when done.
    pub(super) fn tick_host_drag_animation(
        &mut self,
    ) -> Option<terminus_ui::chrome::ChromeAction> {
        if !self
            .chrome
            .panel
            .host_drag
            .as_ref()
            .is_some_and(|d| d.is_snapping() || d.started())
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
        let label = self.host_row_label(id);

        let animate = id != hosts::LOCAL_ID;
        if animate {
            self.begin_session_connecting(id);
        }

        match self.create_tab_with_shell(clipboard, shell, env, Some(id.to_string())) {
            Ok(()) => {
                let tab_index = self.context_manager.current_index();
                self.dress_host_tab(tab_index, id, label);
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

    /// Delete a stored host and close the tabs opened from it: they would
    /// otherwise live on with no sidebar row to reach them by.
    pub fn delete_host_closing_sessions(&mut self, id: &str, clipboard: &mut Clipboard) {
        for index in (0..self.context_manager.len()).rev() {
            let from_host = self
                .context_manager
                .contexts_mut()
                .get(index)
                .and_then(|grid| grid.current().host_id.clone())
                .is_some_and(|host| host == id);
            if from_host {
                self.close_tab_at(index, clipboard);
            }
        }
        if let Some(tunnels) = self.tunnels.as_mut() {
            tunnels.host_deleted(id);
        }
        self.host_store.delete_host(id);
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
        let padding_y_bottom = crate::renderer::utils::padding_bottom_from_config(
            &self.renderer.navigation,
            self.renderer.margin.bottom,
        );

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

/// The session's visible lines, top to bottom, trailing blanks trimmed.
fn printable_lines(ctx: &context::Context<EventProxy>) -> Vec<String> {
    use crate::crosswords::pos::{Column, Line};
    let terminal = ctx.terminal.lock();
    let lines = terminal.screen_lines().min(16);
    let cols = terminal.columns().min(200);
    (0..lines)
        .map(|row| {
            let line = Line(row as i32);
            let text: String = (0..cols)
                .map(|col| match terminal.grid[line][Column(col)].c() {
                    '\0' => ' ',
                    c => c,
                })
                .collect();
            text.trim_end().to_string()
        })
        .collect()
}

/// The last `count` screen lines down to the cursor, top to bottom: where
/// a long-running session's final words (ssh's disconnect message) are,
/// unlike [`printable_lines`], which reads a fresh session from the top.
fn lines_up_to_cursor(ctx: &context::Context<EventProxy>, count: usize) -> Vec<String> {
    use crate::crosswords::pos::{Column, Line};
    let terminal = ctx.terminal.lock();
    let cursor = terminal.grid.cursor.pos.row.0.max(0);
    let first = (cursor + 1 - count as i32).max(0);
    let cols = terminal.columns().min(200);
    (first..=cursor)
        .map(|row| {
            let line = Line(row);
            let text: String = (0..cols)
                .map(|col| match terminal.grid[line][Column(col)].c() {
                    '\0' => ' ',
                    c => c,
                })
                .collect();
            text.trim_end().to_string()
        })
        .collect()
}

/// The exit code in a `ChildExited` status: Unix reports the raw wait
/// status, Windows the code itself.
pub(super) fn exit_code(status: Option<i32>) -> Option<i32> {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        status.and_then(|raw| std::process::ExitStatus::from_raw(raw).code())
    }
    #[cfg(not(unix))]
    {
        status
    }
}

/// The signal that killed the process in a `ChildExited` status, if one did.
pub(super) fn exit_signal(status: Option<i32>) -> Option<i32> {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        status.and_then(|raw| std::process::ExitStatus::from_raw(raw).signal())
    }
    #[cfg(not(unix))]
    {
        // Windows reports an exit code only.
        let _ = status;
        None
    }
}

/// How the session's process ended, from its `ChildExited` status.
pub(super) fn session_end(status: Option<i32>) -> terminus_ui::lost_session::SessionEnd {
    terminus_ui::lost_session::classify_exit(exit_code(status), exit_signal(status))
}

/// A pane's `layout_rect` (physical pixels, relative to the grid root) as
/// the chrome's logical rect: offset by the grid margin, then unscaled the
/// way `apply_taffy_layout` positions the pane.
pub(super) fn pane_chrome_rect(
    layout_rect: [f32; 4],
    margin_left: f32,
    margin_top: f32,
    scale: f32,
) -> terminus_ui::geom::Rect {
    terminus_ui::geom::Rect {
        x: (layout_rect[0] + margin_left) / scale,
        y: (layout_rect[1] + margin_top) / scale,
        width: layout_rect[2] / scale,
        height: layout_rect[3] / scale,
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

#[cfg(test)]
mod tests {
    use super::{exit_code, exit_signal, pane_chrome_rect, session_end};
    use terminus_ui::lost_session::SessionEnd;

    #[test]
    #[cfg(unix)]
    fn exit_code_decodes_the_raw_wait_status() {
        // waitpid's status: the code sits in the second byte.
        assert_eq!(exit_code(Some(255 << 8)), Some(255));
        assert_eq!(exit_code(Some(0)), Some(0));
        // Killed by SIGHUP: no exit code, so never "connection lost".
        assert_eq!(exit_code(Some(libc::SIGHUP)), None);
        assert_eq!(exit_code(None), None);
    }

    #[test]
    #[cfg(unix)]
    fn exit_signal_decodes_the_raw_wait_status() {
        assert_eq!(exit_signal(Some(libc::SIGKILL)), Some(9));
        assert_eq!(exit_signal(Some(libc::SIGHUP)), Some(1));
        // A normal exit has a code, not a signal.
        assert_eq!(exit_signal(Some(1 << 8)), None);
        assert_eq!(exit_signal(Some(0)), None);
        assert_eq!(exit_signal(None), None);
    }

    #[test]
    #[cfg(unix)]
    fn session_end_classifies_raw_wait_statuses() {
        assert_eq!(session_end(Some(0)), SessionEnd::Clean);
        assert_eq!(session_end(Some(255 << 8)), SessionEnd::ConnectionLost);
        assert_eq!(session_end(Some(1 << 8)), SessionEnd::Exited(1));
        assert_eq!(session_end(Some(libc::SIGKILL)), SessionEnd::Signaled(9));
        assert_eq!(session_end(None), SessionEnd::Unknown);
    }

    #[test]
    fn pane_rect_is_the_layout_rect_offset_by_the_margin_and_unscaled() {
        // Physical layout rect [x, y, w, h] with a 20x10 physical margin at 2x.
        let rect = pane_chrome_rect([100.0, 40.0, 600.0, 300.0], 20.0, 10.0, 2.0);
        assert_eq!(
            (rect.x, rect.y, rect.width, rect.height),
            (60.0, 25.0, 300.0, 150.0)
        );
    }
}
