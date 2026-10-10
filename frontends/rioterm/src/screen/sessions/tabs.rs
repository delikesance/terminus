//! Screen sessions: tabs.

use super::super::Screen;
use crate::hosts;
use rio_backend::clipboard::Clipboard;

impl Screen<'_> {
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
    pub(in crate::screen) fn switch_visible_context(
        &mut self,
        old_index: usize,
        new_index: usize,
    ) {
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
}
