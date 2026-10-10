use super::Application;
use crate::event::{EventPayload, RioEvent, RioEventType};
use crate::scheduler::{TimerId, Topic};
use rio_window::event::ElementState;
use rio_window::window::CursorIcon;
use std::time::Duration;

impl Application<'_> {
    pub(super) fn cursor_hover(
        &mut self,
        window_id: rio_backend::event::WindowId,
        position: rio_window::dpi::PhysicalPosition<f64>,
        x: f64,
        y: f64,
    ) {
        let Some(route) = self.router.routes.get_mut(&window_id) else {
            return;
        };
        // Terminus chrome hover: the host rows and the add-host
        // button highlight under the pointer. Only the highlight
        // is UI-side, but the panel reflows nothing, so a plain
        // overlay redraw does.
        if !route.window.screen.renderer.command_palette.is_enabled()
            && !route.window.screen.renderer.search.is_active()
        {
            let scale = route.window.screen.sugarloaf.scale_factor();
            let lx = x as f32 / scale;
            let ly = y as f32 / scale;
            let mut chrome_dirty = false;
            if route.window.screen.handle_sftp_hover(lx, ly) {
                chrome_dirty = true;
            }
            let sftp_drag_active = route.window.screen.sftp.is_some()
                && route.window.screen.mouse.left_button_state == ElementState::Pressed;
            if sftp_drag_active && route.window.screen.handle_sftp_drag_move(lx, ly) {
                chrome_dirty = true;
            }
            let host_drag_active = route.window.screen.chrome.panel.host_drag.is_some()
                && route.window.screen.mouse.left_button_state == ElementState::Pressed;
            if host_drag_active {
                chrome_dirty = route.window.screen.chrome_drag_move(lx, ly);
            } else if route.window.screen.chrome_hover(lx, ly) {
                chrome_dirty = true;
            }
            let over_view = route.window.screen.view_owns(lx, ly);
            if over_view {
                let dragging =
                    route.window.screen.mouse.left_button_state == ElementState::Pressed;
                let input = terminus_ui::screens::ViewInput::Move {
                    x: lx,
                    y: ly,
                    dragging,
                };
                if route
                    .window
                    .screen
                    .view_input(input, &mut self.router.clipboard)
                {
                    chrome_dirty = true;
                }
            }
            // Overlay dialogs own the pointer; prefer chrome over SFTP.
            let icon = if route.window.screen.chrome_overlay_dialog_open() {
                route
                    .window
                    .screen
                    .chrome_cursor_at(lx, ly)
                    .or_else(|| route.window.screen.sftp_cursor_at(lx, ly))
            } else {
                let view_icon = route.window.screen.view_cursor_at(lx, ly);
                view_icon
                    .or_else(|| route.window.screen.sftp_cursor_at(lx, ly))
                    .or_else(|| route.window.screen.chrome_cursor_at(lx, ly))
            };
            if let Some(icon) = icon {
                route.window.winit_window.set_cursor(icon);
            }
            if chrome_dirty {
                // UI-only change: `request_redraw` alone leaves the
                // framebuffer untouched, because the renderer gates
                // on the context being dirty.
                route.request_overlay_redraw();
            }
            // Host/group drag owns the pointer until release — same
            // contract as the tab strip. Falling through would keep
            // extending a terminal selection under the open LMB.
            if host_drag_active {
                return;
            }
            // A view covering the terminal keeps the pointer too.
            let shell = &route.window.screen.chrome.shell;
            if !shell.view().shows_terminal() && shell.layout().main.contains(lx, ly) {
                if route.window.screen.chrome_cursor_at(lx, ly).is_none()
                    && route.window.screen.sftp_cursor_at(lx, ly).is_none()
                {
                    route.window.winit_window.set_cursor(CursorIcon::Default);
                }
                return;
            }
        }

        if route.window.screen.mouse.left_button_state == ElementState::Pressed
            && route
                .window
                .screen
                .renderer
                .island
                .as_ref()
                .is_some_and(|i| i.is_dragging())
        {
            let scale = route.window.screen.sugarloaf.scale_factor();
            route.window.screen.handle_tab_drag_move(x as f32 / scale);
            route.window.winit_window.set_cursor(CursorIcon::Default);
            route.request_redraw();
            return;
        }

        if route.window.screen.update_close_button_hover(x, y) {
            route.request_overlay_redraw();
        }

        // The macOS full-size content view keeps this band as custom
        // window chrome even when hide-if-single hides the island.
        // Other platforms only reserve it while the island is drawn.
        use crate::renderer::island::CONTEXT_BAR_HEIGHT;
        let scale_factor = route.window.screen.sugarloaf.scale_factor();
        let island_height_px = (CONTEXT_BAR_HEIGHT * scale_factor) as f64;
        let num_tabs = route.window.screen.ctx().len();
        let nav = &route.window.screen.renderer.navigation;
        if nav.chrome_band_reserved(num_tabs) && y <= island_height_px {
            let over_chrome = route
                .window
                .screen
                .chrome_cursor_at(x as f32 / scale_factor, y as f32 / scale_factor)
                .is_some();
            if !over_chrome {
                route.window.winit_window.set_cursor(CursorIcon::Default);
            }
            return;
        }

        // Handle scrollbar drag
        if route.window.screen.renderer.scrollbar.is_dragging() {
            let scale = route.window.screen.sugarloaf.scale_factor();
            let mouse_y = y as f32 / scale;
            route.window.screen.handle_scrollbar_drag(mouse_y);
            route.window.winit_window.set_cursor(CursorIcon::Default);
            route.request_redraw();
            return;
        }

        // Handle panel border resize
        if route.window.screen.resize_state.is_some() {
            let state = route.window.screen.resize_state.unwrap();
            let current_pos = match state.border.direction {
                crate::layout::BorderDirection::Vertical => x as f32,
                crate::layout::BorderDirection::Horizontal => y as f32,
            };
            let delta = current_pos - state.start_pos;
            let border = state.border;
            let original_sizes = state.original_sizes;
            route
                .window
                .screen
                .context_manager
                .current_grid_mut()
                .resize_border(
                    &border,
                    original_sizes,
                    delta,
                    &mut route.window.screen.sugarloaf,
                );
            // Dragging a split divider displaces panel origins;
            // that is layout, not cursor travel.
            route.window.screen.renderer.trail_cursor.snap();
            let cursor = match border.direction {
                crate::layout::BorderDirection::Vertical => CursorIcon::ColResize,
                crate::layout::BorderDirection::Horizontal => CursorIcon::RowResize,
            };
            route.window.winit_window.set_cursor(cursor);
            route.window.screen.context_manager.request_render();
            route.request_redraw();
            return;
        }

        // Check if hovering over a panel border
        {
            let grid = route.window.screen.context_manager.current_grid();
            if let Some(border) = grid.find_border_at_position(x as f32, y as f32) {
                let cursor = match border.direction {
                    crate::layout::BorderDirection::Vertical => CursorIcon::ColResize,
                    crate::layout::BorderDirection::Horizontal => CursorIcon::RowResize,
                };
                route.window.winit_window.set_cursor(cursor);
                route.window.screen.mouse.on_border = true;
                return;
            }
        }

        // Check if hovering over scrollbar
        if route.window.screen.is_hovering_scrollbar() {
            route.window.winit_window.set_cursor(CursorIcon::Default);
            return;
        }

        // Track leaving a border to force cursor reset below
        let was_on_border = route.window.screen.mouse.on_border;
        route.window.screen.mouse.on_border = false;

        let lmb_pressed =
            route.window.screen.mouse.left_button_state == ElementState::Pressed;
        let rmb_pressed =
            route.window.screen.mouse.right_button_state == ElementState::Pressed;

        let has_selection = !route.window.screen.selection_is_empty();
        if has_selection && (lmb_pressed || rmb_pressed) {
            // Only start the timer when the mouse enters the scroll
            // zone. Once running, the tick reads mouse.raw_y each
            // iteration so it keeps scrolling after CursorMoved
            // stops (mouse left window). Cancelled on button release.
            let delta = route.window.screen.selection_scroll_delta(position.y);
            if delta != 0 {
                let scroll_timer_id = route.window.screen.ctx().current_route();
                let timer_id = TimerId::new(Topic::SelectionScrolling, scroll_timer_id);
                if !self.scheduler.scheduled(timer_id) {
                    let event = EventPayload::new(
                        RioEventType::Rio(RioEvent::SelectionScrollTick),
                        window_id,
                    );
                    self.scheduler.schedule(
                        event,
                        Duration::from_millis(15),
                        true,
                        timer_id,
                    );
                }
            }
        }

        let display_offset = route.window.screen.display_offset();
        let point = route.window.screen.mouse_position(display_offset);

        // Compare *cell* coordinates, not pixel coordinates, so
        // subpixel HiDPI jitter inside the same cell doesn't
        // re-fire hint / OSC-8 / hyperlink work every event.
        let prev_cell = route.window.screen.mouse.last_cell;
        let cell_changed = prev_cell != Some(point);
        route.window.screen.mouse.last_cell = Some(point);

        let inside_text_area = route.window.screen.contains_point(x, y);
        let square_side = route.window.screen.side_by_pos(x);

        // If the cursor hasn't changed cells, do nothing.
        // Force update when transitioning off a border so the cursor resets.
        if !cell_changed
            && !was_on_border
            && route.window.screen.mouse.square_side == square_side
            && route.window.screen.mouse.inside_text_area == inside_text_area
        {
            return;
        }

        // Skip hint/hyperlink highlighting during active selection
        // drag to avoid unnecessary terminal locks and regex matching.
        let is_selecting = (lmb_pressed || rmb_pressed)
            && (route.window.screen.modifiers.state().shift_key()
                || !route.window.screen.mouse_mode());

        if !is_selecting {
            let hint_changed = route.window.screen.update_highlighted_hints();
            let scale = route.window.screen.sugarloaf.scale_factor();
            let lx = x as f32 / scale;
            let ly = y as f32 / scale;
            let icon = if route.window.screen.chrome_overlay_dialog_open() {
                route
                    .window
                    .screen
                    .chrome_cursor_at(lx, ly)
                    .or_else(|| route.window.screen.sftp_cursor_at(lx, ly))
                    .unwrap_or_else(|| route.window.screen.mouse_cursor_icon())
            } else {
                route
                    .window
                    .screen
                    .sftp_cursor_at(lx, ly)
                    .or_else(|| route.window.screen.chrome_cursor_at(lx, ly))
                    .unwrap_or_else(|| route.window.screen.mouse_cursor_icon())
            };
            route.window.winit_window.set_cursor(icon);

            if hint_changed {
                route.window.screen.context_manager.request_render();
            }
        }

        route.window.screen.mouse.inside_text_area = inside_text_area;
        route.window.screen.mouse.square_side = square_side;

        if is_selecting {
            route.window.screen.update_selection(point, square_side);
            route.window.screen.context_manager.request_render();
        } else if cell_changed && route.window.screen.has_mouse_motion_and_drag() {
            if lmb_pressed {
                // A latched hint click hides its press and release
                // from the application; a drag report leaking out
                // mid-click would arrive with no press around it.
                if route.window.screen.mouse.hint_click_latched.is_none() {
                    route.window.screen.mouse_report(32, ElementState::Pressed);
                }
            } else if route.window.screen.mouse.middle_button_state
                == ElementState::Pressed
            {
                route.window.screen.mouse_report(33, ElementState::Pressed);
            } else if route.window.screen.mouse.right_button_state
                == ElementState::Pressed
            {
                route.window.screen.mouse_report(34, ElementState::Pressed);
            } else if route.window.screen.has_mouse_motion() {
                route.window.screen.mouse_report(35, ElementState::Pressed);
            }
        }
    }
}
