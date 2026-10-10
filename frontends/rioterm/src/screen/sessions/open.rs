//! Screen sessions: open.

use super::super::Screen;
use crate::context::next_rich_text_id;
use crate::hosts;
use rio_backend::clipboard::Clipboard;
use rio_backend::config::Shell;

impl Screen<'_> {
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
    pub(in crate::screen) fn create_tab_in(
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
    pub(in crate::screen) fn host_row_label(&self, id: &str) -> String {
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
    pub(in crate::screen) fn dress_host_tab(
        &mut self,
        tab_index: usize,
        id: &str,
        label: String,
    ) {
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
}
