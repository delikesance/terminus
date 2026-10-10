use super::Application;
use crate::scheduler::{TimerId, Topic};
use rio_backend::clipboard::ClipboardType;
use rio_window::event::{ElementState, MouseButton};
use rio_window::window::CursorIcon;
use terminus_ui::chrome::ChromeAction;

impl Application<'_> {
    pub(super) fn handle_mouse_release(
        &mut self,
        window_id: rio_backend::event::WindowId,
        button: MouseButton,
    ) {
        let Some(route) = self.router.routes.get_mut(&window_id) else {
            return;
        };
        // Stop selection auto-scroll on button release.
        if let MouseButton::Left | MouseButton::Right = button {
            let scroll_timer_id = route.window.screen.ctx().current_route();
            let timer_id = TimerId::new(Topic::SelectionScrolling, scroll_timer_id);
            self.scheduler.unschedule(timer_id);
        }

        if button == MouseButton::Left {
            let scale = route.window.screen.sugarloaf.scale_factor();
            let mx = route.window.screen.mouse.x as f32 / scale;
            let my = route.window.screen.mouse.y as f32 / scale;
            if route.window.screen.view_owns(mx, my)
                && route.window.screen.chrome.panel.host_drag.is_none()
            {
                let input = terminus_ui::screens::ViewInput::Release { x: mx, y: my };
                if route
                    .window
                    .screen
                    .view_input(input, &mut self.router.clipboard)
                {
                    route.request_redraw();
                }
                return;
            }
        }

        // SFTP file drag-drop between panes.
        if button == MouseButton::Left && route.window.screen.sftp.is_some() {
            let scale = route.window.screen.sugarloaf.scale_factor();
            let mx = route.window.screen.mouse.x as f32 / scale;
            let my = route.window.screen.mouse.y as f32 / scale;
            if route.window.screen.handle_sftp_drag_release(mx, my) {
                route.request_redraw();
                return;
            }
        }

        if button == MouseButton::Left
            && route
                .window
                .screen
                .renderer
                .island
                .as_ref()
                .is_some_and(|i| i.is_dragging())
        {
            let started = route.window.screen.handle_tab_drag_release();
            if started {
                route.request_redraw();
                return;
            }
        }

        // Host→group drag / deferred OpenHost on click.
        if button == MouseButton::Left
            && route.window.screen.chrome.panel.host_drag.is_some()
        {
            let scale = route.window.screen.sugarloaf.scale_factor();
            let mx = route.window.screen.mouse.x as f32 / scale;
            let my = route.window.screen.mouse.y as f32 / scale;
            let action = route.window.screen.chrome_release(mx, my);
            match action {
                ChromeAction::OpenHost(id) => {
                    match route
                        .window
                        .screen
                        .open_host_session(&id, &mut self.router.clipboard)
                    {
                        Ok(()) => {
                            route.window.screen.chrome.panel.error = None;
                            route.window.screen.chrome.panel.notice = None;
                        }
                        Err(err) => {
                            route.window.screen.chrome.panel.error = Some(err);
                        }
                    }
                    route.request_overlay_redraw();
                    return;
                }
                ChromeAction::ToggleGroup(id) => {
                    route.window.screen.chrome.toggle_group_collapsed(&id);
                    let _ = route.window.screen.pump_chrome();
                    route.request_overlay_redraw();
                    return;
                }
                ChromeAction::SetHostGroup { .. }
                | ChromeAction::ReorderHost { .. }
                | ChromeAction::ReorderGroup { .. } => {
                    route.window.screen.apply_host_drag_action(action);
                    route.request_overlay_redraw();
                    return;
                }
                _ => {
                    route.request_overlay_redraw();
                    return;
                }
            }
        }

        if route.window.screen.renderer.scrollbar.is_dragging() {
            route.window.screen.handle_scrollbar_release();
            route.request_redraw();
            return;
        }

        if route.window.screen.resize_state.is_some() {
            route.window.screen.resize_state = None;
            route.window.winit_window.set_cursor(CursorIcon::Default);
            return;
        }

        // Consume the press handler's latched hint so press
        // and release always take the same path, even when
        // the hint modifier changed mid-click. The
        // application never sees one without the other.
        let latched_hint = if button == MouseButton::Left {
            route.window.screen.mouse.hint_click_latched.take()
        } else {
            None
        };
        let hint_click = latched_hint.is_some();

        if !route.window.screen.modifiers.state().shift_key()
            && !hint_click
            && route.window.screen.mouse_mode()
        {
            let code = match button {
                MouseButton::Left => 0,
                MouseButton::Middle => 1,
                MouseButton::Right => 2,
                // Can't properly report more than three buttons.
                MouseButton::Back | MouseButton::Forward | MouseButton::Other(_) => {
                    return
                }
            };
            route
                .window
                .screen
                .mouse_report(code, ElementState::Released);
            return;
        }

        // Releasing a drag selection copies it (with
        // copy-on-select) and must not activate a hint
        // sitting under the release point; hints fire on
        // plain clicks only, when no selection exists.
        if route.window.screen.selection_is_empty() {
            if button == MouseButton::Left {
                // Only a latched press opens a link, and only
                // when the release lands on the same span the
                // press did. Mouse mode never turns the drag
                // into a selection, so without the span check
                // a press on one link released over another
                // would open the wrong one; and a press the
                // chrome consumed (which never latches) must
                // not open a highlight it never touched. The
                // latched match is what executes: a modifier
                // change mid-click can swap which hint config
                // the same span resolves to.
                if let Some(latched) = latched_hint {
                    let same_span =
                        route.window.screen.highlighted_hint().is_some_and(|h| {
                            h.text == latched.text
                                && h.start == latched.start
                                && h.end == latched.end
                        });
                    if same_span {
                        route
                            .window
                            .screen
                            .open_latched_hint(latched, &mut self.router.clipboard);
                        // Paint the cleared highlight now: an
                        // action that steals no focus (Copy)
                        // schedules no frame of its own.
                        route.window.screen.context_manager.request_render();
                    }
                }
            }
        } else if matches!(button, MouseButton::Left | MouseButton::Right)
            && self.config.copy_on_select
        {
            route
                .window
                .screen
                .copy_selection(ClipboardType::Clipboard, &mut self.router.clipboard);
        }
    }
}
