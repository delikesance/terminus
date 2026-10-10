use super::Application;
use crate::event::{EventPayload, RioEvent, RioEventType};
use crate::router::Router;
use crate::scheduler::{TimerId, Topic};
use rio_backend::config::colors::ColorRgb;
use rio_window::event_loop::ActiveEventLoop;
use std::time::Duration;

impl Application<'_> {
    pub(super) fn user_event_terminal(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: rio_backend::event::WindowId,
        payload: RioEventType,
    ) {
        match payload {
            RioEventType::Rio(RioEvent::GlyphProtocolInstalled {
                route_id,
                registry,
            }) => {
                if let Some(route) = self.router.routes.get(&window_id) {
                    route
                        .window
                        .screen
                        .sugarloaf
                        .font_library()
                        .install_glyph_registry(route_id, registry);
                }
            }
            RioEventType::Rio(RioEvent::GlyphProtocolQuery { route_id, cp }) => {
                if let Some(route) = self.router.routes.get_mut(&window_id) {
                    use rio_backend::ansi::glyph_protocol::{
                        format_query_response, QueryStatus,
                    };
                    let library = route.window.screen.sugarloaf.font_library();
                    let in_glossary = library
                        .glyph_registry_for(route_id)
                        .is_some_and(|r| r.contains(cp));
                    let in_system = library.covers_codepoint(cp);
                    let status = match (in_glossary, in_system) {
                        (true, true) => QueryStatus::Both,
                        (true, false) => QueryStatus::Glossary,
                        (false, true) => QueryStatus::System,
                        (false, false) => QueryStatus::Free,
                    };
                    let resp = format_query_response(cp, status);
                    if let Some(item) = route
                        .window
                        .screen
                        .context_manager
                        .current_grid_mut()
                        .get_by_route_id(route_id)
                    {
                        item.context_mut().messenger.send_bytes(resp.into_bytes());
                    }
                }
            }
            RioEventType::Rio(RioEvent::ChildExited(route_id, status)) => {
                if let Some(route) = self.router.routes.get_mut(&window_id) {
                    // An SSH tab whose link dropped stays open with a
                    // "Connection lost" card instead of closing.
                    if route.window.screen.note_child_exit(route_id, status) {
                        route.request_overlay_redraw();
                    }
                }
            }
            RioEventType::Rio(RioEvent::CloseTerminal(route_id)) => {
                if let Some(route) = self.router.routes.get_mut(&window_id) {
                    route
                        .window
                        .screen
                        .sugarloaf
                        .font_library()
                        .remove_glyph_registry(route_id);
                    if route.window.screen.is_connection_lost(route_id) {
                        // Kept for its Reconnect card (see `ChildExited`);
                        // closing it is the card's or the tab's ×.
                        route.request_overlay_redraw();
                        return;
                    }
                    // A host session that dies while connecting reports why.
                    route.window.screen.note_session_exit(route_id);

                    if route
                        .window
                        .screen
                        .context_manager
                        .should_close_context_manager(
                            route_id,
                            &mut route.window.screen.sugarloaf,
                        )
                    {
                        self.router.routes.remove(&window_id);

                        // Unschedule pending events.
                        self.scheduler.unschedule_window(route_id);

                        if self.router.routes.is_empty() {
                            event_loop.exit();
                        }
                    } else {
                        let size = route.window.screen.context_manager.len();
                        route.window.screen.resize_top_or_bottom_line(size);
                    }
                }
            }
            RioEventType::Rio(RioEvent::CursorBlinkingChange) => {
                if let Some(route) = self.router.routes.get_mut(&window_id) {
                    route.request_redraw();
                }
            }
            RioEventType::Rio(RioEvent::CursorBlinkingChangeOnRoute(route_id)) => {
                if let Some(route) = self.router.routes.get_mut(&window_id) {
                    if route_id == route.window.screen.ctx().current_route() {
                        // Cursor blink toggles the cursor sprite (a
                        // separate quad), not cell content — so we
                        // signal `CursorOnly` and the GPU emit skips
                        // per-row rebuild while the cursor uniform
                        // updates downstream.
                        route
                            .window
                            .screen
                            .ctx_mut()
                            .current_mut()
                            .renderable_content
                            .pending_update
                            .set_terminal_damage(
                                rio_backend::event::TerminalDamage::CursorOnly,
                            );

                        route.request_redraw();
                    }
                }
            }
            RioEventType::Rio(RioEvent::ProgressReport(report)) => {
                if let Some(route) = self.router.routes.get_mut(&window_id) {
                    if let Some(island) = &mut route.window.screen.renderer.island {
                        island.set_progress_report(report);
                        route.request_redraw();
                    }
                }
            }
            RioEventType::Rio(RioEvent::CommandSubmitted {
                route_id,
                command,
                cwd,
            }) => {
                if let Some(route) = self.router.routes.get_mut(&window_id) {
                    route
                        .window
                        .screen
                        .record_submitted_command(route_id, &command, cwd);
                }
            }
            RioEventType::Rio(RioEvent::SelectionScrollTick) => {
                if let Some(route) = self.router.routes.get_mut(&window_id) {
                    route.window.screen.selection_scroll_tick();
                    route.request_redraw();
                }
            }
            RioEventType::Rio(RioEvent::Bell) => {
                // Handle audio bell
                if self.config.bell.audio {
                    self.handle_audio_bell();
                }
            }
            RioEventType::Rio(RioEvent::DesktopNotification { title, body }) => {
                self.handle_desktop_notification(&title, &body);
            }
            RioEventType::Rio(RioEvent::PrepareRender(millis)) => {
                if let Some(route) = self.router.routes.get(&window_id) {
                    let timer_id = TimerId::new(
                        Topic::Render,
                        route.window.screen.ctx().current_route(),
                    );
                    let event =
                        EventPayload::new(RioEventType::Rio(RioEvent::Render), window_id);

                    if !self.scheduler.scheduled(timer_id) {
                        self.scheduler.schedule(
                            event,
                            Duration::from_millis(millis),
                            false,
                            timer_id,
                        );
                    }
                }
            }
            RioEventType::Rio(RioEvent::PrepareRenderOnRoute(millis, route_id)) => {
                let timer_id = TimerId::new(Topic::ScheduledRenderRoute, route_id);
                let event = EventPayload::new(
                    RioEventType::Rio(RioEvent::RenderRoute(route_id)),
                    window_id,
                );

                if !self.scheduler.scheduled(timer_id) {
                    self.scheduler.schedule(
                        event,
                        Duration::from_millis(millis),
                        false,
                        timer_id,
                    );
                }
            }
            RioEventType::Rio(RioEvent::BlinkCursor(millis, route_id)) => {
                let timer_id = TimerId::new(Topic::CursorBlinking, route_id);
                let event = EventPayload::new(
                    RioEventType::Rio(RioEvent::CursorBlinkingChangeOnRoute(route_id)),
                    window_id,
                );

                if !self.scheduler.scheduled(timer_id) {
                    self.scheduler.schedule(
                        event,
                        Duration::from_millis(millis),
                        false,
                        timer_id,
                    );
                }
            }
            RioEventType::Rio(RioEvent::Title(title)) => {
                if let Some(route) = self.router.routes.get_mut(&window_id) {
                    route.set_window_title(&title);
                }
            }
            RioEventType::Rio(RioEvent::TitleWithSubtitle(title, subtitle)) => {
                if let Some(route) = self.router.routes.get_mut(&window_id) {
                    route.set_window_title(&title);
                    route.set_window_subtitle(&subtitle);
                }
            }
            RioEventType::Rio(RioEvent::UpdateTitles) => {
                self.router.update_titles();
            }
            RioEventType::Rio(RioEvent::MouseCursorDirty) => {
                if let Some(route) = self.router.routes.get_mut(&window_id) {
                    route.window.screen.reset_mouse();
                }
            }
            RioEventType::Rio(RioEvent::Scroll(scroll)) => {
                if let Some(route) = self.router.routes.get_mut(&window_id) {
                    let mut terminal = route
                        .window
                        .screen
                        .context_manager
                        .current_mut()
                        .terminal
                        .lock();
                    terminal.scroll_display(scroll);
                    drop(terminal);
                }
            }
            RioEventType::Rio(RioEvent::ClipboardLoad(
                route_id,
                clipboard_type,
                format,
            )) => {
                let Router {
                    routes, clipboard, ..
                } = &mut self.router;
                if let Some(route) = routes.get_mut(&window_id) {
                    if route.window.is_focused {
                        let text = format(clipboard.get(clipboard_type).as_str());
                        // Route the paste back to the panel that asked for it
                        // (OSC 52 reply), not whichever panel happens to be
                        // focused now.
                        if let Some(item) = route
                            .window
                            .screen
                            .context_manager
                            .get_by_route_id(route_id)
                        {
                            item.val.messenger.send_bytes(text.into_bytes());
                        }
                    }
                }
            }
            RioEventType::Rio(RioEvent::ClipboardStore(clipboard_type, content)) => {
                let Router {
                    routes, clipboard, ..
                } = &mut self.router;
                if let Some(route) = routes.get_mut(&window_id) {
                    if route.window.is_focused {
                        clipboard.set(clipboard_type, content);
                    }
                }
            }
            RioEventType::Rio(RioEvent::PtyWrite(route_id, text)) => {
                if let Some(route) = self.router.routes.get_mut(&window_id) {
                    // Route reply bytes (CSI / OSC responses) back to the
                    // PTY of the panel that emitted them, not whichever
                    // panel happens to be focused.
                    if let Some(item) = route
                        .window
                        .screen
                        .context_manager
                        .get_by_route_id(route_id)
                    {
                        item.val.messenger.send_bytes(text.into_bytes());
                    }
                }
            }
            RioEventType::Rio(RioEvent::TextAreaSizeRequest(route_id, format)) => {
                if let Some(route) = self.router.routes.get_mut(&window_id) {
                    if let Some(item) = route
                        .window
                        .screen
                        .context_manager
                        .get_by_route_id(route_id)
                    {
                        let dimension = item.val.dimension;
                        let text = format(crate::renderer::utils::terminal_dimensions(
                            &dimension,
                        ));
                        item.val.messenger.send_bytes(text.into_bytes());
                    }
                }
            }
            RioEventType::Rio(RioEvent::ColorRequest(route_id, index, format)) => {
                if let Some(route) = self.router.routes.get_mut(&window_id) {
                    // Read the originating panel's terminal colors and
                    // route the reply back to that same panel — color
                    // theme overrides via OSC 4 / OSC 10-19 are
                    // per-context, so reading from `current()` would
                    // mis-report when the user has focused a different
                    // split mid-flight.
                    let renderer_color = route.window.screen.renderer.colors[index];
                    let Some(item) = route
                        .window
                        .screen
                        .context_manager
                        .get_by_route_id(route_id)
                    else {
                        return;
                    };
                    let terminal = item.val.terminal.lock();
                    let color: ColorRgb = match terminal.colors()[index] {
                        Some(color) => ColorRgb::from_color_arr(color),
                        // Ignore cursor color requests unless it was changed.
                        None if index
                            == crate::crosswords::NamedColor::Cursor as usize =>
                        {
                            return
                        }
                        None => ColorRgb::from_color_arr(renderer_color),
                    };
                    drop(terminal);

                    item.val.messenger.send_bytes(format(color).into_bytes());
                }
            }
            other => self.user_event_window(event_loop, window_id, other),
        }
    }
}
