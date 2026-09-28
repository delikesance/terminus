//! `Screen` config surface, split out of `screen/mod.rs`.

use super::{window_should_be_opaque, Screen};
use crate::bindings::FontSizeAction;
use crate::context;
use crate::context::renderable::RenderableContent;
use crate::layout::ContextDimension;
use crate::renderer::utils::padding_top_from_config;
use crate::renderer::{island, Renderer};
use rio_backend::config::layout::Margin;
use rio_backend::error::{RioError, RioErrorLevel, RioErrorType};

impl Screen<'_> {
    /// update_config is triggered in any configuration file update
    #[inline]
    pub fn update_config(
        &mut self,
        config: &rio_backend::config::Config,
        font_library: &rio_backend::sugarloaf::font::FontLibrary,
        should_update_font_library: bool,
    ) {
        let num_tabs = self.ctx().len();
        let padding_y_top = padding_top_from_config(
            &config.navigation,
            config.margin.top,
            num_tabs,
            config.window.macos_use_unified_titlebar,
        );
        let padding_y_bottom = config.margin.bottom;
        self.chrome.top_inset = padding_y_top;

        if should_update_font_library {
            self.sugarloaf.update_font(font_library);
            // Caches keyed by font_id would serve the old font's data.
            self.grid_rasterizer.clear_font_caches();
            for grid in self.grids.values_mut() {
                grid.clear_atlas();
            }
        }
        let s = self.sugarloaf.style_mut();
        s.font_size = config.fonts.size;
        s.line_height = config.line_height;

        // `wgpu_backend`, not the bare feature: default Windows builds
        // run the wgpu path too, and skipping this made `[renderer]
        // filters` edits require a restart there.
        #[cfg(wgpu_backend)]
        self.sugarloaf
            .update_filters(config.renderer.filters.as_slice());

        // Rebuild bindings so `[bindings]` edits live-reload like the
        // rest of the config instead of waiting for a new window.
        self.bindings = crate::bindings::default_key_bindings(config);

        // Preserve existing Island (tab state) and update its colors
        let old_island = self.renderer.island.take();
        let was_focused = self.renderer.is_window_focused;
        self.renderer = Renderer::new(config);
        self.renderer.is_window_focused = was_focused;
        if let Some(mut island) = old_island {
            island.update_colors(config.colors.tabs, config.colors.tabs_active);
            island.max_tab_width = config.navigation.max_tab_width;
            self.renderer.island = Some(island);
        }

        let scale = self.sugarloaf.scale_factor();
        for context_grid in self.context_manager.contexts_mut() {
            context_grid.update_line_height(config.line_height);

            context_grid.update_scaled_margin(Margin::new(
                padding_y_top * scale,
                config.margin.right * scale,
                padding_y_bottom * scale,
                (config.margin.left + self.chrome.reserved_width()) * scale,
            ));

            // Update per-panel font size and line height BEFORE
            // update_dimensions — the recompute reads from these
            // fields. `rebaseline_font_size` also re-anchors the
            // "reset" target so the next change_font_size(Reset)
            // returns to the new config size.
            for current_context in context_grid.contexts_mut().values_mut() {
                let current_context = current_context.context_mut();
                current_context
                    .dimension
                    .rebaseline_font_size(config.fonts.size);
                current_context.dimension.line_height = config.line_height;
            }

            context_grid.update_dimensions(&mut self.sugarloaf);

            for current_context in context_grid.contexts_mut().values_mut() {
                let current_context = current_context.context_mut();
                let mut terminal = current_context.terminal.lock();
                current_context.renderable_content =
                    RenderableContent::from_cursor_config(&config.cursor);
                let shape = config.cursor.shape;
                terminal.cursor_shape = shape;
                terminal.default_cursor_shape = shape;
                terminal.blinking_cursor = config.cursor.blinking;
                drop(terminal);
            }
        }

        self.mouse
            .set_multiplier_and_divider(config.scroll.multiplier, config.scroll.divider);

        // Update keyboard config in context manager
        self.context_manager.config.keyboard = config.keyboard.clone();

        // Re-evaluate the opaque flag — toggling `window.opacity` /
        // `window.blur` at runtime should flip the compositor mode.
        self.sugarloaf
            .set_window_opaque(window_should_be_opaque(config));

        self.sugarloaf
            .set_background_color(Some(self.renderer.dynamic_background.1));

        if let Some(image) = &config.window.background_image {
            if let Err(message) = self.sugarloaf.set_background_image(image) {
                self.renderer.assistant.set_error(RioError {
                    level: RioErrorLevel::Warning,
                    report: RioErrorType::BackgroundImageLoadFailure(message),
                });
            }
        } else {
            self.sugarloaf.clear_background_image();
        }

        self.resize_all_contexts();
    }

    #[inline]
    pub fn change_font_size(&mut self, action: FontSizeAction) {
        let dim = &mut self.context_manager.current_mut().dimension;
        let changed = match action {
            FontSizeAction::Increase => dim.increase_font_size(),
            FontSizeAction::Decrease => dim.decrease_font_size(),
            FontSizeAction::Reset => dim.reset_font_size(),
        };
        if !changed {
            return;
        }

        self.context_manager
            .current_grid_mut()
            .update_dimensions(&mut self.sugarloaf);

        self.mark_dirty();
        self.resize_all_contexts();
        // Reflowed cursor displacement is layout, not travel.
        self.renderer.trail_cursor.snap();
    }

    #[inline]
    pub fn resize(&mut self, new_size: rio_window::dpi::PhysicalSize<u32>) -> &mut Self {
        if self
            .context_manager
            .current()
            .renderable_content
            .selection_range
            .is_some()
        {
            self.clear_selection();
        }
        self.sugarloaf.resize(new_size.width, new_size.height);
        let width = new_size.width as f32;
        let height = new_size.height as f32;

        self.context_manager
            .resize_all_grids(width, height, &mut self.sugarloaf);

        // A resize reflows the cursor; that displacement is layout,
        // not travel, and must not animate a smear.
        self.renderer.trail_cursor.snap();

        self
    }

    /// Re-read the window's live scale factor and re-run the rescale path
    /// when it diverged from the one being rendered with. Display
    /// reconfiguration during sleep/wake can change the backing scale
    /// without a `ScaleFactorChanged` ever being delivered (the macOS
    /// producer de-dupes on the numeric value and wake notifications
    /// coalesce), so cheap checkpoints call this instead of trusting
    /// event delivery. Returns whether a rescale ran.
    pub fn reconcile_scale(&mut self, winit_window: &rio_window::window::Window) -> bool {
        let live_scale = winit_window.scale_factor() as f32;
        if live_scale > 0.0
            && (live_scale - self.sugarloaf.scale_factor()).abs() > f32::EPSILON
        {
            self.set_scale(live_scale, winit_window.inner_size());
            return true;
        }
        false
    }

    #[inline]
    pub fn set_scale(
        &mut self,
        new_scale: f32,
        new_size: rio_window::dpi::PhysicalSize<u32>,
    ) -> &mut Self {
        self.sugarloaf.rescale(new_scale);
        self.sugarloaf.resize(new_size.width, new_size.height);

        for context_grid in self.context_manager.contexts_mut() {
            let old_scale = context_grid.current().dimension.dimension.scale.max(1.0);
            let scaled_margin = context_grid.scaled_margin;
            let unscaled_margin = Margin::new(
                scaled_margin.top / old_scale,
                scaled_margin.right / old_scale,
                scaled_margin.bottom / old_scale,
                scaled_margin.left / old_scale,
            );

            context_grid.update_scaled_margin(Margin::new(
                unscaled_margin.top * new_scale,
                unscaled_margin.right * new_scale,
                unscaled_margin.bottom * new_scale,
                unscaled_margin.left * new_scale,
            ));
            context_grid.update_scale(new_scale);

            for context in context_grid.contexts_mut().values_mut() {
                let ctx = context.context_mut();
                ctx.dimension.update_scale(new_scale);
                // Resident GPU cell buffers hold glyphs shaped at the old
                // scale; a scale change that preserves cols/rows produces
                // no terminal damage on its own, so without this the grid
                // geometry updates while the sprites stay stale.
                ctx.renderable_content
                    .pending_update
                    .set_terminal_damage(rio_backend::event::TerminalDamage::Full);
            }

            context_grid.update_dimensions(&mut self.sugarloaf);
        }

        let width = new_size.width as f32;
        let height = new_size.height as f32;

        self.context_manager
            .resize_all_grids(width, height, &mut self.sugarloaf);
        self.mark_dirty();
        // Rescaled cursor displacement is layout, not travel.
        self.renderer.trail_cursor.snap();

        self
    }

    #[inline]
    pub fn resize_all_contexts(&mut self) {
        // whenever a resize update happens: it will stored in
        // the next layout, so once the messenger.send_resize triggers
        // the wakeup from pty it will also trigger a sugarloaf.render()
        // and then eventually a render with the new layout computation.
        for context_grid in self.context_manager.contexts_mut() {
            for context in context_grid.contexts_mut().values_mut() {
                let ctx = context.context_mut();
                let mut terminal = ctx.terminal.lock();
                terminal.resize::<ContextDimension>(ctx.dimension);
                drop(terminal);
                let winsize = crate::renderer::utils::terminal_dimensions(&ctx.dimension);
                let _ = ctx.messenger.send_resize(winsize);
            }
        }
    }
}
