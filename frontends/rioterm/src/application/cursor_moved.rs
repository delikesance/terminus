use super::Application;
use crate::router::routes::RoutePath;
use rio_window::window::CursorIcon;

impl Application<'_> {
    pub(super) fn handle_cursor_moved(
        &mut self,
        window_id: rio_backend::event::WindowId,
        position: rio_window::dpi::PhysicalPosition<f64>,
    ) {
        let Some(route) = self.router.routes.get_mut(&window_id) else {
            return;
        };
        if self.config.hide_cursor_when_typing {
            route.window.winit_window.set_cursor_visible(true);
        }

        let layout = route.window.screen.sugarloaf.window_size();

        // Keep f64 precision all the way to the cell-grid
        // divide. The old `as usize` cast here dropped
        // subpixel info from HiDPI events.
        let x = position.x.clamp(0.0, (layout.width as i32 - 1) as f64);
        let y = position.y.clamp(0.0, (layout.height as i32 - 1) as f64);

        route.window.screen.mouse.x = x;
        route.window.screen.mouse.y = y;
        route.window.screen.mouse.raw_y = position.y;

        // Frameless Linux window: edges and corners own the pointer.
        if let Some(repaint) = route.window.screen.frameless_edge_hover(
            &route.window.winit_window,
            self.router.quake_window_id == Some(window_id),
        ) {
            if repaint {
                route.request_overlay_redraw();
            }
            return;
        }

        if self.cursor_overlays(window_id, x, y) {
            return;
        }

        self.cursor_hover(window_id, position, x, y);
    }

    pub(super) fn cursor_overlays(
        &mut self,
        window_id: rio_backend::event::WindowId,
        x: f64,
        y: f64,
    ) -> bool {
        let Some(route) = self.router.routes.get_mut(&window_id) else {
            return false;
        };
        let layout = route.window.screen.sugarloaf.window_size();
        if route.window.screen.renderer.confirm_quit.is_active() {
            let scale = route.window.screen.sugarloaf.scale_factor();
            let win = (layout.width / scale, layout.height / scale);
            let (lx, ly) = (x as f32 / scale, y as f32 / scale);
            if route
                .window
                .screen
                .renderer
                .confirm_quit
                .hover_at(win, lx, ly)
            {
                route.request_overlay_redraw();
            }
            let on_button = route
                .window
                .screen
                .renderer
                .confirm_quit
                .over_button(win, lx, ly);
            route.window.winit_window.set_cursor(if on_button {
                CursorIcon::Pointer
            } else {
                CursorIcon::Default
            });
            return true;
        }
        if route.path != RoutePath::Terminal {
            route.window.winit_window.set_cursor(CursorIcon::Default);
            return true;
        }

        // Handle assistant overlay hover
        if route.window.screen.renderer.assistant.is_active() {
            let scale = route.window.screen.sugarloaf.scale_factor();
            let win_w = route.window.screen.sugarloaf.window_size().width;
            let mx = x as f32 / scale;
            let my = y as f32 / scale;
            if route
                .window
                .screen
                .renderer
                .assistant
                .hover(mx, my, win_w, scale)
            {
                route.request_overlay_redraw();
            }

            if route
                .window
                .screen
                .renderer
                .assistant
                .hovered_button()
                .is_some()
            {
                route.window.winit_window.set_cursor(CursorIcon::Pointer);
                return true;
            }
            // Errors are modal; a warning lets the pointer through.
            if route.window.screen.renderer.assistant.is_error() {
                route.window.winit_window.set_cursor(CursorIcon::Default);
                return true;
            }
        }

        // Handle command palette hover
        if route.window.screen.renderer.command_palette.is_enabled() {
            let scale = route.window.screen.sugarloaf.scale_factor();
            let win_w = route.window.screen.sugarloaf.window_size().width;
            let mx = x as f32 / scale;
            let my = y as f32 / scale;
            if route
                .window
                .screen
                .renderer
                .command_palette
                .hover(mx, my, win_w, scale)
            {
                route.request_overlay_redraw();
            }
            route.window.winit_window.set_cursor(CursorIcon::Default);
            return true;
        }

        // Handle search overlay hover
        if route.window.screen.renderer.search.is_active() {
            let scale = route.window.screen.sugarloaf.scale_factor();
            let win_w = route.window.screen.sugarloaf.window_size().width;
            let mx = x as f32 / scale;
            let my = y as f32 / scale;
            if route
                .window
                .screen
                .renderer
                .search
                .hover(mx, my, win_w, scale)
            {
                // UI-only change (hover highlight). `set_dirty`
                // passes `Renderer::run`'s per-context gate;
                // the inner damage match hits
                // `(None, None) => TerminalDamage::Noop` so
                // no rows rebuild. The search overlay itself
                // is drawn unconditionally after the per-context
                // loop in `Renderer::run`.
                route
                    .window
                    .screen
                    .ctx_mut()
                    .current_mut()
                    .renderable_content
                    .pending_update
                    .set_dirty();
                route.request_redraw();
            }
        }
        false
    }
}
