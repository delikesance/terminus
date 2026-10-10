use super::Application;
use crate::router::routes::RoutePath;
use rio_window::event::{ElementState, MouseButton};
use rio_window::event_loop::ActiveEventLoop;

impl Application<'_> {
    pub(super) fn handle_mouse_input(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: rio_backend::event::WindowId,
        state: ElementState,
        button: MouseButton,
    ) {
        let Some(route) = self.router.routes.get_mut(&window_id) else {
            return;
        };
        // Frameless Linux window: an edge press starts a resize.
        if state == ElementState::Pressed
            && button == MouseButton::Left
            && route.window.screen.frameless_edge_press(
                &route.window.winit_window,
                self.router.quake_window_id == Some(window_id),
            )
        {
            return;
        }
        if state == ElementState::Pressed
            && button == MouseButton::Left
            && route.window.screen.renderer.confirm_quit.is_active()
        {
            let scale = route.window.screen.sugarloaf.scale_factor();
            let size = route.window.screen.sugarloaf.window_size();
            let win = (size.width / scale, size.height / scale);
            let mouse = &route.window.screen.mouse;
            let (x, y) = (mouse.x as f32 / scale, mouse.y as f32 / scale);
            let outcome = route.window.screen.renderer.confirm_quit.press(win, x, y);
            route.apply_quit_outcome(outcome);
            return;
        }
        if route.path != RoutePath::Terminal
            || route.window.screen.renderer.confirm_quit.is_active()
        {
            if state == ElementState::Pressed
                && button == MouseButton::Left
                && route.window.screen.allow_manual_dragging
            {
                use crate::renderer::island::CONTEXT_BAR_HEIGHT;
                let scale = route.window.screen.sugarloaf.scale_factor();
                if route.window.screen.mouse.y <= (CONTEXT_BAR_HEIGHT * scale) as f64 {
                    let start_drag = {
                        let logical_w =
                            route.window.screen.sugarloaf.window_size().width / scale;
                        let x = route.window.screen.mouse.x as f32 / scale;
                        let y = route.window.screen.mouse.y as f32 / scale;
                        let on_action =
                            crate::renderer::island::title_bar_hit(logical_w, x, y)
                                .is_some();
                        #[cfg(target_os = "windows")]
                        let on_caption =
                            crate::renderer::window_controls::hit_test(logical_w, x, y)
                                .is_some();
                        #[cfg(not(target_os = "windows"))]
                        let on_caption = false;
                        !on_action && !on_caption
                    };
                    if start_drag {
                        let _ = route.window.winit_window.drag_window();
                    }
                }
            }
            if state == ElementState::Pressed {
                let _ = route.window.screen.take_chrome_press();
            } else if state == ElementState::Released && button == MouseButton::Left {
                route.window.screen.mouse.left_button_state = ElementState::Released;
                // A release swallowed here must also drop the hint
                // latch, or a later chrome-consumed press would
                // release against a hint it never landed on.
                route.window.screen.mouse.hint_click_latched = None;
                if let Some(ref mut island) = route.window.screen.renderer.island {
                    island.cancel_drag();
                }
                route.window.screen.renderer.scrollbar.end_drag();
                route.window.screen.resize_state = None;
            }
            return;
        }

        if self.config.hide_cursor_when_typing {
            route.window.winit_window.set_cursor_visible(true);
        }

        match button {
            MouseButton::Left => route.window.screen.mouse.left_button_state = state,
            MouseButton::Middle => route.window.screen.mouse.middle_button_state = state,
            MouseButton::Right => route.window.screen.mouse.right_button_state = state,
            _ => (),
        }

        match state {
            ElementState::Pressed => {
                self.handle_mouse_press(event_loop, window_id, button)
            }
            ElementState::Released => self.handle_mouse_release(window_id, button),
        }
    }
}
