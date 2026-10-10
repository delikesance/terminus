use super::Application;
use crate::router::routes::RoutePath;
use rio_window::event::{MouseScrollDelta, TouchPhase};

impl Application<'_> {
    pub(super) fn handle_mouse_wheel(
        &mut self,
        window_id: rio_backend::event::WindowId,
        delta: MouseScrollDelta,
        phase: TouchPhase,
    ) {
        let Some(route) = self.router.routes.get_mut(&window_id) else {
            return;
        };
        if route.path != RoutePath::Terminal
            || route.window.screen.renderer.confirm_quit.is_active()
        {
            return;
        }

        {
            let scale = route.window.screen.sugarloaf.scale_factor();
            let mx = route.window.screen.mouse.x as f32 / scale;
            let my = route.window.screen.mouse.y as f32 / scale;
            if route.window.screen.view_owns(mx, my) {
                let lines = match delta {
                    MouseScrollDelta::LineDelta(_, lines) => -lines,
                    MouseScrollDelta::PixelDelta(pos) => -(pos.y as f32) / 20.0,
                };
                let input = terminus_ui::screens::ViewInput::Wheel {
                    x: mx,
                    y: my,
                    lines,
                };
                if route
                    .window
                    .screen
                    .view_input(input, &mut self.router.clipboard)
                {
                    route.request_redraw();
                }
                return;
            }
            let shell = &route.window.screen.chrome.shell;
            if !shell.view().shows_terminal()
                && shell.content_rect().contains(mx, my)
                && route.window.screen.sftp.is_none()
            {
                return;
            }
        }

        if route.window.screen.sftp.is_some() {
            let scale = route.window.screen.sugarloaf.scale_factor();
            let mx = route.window.screen.mouse.x as f32 / scale;
            let my = route.window.screen.mouse.y as f32 / scale;
            let lines = match delta {
                MouseScrollDelta::LineDelta(_, lines) => lines,
                MouseScrollDelta::PixelDelta(pos) => (pos.y as f32) / 20.0,
            };
            if route.window.screen.handle_sftp_scroll(mx, my, lines) {
                route.request_redraw();
                return;
            }
        }

        if self.config.hide_cursor_when_typing {
            route.window.winit_window.set_cursor_visible(true);
        }

        match delta {
            MouseScrollDelta::LineDelta(columns, lines) => {
                // The chrome's host list owns the wheel while the
                // pointer is over it; the terminal never sees
                // those notches.
                {
                    let scale = route.window.screen.sugarloaf.scale_factor();
                    let mx = route.window.screen.mouse.x as f32 / scale;
                    let my = route.window.screen.mouse.y as f32 / scale;
                    if route.window.screen.chrome_wheel(mx, my, lines) {
                        route.request_overlay_redraw();
                        return;
                    }
                }

                // One wheel notch is one line/column. Convert
                // with the cell size: scroll() divides the
                // accumulated pixels by it, and converting
                // with font_size (smaller than a cell) made
                // single notches floor to zero lines (#1350).
                let cell = route.window.screen.ctx().current().dimension.dimension;
                if cell.width > 0.0 && cell.height > 0.0 {
                    let new_scroll_px_x = columns * cell.width;
                    let new_scroll_px_y = lines * cell.height;
                    route
                        .window
                        .screen
                        .scroll(new_scroll_px_x as f64, new_scroll_px_y as f64);
                }
            }
            MouseScrollDelta::PixelDelta(mut lpos) => {
                // Touchpads: the host list owns the wheel over
                // the sidebar, in pixels.
                {
                    let scale = route.window.screen.sugarloaf.scale_factor();
                    let mx = route.window.screen.mouse.x as f32 / scale;
                    let my = route.window.screen.mouse.y as f32 / scale;
                    if route.window.screen.chrome_wheel_pixels(
                        mx,
                        my,
                        lpos.y as f32 / scale,
                    ) {
                        route.request_overlay_redraw();
                        return;
                    }
                }
                match phase {
                    TouchPhase::Started => {
                        // Reset offset to zero.
                        route.window.screen.mouse.accumulated_scroll = Default::default();
                    }
                    TouchPhase::Moved => {
                        // When the angle between (x, 0) and (x, y) is lower than ~25 degrees
                        // (cosine is larger that 0.9) we consider this scrolling as horizontal.
                        if lpos.x.abs() / lpos.x.hypot(lpos.y) > 0.9 {
                            lpos.y = 0.;
                        } else {
                            lpos.x = 0.;
                        }

                        route.window.screen.scroll(lpos.x, lpos.y);
                    }
                    _ => (),
                }
            }
        }

        route.request_redraw();
    }
}
