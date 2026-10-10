use super::Application;
use crate::event::{EventPayload, RioEvent, RioEventType};
use crate::renderer::utils::update_colors_based_on_theme;
use crate::scheduler::{TimerId, Topic};
use rio_window::event_loop::ActiveEventLoop;
use std::time::Duration;

impl Application<'_> {
    pub(super) fn handle_user_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: rio_backend::event::WindowId,
        payload: RioEventType,
    ) {
        match payload {
            RioEventType::Rio(RioEvent::SystemResumed) => self.rebuild_gpu_caches(),
            RioEventType::Rio(RioEvent::Render) => {
                if let Some(route) = self.router.routes.get_mut(&window_id) {
                    // Skip rendering for unfocused windows if configured
                    if self.config.renderer.disable_unfocused_render
                        && !route.window.is_focused
                    {
                        return;
                    }

                    // Skip rendering for occluded windows if configured, unless we need to render after occlusion
                    if self.config.renderer.disable_occluded_render
                        && route.window.is_occluded
                        && !route.window.needs_render_after_occlusion
                    {
                        return;
                    }

                    // Clear the one-time render flag if it was set
                    if route.window.needs_render_after_occlusion {
                        route.window.needs_render_after_occlusion = false;
                    }

                    route.request_redraw();
                }
            }
            RioEventType::Rio(RioEvent::RenderRoute(route_id)) => {
                if self.config.renderer.strategy.is_event_based() {
                    if let Some(route) = self.router.routes.get_mut(&window_id) {
                        // Skip rendering for unfocused windows if configured
                        if self.config.renderer.disable_unfocused_render
                            && !route.window.is_focused
                        {
                            if route.window.screen.renderer.scrollbar.needs_redraw() {
                                route.request_redraw();
                            }
                            return;
                        }

                        // Skip rendering for occluded windows if configured, unless we need to render after occlusion
                        if self.config.renderer.disable_occluded_render
                            && route.window.is_occluded
                            && !route.window.needs_render_after_occlusion
                        {
                            return;
                        }

                        // Clear the one-time render flag if it was set
                        if route.window.needs_render_after_occlusion {
                            route.window.needs_render_after_occlusion = false;
                        }

                        // Mark the renderable content as needing to render
                        if let Some(ctx_item) =
                            route.window.screen.ctx_mut().get_by_route_id(route_id)
                        {
                            ctx_item.val.renderable_content.pending_update.set_dirty();
                        }

                        // Check if we need to throttle based on timing
                        if let Some(wait_duration) = route.window.wait_until() {
                            // We need to wait before rendering again
                            let timer_id = TimerId::new(Topic::RenderRoute, route_id);
                            let event = EventPayload::new(
                                RioEventType::Rio(RioEvent::Render),
                                window_id,
                            );

                            // Only schedule if not already scheduled
                            if !self.scheduler.scheduled(timer_id) {
                                self.scheduler.schedule(
                                    event,
                                    wait_duration,
                                    false,
                                    timer_id,
                                );
                            }
                        } else {
                            // We can render immediately
                            route.request_redraw();
                        }
                    }
                }
            }

            RioEventType::Rio(RioEvent::TerminalDamaged(route_id)) => {
                if self.config.renderer.strategy.is_event_based() {
                    if let Some(route) = self.router.routes.get_mut(&window_id) {
                        if self.config.renderer.disable_unfocused_render
                            && !route.window.is_focused
                        {
                            return;
                        }
                        if self.config.renderer.disable_occluded_render
                            && route.window.is_occluded
                            && !route.window.needs_render_after_occlusion
                        {
                            return;
                        }

                        if let Some(ctx_item) =
                            route.window.screen.ctx_mut().get_by_route_id(route_id)
                        {
                            // Just mark dirty — damage will be extracted from
                            // the terminal when the renderer locks it.
                            ctx_item.val.renderable_content.pending_update.set_dirty();
                            route.request_redraw();
                        }
                    }
                }
            }
            RioEventType::Rio(RioEvent::UpdateGraphics { route_id, queues }) => {
                use rio_backend::sugarloaf::route_image_key;
                if let Some(route) = self.router.routes.get_mut(&window_id) {
                    // A batch the VT thread queued before the user
                    // closed its tab or split would otherwise land
                    // under a route nothing will ever release.
                    if route
                        .window
                        .screen
                        .context_manager
                        .get_by_route_id(route_id)
                        .is_none()
                    {
                        return;
                    }

                    // Process graphics directly in sugarloaf
                    let sugarloaf = &mut route.window.screen.sugarloaf;

                    // Atlas graphics (sixel/iTerm2) share the per-image
                    // texture store with kitty images, in a disjoint key
                    // namespace.
                    for graphic_data in queues.pending {
                        let key = route_image_key(
                            route_id,
                            crate::renderer::atlas_image_key(graphic_data.id.get()),
                        );
                        sugarloaf.image_data.insert(
                            key,
                            rio_backend::sugarloaf::GraphicDataEntry::from_graphic_data(
                                graphic_data,
                            ),
                        );
                    }

                    // Removals arrive as final image keys (atlas refs
                    // dropped off scrollback, kitty evictions) and free
                    // both the pixel store and the cached GPU texture.
                    // They run before the kitty uploads so a batch that
                    // frees a key and resends it keeps the new pixels.
                    for key in queues.remove_queue {
                        sugarloaf.remove_image(route_image_key(route_id, key));
                    }

                    // Image textures (kitty) → separate store, no clone
                    for (image_id, graphic_data) in queues.pending_images {
                        sugarloaf.image_data.insert(
                            route_image_key(
                                route_id,
                                crate::renderer::kitty_image_key(image_id),
                            ),
                            rio_backend::sugarloaf::GraphicDataEntry::from_graphic_data(
                                graphic_data,
                            ),
                        );
                    }

                    // Mark the panel dirty: the renderer skips non-dirty
                    // panels, so a bare redraw after the pixels arrive
                    // would no-op and leave the image blank until the
                    // next unrelated damage.
                    if let Some(ctx_item) =
                        route.window.screen.ctx_mut().get_by_route_id(route_id)
                    {
                        ctx_item.val.renderable_content.pending_update.set_dirty();
                    }

                    // Request a redraw to display the updated graphics
                    route.request_redraw();
                }
            }
            RioEventType::Rio(RioEvent::PrepareUpdateConfig) => {
                let timer_id = TimerId::new(Topic::UpdateConfig, 0);
                let event = EventPayload::new(
                    RioEventType::Rio(RioEvent::UpdateConfig),
                    window_id,
                );

                if !self.scheduler.scheduled(timer_id) {
                    self.scheduler.schedule(
                        event,
                        Duration::from_millis(250),
                        false,
                        timer_id,
                    );
                }
            }
            RioEventType::Rio(RioEvent::ReportToAssistant(error)) => {
                if let Some(route) = self.router.routes.get_mut(&window_id) {
                    route.report_error(&error);
                }
            }
            RioEventType::Rio(RioEvent::UpdateConfig) => {
                let (config, config_error) = match rio_backend::config::Config::try_load()
                {
                    Ok(config) => (config, None),
                    Err(error) => (rio_backend::config::Config::default(), Some(error)),
                };

                let has_font_updates = self.config.fonts != config.fonts;
                let has_binding_updates = self.config.bindings != config.bindings;

                let font_library_errors = if has_font_updates {
                    let new_font_library = rio_backend::sugarloaf::font::FontLibrary::new(
                        config.fonts.to_owned(),
                    );
                    *self.router.font_library = new_font_library.0;
                    new_font_library.1
                } else {
                    None
                };

                self.config = config;

                // Dropping the old manager unregisters its hotkeys, so
                // ToggleQuake binding edits apply without restarting.
                if has_binding_updates {
                    self.setup_quake_hotkey();
                }

                let mut has_checked_adaptive_colors = false;
                for (_id, route) in self.router.routes.iter_mut() {
                    // Apply system theme to ensure colors are consistent
                    if !has_checked_adaptive_colors {
                        let system_theme = event_loop.system_theme();
                        let theme = self
                            .config
                            .force_theme
                            .map(|t| t.to_window_theme())
                            .or(system_theme);
                        update_colors_based_on_theme(&mut self.config, theme);
                        has_checked_adaptive_colors = true;
                    }

                    if has_font_updates {
                        if let Some(ref err) = font_library_errors {
                            route
                                .window
                                .screen
                                .context_manager
                                .report_error_fonts_not_found(
                                    err.fonts_not_found.clone(),
                                );
                        }
                    }

                    route.update_config(
                        &self.config,
                        &self.router.font_library,
                        has_font_updates,
                    );
                    route.window.configure_window(&self.config);

                    if let Some(error) = &config_error {
                        route.report_error(&error.to_owned().into());
                    } else {
                        route.clear_errors();
                    }

                    route.request_redraw();
                }
            }
            RioEventType::Rio(RioEvent::Exit | RioEvent::Quit) => {
                if let Some(route) = self.router.routes.get_mut(&window_id) {
                    if self.config.confirm_before_quit {
                        route.confirm_quit();
                    } else {
                        route.quit();
                    }
                }
            }
            other => self.user_event_terminal(event_loop, window_id, other),
        }
    }
}
