//! Screen sessions: lost.

use super::super::Screen;
use crate::context::next_rich_text_id;
use rio_backend::clipboard::Clipboard;

use super::probe::{lines_up_to_cursor, pane_chrome_rect, printable_lines, session_end};

impl Screen<'_> {
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
    pub(in crate::screen) fn tab_of_route(&mut self, route_id: usize) -> Option<usize> {
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
    pub(in crate::screen) fn sync_lost_session(&mut self) {
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

    /// Whether launching `host_id` needs the vault passphrase first.
    pub(in crate::screen) fn vault_locked_for(&self, host_id: &str) -> bool {
        matches!(self.shell_for_row(host_id), Err(err) if err.contains("Unlock the vault"))
    }

    /// Reopen the host of dead tab `route_id` in its place: a fresh session
    /// (with the connection progress) lands beside it, takes over its name,
    /// and the dead tab closes, so the new one sits where the old one was.
    /// A dead pane of a split tab is replaced inside the split instead.
    /// A locked vault prompts first and leaves the dead session until unlock.
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
        if self.vault_locked_for(&host_id) {
            self.open_vault_unlock_for(
                terminus_ui::PendingVaultAction::ReconnectSession(route_id),
            );
            return Ok(());
        }
        if self.context_manager.contexts_mut()[tab].len() > 1 {
            return self.reconnect_lost_pane(tab, route_id, &host_id, clipboard);
        }
        let title = self.context_manager.custom_title(tab).map(str::to_string);
        if tab != self.context_manager.current_index() {
            self.focus_session(tab, clipboard);
        }
        let before = self.context_manager.len();
        self.add_host_session(&host_id, clipboard)?;
        if self.context_manager.len() == before {
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

    /// Reconnect dead pane `route_id` of split tab `tab`: a fresh session
    /// splits beside it, then the dead pane goes, so its siblings stay put.
    pub(in crate::screen) fn reconnect_lost_pane(
        &mut self,
        tab: usize,
        route_id: usize,
        host_id: &str,
        clipboard: &mut Clipboard,
    ) -> Result<(), String> {
        let (shell, env) = self.shell_for_row(host_id)?;
        self.focus_session(tab, clipboard);
        self.context_manager.contexts_mut()[tab].select_route_id(route_id);
        let label = self.host_row_label(host_id);
        self.begin_session_connecting(host_id);
        if let Err(err) = self.context_manager.split_with_shell(
            next_rich_text_id(),
            false,
            &mut self.sugarloaf,
            shell,
            env,
            Some(host_id.to_string()),
        ) {
            self.end_session_connecting();
            return Err(format!("{label}: {err}"));
        }
        self.host_store.detect_os(host_id);
        self.close_lost_pane(route_id);
        Ok(())
    }

    /// Remove just the dead pane `route_id` from its split tab; the other
    /// panes (and the tab) stay.
    pub(in crate::screen) fn close_lost_pane(&mut self, route_id: usize) {
        self.clear_selection();
        let _ = self
            .context_manager
            .should_close_context_manager(route_id, &mut self.sugarloaf);
        self.mark_dirty();
    }
}
