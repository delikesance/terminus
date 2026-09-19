//! `Screen` mouse surface, split out of `screen/mod.rs`.

use super::Screen;
use crate::crosswords::pos::{Column, Pos};
use crate::crosswords::Mode;
use crate::renderer::island;
use crate::selection::SelectionType;
use rio_backend::clipboard::Clipboard;
use rio_backend::event::ClickState;
use rio_window::event::ElementState;

impl Screen<'_> {
    #[inline]
    pub fn on_left_click(&mut self, point: Pos, clipboard: &mut Clipboard) {
        let side = self.mouse.square_side;

        match self.mouse.click_state {
            ClickState::Click => {
                // If Shift is pressed and there's an existing selection, expand it
                if self.modifiers.state().shift_key() && !self.selection_is_empty() {
                    self.update_selection(point, side);
                } else {
                    self.clear_selection();

                    // Start new empty selection.
                    if self.modifiers.state().control_key() {
                        self.start_selection(
                            SelectionType::Block,
                            point,
                            side,
                            clipboard,
                        );
                    } else {
                        self.start_selection(
                            SelectionType::Simple,
                            point,
                            side,
                            clipboard,
                        );
                    }
                }
            }
            ClickState::DoubleClick => {
                self.start_selection(SelectionType::Semantic, point, side, clipboard);
            }
            ClickState::TripleClick => {
                self.start_selection(SelectionType::Lines, point, side, clipboard);
            }
            ClickState::None => (),
        };

        // Move vi mode cursor to mouse click position.
        let mut terminal = self.context_manager.current_mut().terminal.lock();
        if terminal.mode().contains(Mode::VI) {
            terminal.vi_mode_cursor.pos = point;
        }
        drop(terminal);
    }

    pub(super) fn sgr_mouse_report(&mut self, pos: Pos, button: u8, state: ElementState) {
        let c = match state {
            ElementState::Pressed => 'M',
            ElementState::Released => 'm',
        };

        let msg = format!("\x1b[<{};{};{}{}", button, pos.col + 1, pos.row + 1, c);
        self.ctx_mut()
            .current_mut()
            .messenger
            .send_write(msg.into_bytes());
    }

    #[inline]
    pub fn has_mouse_motion_and_drag(&mut self) -> bool {
        self.get_mode()
            .intersects(Mode::MOUSE_MOTION | Mode::MOUSE_DRAG)
    }

    #[inline]
    pub fn has_mouse_motion(&mut self) -> bool {
        self.get_mode().intersects(Mode::MOUSE_MOTION)
    }

    #[inline]
    pub fn mouse_report(&mut self, button: u8, state: ElementState) {
        let terminal = self.ctx().current().terminal.lock();
        let display_offset = terminal.display_offset();
        let mode = terminal.mode();
        drop(terminal);

        let pos = self.mouse_position(display_offset);

        // Assure the mouse pos is not in the scrollback.
        if pos.row < 0 {
            return;
        }

        // X10 reports presses of the left, middle and right buttons only, and
        // never carries modifiers. Motion never reaches here under X10 because
        // it is gated on MOUSE_MOTION and MOUSE_DRAG, but releases and the
        // wheel codes (64 and up) do, and neither is reportable. Both are
        // dropped rather than falling back to local scrolling, since the
        // protocol still owns the wheel while it is active.
        if mode.contains(Mode::MOUSE_REPORT_X10) {
            if state == ElementState::Pressed && button <= 2 {
                if mode.contains(Mode::SGR_MOUSE) {
                    self.sgr_mouse_report(pos, button, state);
                } else {
                    self.normal_mouse_report(pos, button);
                }
            }

            return;
        }

        // Calculate modifiers value.
        let mut mods = 0;
        let mod_state = self.modifiers.state();
        if mod_state.shift_key() {
            mods += 4;
        }
        if mod_state.alt_key() {
            mods += 8;
        }
        if mod_state.control_key() {
            mods += 16;
        }

        // Report mouse events.
        if mode.contains(Mode::SGR_MOUSE) {
            self.sgr_mouse_report(pos, button + mods, state);
        } else if let ElementState::Released = state {
            self.normal_mouse_report(pos, 3 + mods);
        } else {
            self.normal_mouse_report(pos, button + mods);
        }
    }

    #[inline]
    pub(super) fn normal_mouse_report(&mut self, position: Pos, button: u8) {
        let Pos { row, col } = position;
        let utf8 = self.get_mode().contains(Mode::UTF8_MOUSE);

        let max_point = if utf8 { 2015 } else { 223 };

        if row >= max_point || col >= max_point {
            return;
        }

        let mut msg = vec![b'\x1b', b'[', b'M', 32 + button];

        let mouse_pos_encode = |pos: usize| -> Vec<u8> {
            let pos = 32 + 1 + pos;
            let first = 0xC0 + pos / 64;
            let second = 0x80 + (pos & 63);
            vec![first as u8, second as u8]
        };

        if utf8 && col >= Column(95) {
            msg.append(&mut mouse_pos_encode(col.0));
        } else {
            msg.push(32 + 1 + col.0 as u8);
        }

        if utf8 && row >= 95 {
            msg.append(&mut mouse_pos_encode(row.0 as usize));
        } else {
            msg.push(32 + 1 + row.0 as u8);
        }

        self.ctx_mut().current_mut().messenger.send_write(msg);
    }

    #[inline]
    pub fn on_focus_change(&mut self, is_focused: bool) {
        self.renderer.is_window_focused = is_focused;
        if is_focused {
            self.mark_dirty();
        }
        if !is_focused {
            let rc = &mut self.context_manager.current_mut().renderable_content;
            if !rc.is_blinking_cursor_visible {
                rc.is_blinking_cursor_visible = true;
            }
            rc.last_blink_toggle = None;
            rc.pending_update
                .set_terminal_damage(rio_backend::event::TerminalDamage::CursorOnly);

            if let Some(ref mut island) = self.renderer.island {
                if island.is_dragging() {
                    island.cancel_drag();
                    self.mark_dirty();
                }
            }
            self.mouse.left_button_state = ElementState::Released;
        }

        if self.get_mode().contains(Mode::FOCUS_IN_OUT) {
            let chr = if is_focused { "I" } else { "O" };

            let msg = format!("\x1b[{chr}");
            self.ctx_mut()
                .current_mut()
                .messenger
                .send_write(msg.into_bytes());
        }
    }
}
