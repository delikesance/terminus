use super::Application;
use crate::ime::{Preedit, PreeditCursor};
use crate::renderer::utils::update_colors_based_on_theme;
use crate::router::routes::RoutePath;
use crate::scheduler::{TimerId, Topic};
use crate::screen::touch::on_touch;
use rio_window::event::{ElementState, Ime, WindowEvent};
use rio_window::event_loop::ActiveEventLoop;
use rio_window::window::WindowId;

impl Application<'_> {
    pub(super) fn handle_window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        // Ignore all events we do not care about.
        if Self::skip_window_event(&event) {
            return;
        }

        // The event loop keys on rio-window's id; the router keys on the
        // core's `WindowId`. Convert once at this boundary.
        let window_id: rio_backend::event::WindowId = window_id.into();

        let route = match self.router.routes.get_mut(&window_id) {
            Some(window) => window,
            None => return,
        };

        match event {
            WindowEvent::CloseRequested => {
                self.close_window(event_loop, window_id);
            }

            WindowEvent::ModifiersChanged(modifiers) => {
                route.window.screen.set_modifiers(modifiers);

                // Hint mods (cmd on macOS) are pressed with the pointer
                // already parked over the link, and `CursorMoved` only
                // recomputes hints when the pointer crosses a cell
                // boundary. Without refreshing here the link is never
                // highlighted, so the click that follows has nothing to
                // activate. The set half only runs with the pointer inside
                // the text area (a clamped chrome position must not light
                // up a link it is not over), but the clear half always
                // runs: a highlight left behind would outlive its modifier
                // and hijack the next plain click.
                if route.path == RoutePath::Terminal
                    && (if route.window.screen.mouse.inside_text_area {
                        route.window.screen.update_highlighted_hints()
                    } else {
                        route.window.screen.clear_highlighted_hint()
                    })
                {
                    route
                        .window
                        .winit_window
                        .set_cursor(route.window.screen.mouse_cursor_icon());
                    route.window.screen.context_manager.request_render();
                }
            }

            WindowEvent::MouseInput { state, button, .. } => {
                self.handle_mouse_input(event_loop, window_id, state, button)
            }
            WindowEvent::CursorLeft { .. } => {
                if route.window.screen.clear_close_button_hover() {
                    route.request_redraw();
                }
            }

            WindowEvent::CursorMoved { position, .. } => {
                self.handle_cursor_moved(window_id, position)
            }
            WindowEvent::MouseWheel { delta, phase, .. } => {
                self.handle_mouse_wheel(window_id, delta, phase)
            }
            WindowEvent::KeyboardInput {
                is_synthetic: false,
                event: key_event,
                ..
            } => {
                if route.has_key_wait(&key_event, &mut self.router.clipboard) {
                    if route.path != RoutePath::Terminal
                        && key_event.state == ElementState::Released
                    {
                        // Scheduler must be cleaned after leave the terminal route
                        self.scheduler.unschedule(TimerId::new(
                            Topic::Render,
                            route.window.screen.ctx().current_route(),
                        ));
                    }
                    return;
                }

                route.window.screen.context_manager.set_last_typing();
                route
                    .window
                    .screen
                    .process_key_event(&key_event, &mut self.router.clipboard);
                // `process_key_event` used to call `self.render()` for
                // local-only keystrokes (VI mode, search input, hint
                // mode). Now it just marks `pending_update.set_dirty()`
                // through `mark_dirty`. Request a redraw so the next
                // vsync fires `RedrawRequested` — PTY-bound keystrokes
                // also flow through here but their render is idempotent
                // with the PTY-damage-driven redraw.
                route.request_redraw();

                if key_event.state == ElementState::Released
                    && self.config.hide_cursor_when_typing
                {
                    route.window.winit_window.set_cursor_visible(false);
                }
            }

            WindowEvent::Ime(ime) => {
                // Modal overlays own keyboard input (`modal_owns_input`
                // walks the `has_key_wait` roster): while one is up,
                // composition input must not reach the terminal, but
                // any stored preedit must still CLEAR: a live
                // composition would keep painting into the grid behind
                // the overlay and its key gate would swallow every
                // plain keystroke after the overlay closes. Search
                // stays open to IME (commits route into the search
                // input via `paste`).
                match ime {
                    Ime::Commit(text) => {
                        // Text-input overlays (island rename, palette)
                        // consume commits first, in `has_key_wait`'s
                        // order; other modals swallow them; only a bare
                        // terminal receives the text.
                        if route.overlay_commit_text(&text) {
                            return;
                        }
                        if route.modal_owns_input() {
                            return;
                        }
                        // Don't use bracketed paste for single char input.
                        route.window.screen.paste(&text, text.chars().count() > 1);
                    }
                    Ime::Preedit(text, cursor_offset) => {
                        let preedit = if text.is_empty() || route.modal_owns_input() {
                            None
                        } else {
                            // The platform's `None` means the IME asked
                            // for a hidden caret (candidate paging),
                            // NOT end-of-text, which arrives as an
                            // explicit offset.
                            let cursor = match cursor_offset {
                                Some((start, _)) => PreeditCursor::Byte(start),
                                None => PreeditCursor::Hidden,
                            };
                            Some(Preedit::new(text, cursor))
                        };

                        // `set_ime_preedit` owns the composing side
                        // effects (scroll snap, selection, dirty mark)
                        // and their search-mode exceptions.
                        if route.window.screen.set_ime_preedit(preedit) {
                            route.request_redraw();
                        }
                    }
                    Ime::Enabled => {
                        route.window.screen.ime.set_enabled(true);
                    }
                    Ime::Disabled => {
                        // Disabling wipes any live preedit (input
                        // source switched mid-composition): mark dirty
                        // and repaint like the Preedit arm, or the
                        // block ghosts on screen until unrelated
                        // damage.
                        let had_preedit = route.window.screen.ime.preedit().is_some();
                        route.window.screen.ime.set_enabled(false);
                        if had_preedit {
                            route.window.screen.mark_dirty();
                            route.request_redraw();
                        }
                    }
                }
            }
            WindowEvent::Touch(touch) => {
                on_touch(route, touch, &mut self.router.clipboard);
            }

            WindowEvent::Focused(focused) => {
                if self.config.hide_cursor_when_typing {
                    route.window.winit_window.set_cursor_visible(true);
                }

                let focus_changed = route.window.is_focused != focused;
                route.window.is_focused = focused;

                // Focus is a cheap checkpoint to catch backing-scale changes
                // whose ScaleFactorChanged never arrived (sleep/wake display
                // reconfiguration).
                if focused
                    && route
                        .window
                        .screen
                        .reconcile_scale(&route.window.winit_window)
                {
                    route.window.update_vblank_interval();
                    route.request_redraw();
                } else if focus_changed {
                    route.request_redraw();
                }

                route.window.screen.on_focus_change(focused);
            }

            WindowEvent::Occluded(occluded) => {
                let was_occluded = route.window.is_occluded;
                route.window.is_occluded = occluded;

                // If window was occluded and is now visible, mark for one-time render
                if was_occluded && !occluded {
                    route.window.needs_render_after_occlusion = true;
                    // Same checkpoint as focus: the un-occlusion after wake
                    // is often the first event the window receives.
                    if route
                        .window
                        .screen
                        .reconcile_scale(&route.window.winit_window)
                    {
                        route.window.update_vblank_interval();
                    }
                    // An idle terminal produces no PTY traffic to trigger the
                    // deferred post-occlusion render; request it directly.
                    route.request_redraw();
                }
            }

            WindowEvent::ThemeChanged(new_theme) => {
                if self.config.force_theme.is_some() {
                    return;
                }
                update_colors_based_on_theme(&mut self.config, Some(new_theme));
                route.window.screen.update_config(
                    &self.config,
                    &self.router.font_library,
                    false,
                );
                route.window.configure_window(&self.config);
                route.request_redraw();
            }

            WindowEvent::DroppedFile(path) => {
                if route.window.screen.renderer.assistant.is_active() {
                    return;
                }

                route.window.screen.drop_file(&path);
                route.request_overlay_redraw();
            }

            WindowEvent::Resized(new_size) => {
                if new_size.width == 0 || new_size.height == 0 {
                    return;
                }

                route.window.screen.resize(new_size);
                route.request_redraw();
            }

            WindowEvent::ScaleFactorChanged {
                inner_size_writer: _,
                scale_factor,
            } => {
                let scale = scale_factor as f32;
                route
                    .window
                    .screen
                    .set_scale(scale, route.window.winit_window.inner_size());
                route.window.update_vblank_interval();
                route.request_redraw();
            }

            WindowEvent::RedrawRequested => self.handle_redraw(event_loop, window_id),
            _ => {}
        }
    }
}
