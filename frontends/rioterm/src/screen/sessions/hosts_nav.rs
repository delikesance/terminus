//! Keyboard host navigation: `SelectNextHost` / `SelectPrevHost` bring
//! forward the last tab of the neighbouring host (sidebar order), and
//! `sync_sidebar_selection` restores the view that host was last on.

use super::super::Screen;
use rio_backend::clipboard::Clipboard;

impl Screen<'_> {
    /// Go to the next (or previous) host that has an open tab. A no-op with
    /// fewer than two such hosts.
    pub(in crate::screen) fn select_adjacent_host(
        &mut self,
        forward: bool,
        clipboard: &mut Clipboard,
    ) {
        let order: Vec<String> =
            self.chrome.panel.host_ids().map(str::to_string).collect();
        let order: Vec<&str> = order.iter().map(String::as_str).collect();
        let Some(idx) = self.context_manager.neighbour_host_tab(&order, forward) else {
            return;
        };
        self.stop_hint_mode_if_active();
        self.cancel_search(clipboard);
        self.clear_selection();
        let old = self.context_manager.current_index();
        self.context_manager.set_current(idx);
        self.switch_visible_context(old, idx);
        self.mark_dirty();
        self.sync_sidebar_selection();
        self.leave_settings_or_home();
    }
}
