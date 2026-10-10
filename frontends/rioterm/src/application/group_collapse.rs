use super::Application;

impl Application<'_> {
    /// Lazily initialise `host_persistence` from the first route's worker.
    ///
    /// Called once; subsequent calls are no-ops. The handle is stable after
    /// the first window is created and never re-derived.
    pub(super) fn ensure_persist_handle(&mut self) {
        if self.host_persistence.is_none() {
            if let Some(route) = self.router.routes.values().next() {
                self.host_persistence =
                    Some(route.window.screen.host_store.persist_handle());
            }
        }
    }

    /// Apply the explicit `collapsed` state for `group_id` to every open window,
    /// rebuild each window's sidebar rows, request redraws, and submit the state
    /// to the application's persistence handle.
    ///
    /// This is the single authoritative write path for group-collapse state.
    /// All windows receive the identical final state — no window is treated as
    /// the "primary" source of truth at dispatch time.
    pub(super) fn apply_group_collapse_to_all_windows(
        &mut self,
        group_id: &str,
        collapsed: bool,
    ) {
        for route in self.router.routes.values_mut() {
            if collapsed {
                route
                    .window
                    .screen
                    .chrome
                    .panel
                    .collapsed_groups
                    .insert(group_id.to_string());
            } else {
                route
                    .window
                    .screen
                    .chrome
                    .panel
                    .collapsed_groups
                    .remove(group_id);
            }
            let _ = route.window.screen.pump_chrome();
            route.request_overlay_redraw();
        }
        if let Some(handle) = &self.host_persistence {
            handle.set_group_collapsed(group_id, collapsed);
        }
    }

    /// Poll the persistence handle for a group-collapse outcome.
    ///
    /// On success: no action needed (local state is already correct).
    /// On failure: revert every window to the prior state and show the error
    /// on each window's sidebar panel.
    pub(super) fn poll_group_collapse_outcome(&mut self) {
        let outcome = match self
            .host_persistence
            .as_ref()
            .and_then(|h| h.poll_outcome())
        {
            Some(o) => o,
            None => return,
        };
        if let Some(error) = outcome.error {
            // Prior state is the inverse of what we tried to set.
            let prior = !outcome.collapsed;
            for route in self.router.routes.values_mut() {
                if prior {
                    route
                        .window
                        .screen
                        .chrome
                        .panel
                        .collapsed_groups
                        .insert(outcome.group_id.clone());
                } else {
                    route
                        .window
                        .screen
                        .chrome
                        .panel
                        .collapsed_groups
                        .remove(&outcome.group_id);
                }
                route.window.screen.chrome.panel.error = Some(error.clone());
                let _ = route.window.screen.pump_chrome();
                route.request_overlay_redraw();
            }
        }
    }
}
