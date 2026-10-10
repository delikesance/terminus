//! Screen sessions: groups.

use super::super::Screen;
use crate::hosts;
use rio_backend::clipboard::Clipboard;

impl Screen<'_> {
    /// Point the sidebar highlight at the active session (and its host).
    pub(in crate::screen) fn sync_sidebar_selection(&mut self) {
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
    pub(in crate::screen) fn tick_host_drag_animation(
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

    /// Delete a stored host and close the tabs opened from it: they would
    /// otherwise live on with no sidebar row to reach them by.
    pub fn delete_host_closing_sessions(&mut self, id: &str, clipboard: &mut Clipboard) {
        self.close_host_sessions(id, clipboard);
        if let Some(tunnels) = self.tunnels.as_mut() {
            tunnels.host_deleted(id);
        }
        self.host_store.delete_host(id);
    }

    /// Open a session on every host of group `group_id`; reports the first
    /// host that could not open.
    pub fn open_group_sessions(
        &mut self,
        group_id: &str,
        clipboard: &mut Clipboard,
    ) -> Result<(), String> {
        let mut first_error = None;
        for id in self.group_host_ids(group_id) {
            if let Err(err) = self.open_host_session(&id, clipboard) {
                first_error.get_or_insert(err);
            }
            if self.chrome.vault_unlock_is_open() {
                self.open_vault_unlock_for(terminus_ui::PendingVaultAction::OpenGroup(
                    group_id.to_string(),
                ));
                break;
            }
        }
        first_error.map_or(Ok(()), Err)
    }

    /// Close the tabs opened from the hosts of group `group_id`.
    pub fn close_group_sessions(&mut self, group_id: &str, clipboard: &mut Clipboard) {
        for id in self.group_host_ids(group_id) {
            self.close_host_sessions(&id, clipboard);
        }
    }

    pub(in crate::screen) fn group_host_ids(&self, group_id: &str) -> Vec<String> {
        hosts::host_ids_in_group(self.host_store.hosts(), group_id)
    }

    pub(in crate::screen) fn close_host_sessions(
        &mut self,
        id: &str,
        clipboard: &mut Clipboard,
    ) {
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
    }
}
