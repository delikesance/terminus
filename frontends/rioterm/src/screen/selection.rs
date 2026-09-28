//! `Screen` selection surface, split out of `screen/mod.rs`.

use super::Screen;
use crate::context;
use crate::crosswords::grid::Scroll;
use crate::crosswords::pos::{Pos, Side};
use crate::crosswords::Mode;
use crate::selection::{Selection, SelectionType};
use rio_backend::clipboard::{Clipboard, ClipboardType};
use rio_window::event::ElementState;

impl Screen<'_> {
    #[inline]
    pub fn select_current_based_on_mouse(&mut self) -> bool {
        if self
            .context_manager
            .current_grid_mut()
            .select_current_based_on_mouse(&self.mouse)
        {
            self.context_manager.select_route_from_current_grid();
            // The focusing click never reaches on_left_click, so a
            // selection left behind in the target panel would
            // drag-extend from its stale anchor; drop it on switch.
            self.clear_selection();
            return true;
        }
        false
    }

    #[inline]
    pub(super) fn start_selection(
        &mut self,
        ty: SelectionType,
        point: Pos,
        side: Side,
        clipboard: &mut Clipboard,
    ) {
        self.copy_selection(ClipboardType::Selection, clipboard);
        let current = self.context_manager.current_mut();
        let mut terminal = current.terminal.lock();
        let selection = Selection::new(ty, point, side);
        let selection_range = selection.to_range(&terminal);
        terminal.selection = Some(selection);
        drop(terminal);

        // Use set_selection to trigger render
        current.set_selection(selection_range);

        // Request render to ensure it shows immediately
        self.context_manager.request_render();
    }

    #[inline]
    pub(super) fn toggle_selection(
        &mut self,
        ty: SelectionType,
        side: Side,
        clipboard: &mut Clipboard,
    ) {
        let mut terminal = self.context_manager.current().terminal.lock();
        match &mut terminal.selection {
            Some(selection) if selection.ty == ty && !selection.is_empty() => {
                drop(terminal);
                self.clear_selection();
            }
            Some(selection) if !selection.is_empty() => {
                selection.ty = ty;
                drop(terminal);
                self.copy_selection(ClipboardType::Selection, clipboard);
            }
            _ => {
                let pos = terminal.vi_mode_cursor.pos;
                drop(terminal);
                self.start_selection(ty, pos, side, clipboard)
            }
        }

        let current = self.context_manager.current_mut();
        let mut terminal = current.terminal.lock();
        let mut selection = match terminal.selection.take() {
            Some(selection) => {
                // Make sure initial selection is not empty.
                selection
            }
            None => return,
        };

        selection.include_all();
        current.renderable_content.selection_range = selection.to_range(&terminal);
        terminal.selection = Some(selection);
        drop(terminal);
    }

    #[inline]
    pub fn update_selection(&mut self, mut pos: Pos, side: Side) {
        let is_search_active = self.search_active();
        let current = self.context_manager.current_mut();
        let mut terminal = current.terminal.lock();
        let mut selection = match terminal.selection.take() {
            Some(selection) => selection,
            None => return,
        };

        // Treat motion over message bar like motion over the last line.
        pos.row = std::cmp::min(pos.row, terminal.bottommost_line());

        // Update selection.
        selection.update(pos, side);

        // Move vi cursor and expand selection.
        if terminal.mode().contains(Mode::VI) && !is_search_active {
            terminal.vi_mode_cursor.pos = pos;
            selection.include_all();
        }

        let selection_range = selection.to_range(&terminal);
        terminal.selection = Some(selection);
        drop(terminal);

        // Use set_selection to trigger render
        current.set_selection(selection_range);

        // Request render to ensure it shows immediately
        self.context_manager.request_render();
    }

    #[inline]
    /// Compute the selection scroll delta for the given mouse Y position.
    /// Returns 0 if the mouse is within the viewport, ±1 at the edges.
    /// `mouse_y` is in physical pixels (from CursorMoved position.y).
    pub fn selection_scroll_delta(&self, mouse_y: f64) -> i32 {
        let current_grid = self.context_manager.current_grid();
        let (context, margin) = current_grid.current_context_with_computed_dimension();
        let layout = context.dimension;
        // Canonical integer cell stride. line_height is already
        // baked into `cell.cell_height`; the previous code
        // multiplied by line_height again, breaking the
        // edge-of-viewport detection at line_height ≠ 1.0.
        let cell_height = layout.cell.cell_height as f64;
        let text_area_top = margin.top as f64;
        let text_area_bottom = text_area_top + layout.lines as f64 * cell_height;
        let window_height = self.sugarloaf.window_size().height as f64;

        if mouse_y < text_area_top {
            1 // scroll up (into history)
        } else if mouse_y >= window_height - cell_height && mouse_y >= text_area_bottom {
            -1 // scroll down (toward present)
        } else {
            0
        }
    }

    /// Perform one tick of selection auto-scroll.
    /// Reads mouse.raw_y to compute scroll direction.
    /// Scrolls 1 line per tick.
    pub fn selection_scroll_tick(&mut self) {
        if self.mouse.left_button_state != rio_window::event::ElementState::Pressed {
            return;
        }

        let delta = self.selection_scroll_delta(self.mouse.raw_y);
        if delta == 0 {
            return;
        }

        let mut terminal = self.context_manager.current_mut().terminal.lock();
        terminal.scroll_display(Scroll::Delta(delta));
        drop(terminal);

        // Update selection to match the new scroll position.
        let display_offset = self.display_offset();
        let point = self.mouse_position(display_offset);
        let side = self.mouse.square_side;
        self.update_selection(point, side);
    }

    #[inline]
    pub fn contains_point(&self, x: f64, y: f64) -> bool {
        let current_grid = self.context_manager.current_grid();
        let (context, margin) = current_grid.current_context_with_computed_dimension();
        let layout = context.dimension;
        // Canonical integer stride — same as the GPU paints with.
        // line_height is already baked into `cell.cell_height`; do
        // NOT multiply again here.
        let cell_w = layout.cell.cell_width as f64;
        let cell_h = layout.cell.cell_height as f64;
        let left = margin.left as f64;
        let top = margin.top as f64;
        x > left
            && x <= left + layout.columns as f64 * cell_w
            && y > top
            && y <= top + layout.lines as f64 * cell_h
    }

    #[inline]
    pub fn side_by_pos(&self, x: f64) -> Side {
        let current_grid = self.context_manager.current_grid();
        let (_, margin) = current_grid.current_context_with_computed_dimension();
        let current_context = self.context_manager.current();
        let layout = current_context.dimension;

        crate::mouse::calculate_side_by_pos(
            x,
            margin.left,
            layout.cell.cell_width,
            layout.width,
        )
    }

    #[inline]
    pub fn selection_is_empty(&self) -> bool {
        self.context_manager
            .current()
            .renderable_content
            .selection_range
            .is_none()
    }
}
