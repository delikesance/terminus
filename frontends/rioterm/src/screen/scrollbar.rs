//! `Screen` scrollbar surface, split out of `screen/mod.rs`.

use super::Screen;
use crate::context;
use crate::crosswords::grid::Scroll;
use crate::crosswords::Mode;
use rio_window::event::ElementState;

impl Screen<'_> {
    pub fn handle_scrollbar_click(&mut self) -> bool {
        let scale_factor = self.sugarloaf.scale_factor();
        let mouse_x = self.mouse.x as f32 / scale_factor;
        let mouse_y = self.mouse.y as f32 / scale_factor;

        let grid = self.context_manager.current_grid_mut();
        let grid_margin = (grid.scaled_margin.left, grid.scaled_margin.top);

        let item = match grid.current_item() {
            Some(item) => item,
            None => return false,
        };

        let panel_rect = item.layout_rect;
        let rich_text_id = item.context().rich_text_id;

        let terminal = item.context().terminal.lock();
        let display_offset = terminal.display_offset();
        let history_size = terminal.history_size();
        let screen_lines = terminal.screen_lines();
        drop(terminal);

        if let Some((grab_offset, geom)) = self.renderer.scrollbar.hit_test(
            mouse_x,
            mouse_y,
            panel_rect,
            scale_factor,
            display_offset,
            history_size,
            screen_lines,
            grid_margin,
        ) {
            self.renderer.scrollbar.start_drag(
                rich_text_id,
                grab_offset,
                &geom,
                history_size,
            );

            // If clicked on track (not on thumb), jump-scroll to that position
            if grab_offset.is_none() {
                if let Some(new_offset) = self.renderer.scrollbar.drag_update(mouse_y) {
                    let mut terminal = self.context_manager.current_mut().terminal.lock();
                    let current = terminal.display_offset();
                    let delta = new_offset as i32 - current as i32;
                    terminal.scroll_display(Scroll::Delta(delta));
                    drop(terminal);
                }
            }
            self.mark_dirty();
            true
        } else {
            false
        }
    }

    pub fn handle_scrollbar_drag(&mut self, mouse_y: f32) -> bool {
        if !self.renderer.scrollbar.is_dragging() {
            return false;
        }

        if let Some(new_offset) = self.renderer.scrollbar.drag_update(mouse_y) {
            let mut terminal = self.context_manager.current_mut().terminal.lock();
            let current = terminal.display_offset();
            let delta = new_offset as i32 - current as i32;
            if delta != 0 {
                terminal.scroll_display(Scroll::Delta(delta));
            }
            drop(terminal);
            self.mark_dirty();
        }
        true
    }

    pub fn handle_scrollbar_release(&mut self) {
        self.renderer.scrollbar.end_drag();
    }

    pub fn is_hovering_scrollbar(&self) -> bool {
        if !self.renderer.scrollbar.is_enabled() {
            return false;
        }
        let scale_factor = self.sugarloaf.scale_factor();
        let mouse_x = self.mouse.x as f32 / scale_factor;
        let mouse_y = self.mouse.y as f32 / scale_factor;

        let grid = self.context_manager.current_grid();
        let grid_margin = (grid.scaled_margin.left, grid.scaled_margin.top);

        let item = match grid.current_item() {
            Some(item) => item,
            None => return false,
        };

        let panel_rect = item.layout_rect;

        let terminal = item.context().terminal.lock();
        let display_offset = terminal.display_offset();
        let history_size = terminal.history_size();
        let screen_lines = terminal.screen_lines();
        drop(terminal);

        self.renderer
            .scrollbar
            .hit_test(
                mouse_x,
                mouse_y,
                panel_rect,
                scale_factor,
                display_offset,
                history_size,
                screen_lines,
                grid_margin,
            )
            .is_some()
    }

    #[inline]
    pub fn scroll(&mut self, new_scroll_x_px: f64, new_scroll_y_px: f64) {
        // Scrolling slides different text under the pointer while the
        // viewport cell stays the same, so the hover-probe dedup key
        // must not suppress the next probe.
        self.last_hint_probe = None;

        let dim = self.context_manager.current().dimension.dimension;
        let width = dim.width as f64;
        let height = dim.height as f64;
        let mode = self.get_mode();

        const MOUSE_WHEEL_UP: u8 = 64;
        const MOUSE_WHEEL_DOWN: u8 = 65;
        const MOUSE_WHEEL_LEFT: u8 = 66;
        const MOUSE_WHEEL_RIGHT: u8 = 67;

        if mode.intersects(Mode::MOUSE_MODE) && !mode.contains(Mode::VI) {
            self.mouse.accumulated_scroll.x += new_scroll_x_px;
            self.mouse.accumulated_scroll.y += new_scroll_y_px;

            let code = if new_scroll_y_px > 0. {
                MOUSE_WHEEL_UP
            } else {
                MOUSE_WHEEL_DOWN
            };
            let lines = (self.mouse.accumulated_scroll.y / height).abs() as usize;

            for _ in 0..lines {
                self.mouse_report(code, ElementState::Pressed);
            }

            let code = if new_scroll_x_px > 0. {
                MOUSE_WHEEL_LEFT
            } else {
                MOUSE_WHEEL_RIGHT
            };
            let columns = (self.mouse.accumulated_scroll.x / width).abs() as usize;

            for _ in 0..columns {
                self.mouse_report(code, ElementState::Pressed);
            }
        } else if mode.contains(Mode::ALT_SCREEN | Mode::ALTERNATE_SCROLL)
            && !self.modifiers.state().shift_key()
        {
            self.mouse.accumulated_scroll.x +=
                (new_scroll_x_px * self.mouse.multiplier) / self.mouse.divider;
            self.mouse.accumulated_scroll.y +=
                (new_scroll_y_px * self.mouse.multiplier) / self.mouse.divider;

            // The chars here are the same as for the respective arrow keys.
            let line_cmd = if new_scroll_y_px > 0. { b'A' } else { b'B' };
            let column_cmd = if new_scroll_x_px > 0. { b'D' } else { b'C' };

            let lines = (self.mouse.accumulated_scroll.y / height).abs() as usize;

            let columns = (self.mouse.accumulated_scroll.x / width).abs() as usize;

            let mut content = Vec::with_capacity(3 * (lines + columns));

            for _ in 0..lines {
                content.push(0x1b);
                content.push(b'O');
                content.push(line_cmd);
            }

            for _ in 0..columns {
                content.push(0x1b);
                content.push(b'O');
                content.push(column_cmd);
            }

            if !content.is_empty() {
                self.ctx_mut().current_mut().messenger.send_write(content);
            }
        } else {
            self.mouse.accumulated_scroll.y +=
                (new_scroll_y_px * self.mouse.multiplier) / self.mouse.divider;
            let lines = (self.mouse.accumulated_scroll.y / height) as i32;

            if lines != 0 {
                let current = self.context_manager.current_mut();
                let rich_text_id = current.rich_text_id;
                let mut terminal = current.terminal.lock();
                terminal.scroll_display(Scroll::Delta(lines));
                drop(terminal);
                self.renderer.scrollbar.notify_scroll(rich_text_id);
            }
        }

        self.mouse.accumulated_scroll.x %= width;
        self.mouse.accumulated_scroll.y %= height;
    }
}
