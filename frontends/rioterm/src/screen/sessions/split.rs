//! Screen sessions: split.

use super::super::Screen;
use crate::context::next_rich_text_id;
use crate::renderer::utils::padding_top_from_config;
use rio_backend::config::layout::Margin;

use super::{split_target, SplitTarget};

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
        self.split_focused(false);
    }

    pub fn split_down(&mut self) {
        self.split_focused(true);
    }

    /// Split the focused pane. A pane that runs a sidebar row's session
    /// (ssh host, WSL distro) opens the new half on the same row, through
    /// the same launch as opening it again; anything else is a local shell.
    pub(in crate::screen) fn split_focused(&mut self, split_down: bool) {
        let rich_text_id = next_rich_text_id();
        let target = split_target(self.context_manager.current().host_id.as_deref());
        let SplitTarget::Row(id) = target else {
            self.context_manager
                .split(rich_text_id, split_down, &mut self.sugarloaf);
            self.mark_dirty();
            return;
        };

        // Never fall back to a local shell for a host pane: a locked vault
        // or a missing credential is reported instead.
        let (shell, env) = match self.shell_for_row(&id) {
            Ok(launch) => launch,
            Err(err) => {
                self.chrome.panel.error = Some(err);
                self.mark_dirty();
                return;
            }
        };

        let label = self.host_row_label(&id);
        self.begin_session_connecting(&id);
        match self.context_manager.split_with_shell(
            rich_text_id,
            split_down,
            &mut self.sugarloaf,
            shell,
            env,
            Some(id.clone()),
        ) {
            Ok(()) => self.host_store.detect_os(&id),
            Err(err) => {
                self.end_session_connecting();
                self.chrome.panel.error = Some(format!("{label}: {err}"));
            }
        }

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
