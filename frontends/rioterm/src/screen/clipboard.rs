//! `Screen` clipboard surface, split out of `screen/mod.rs`.

use super::Screen;
use crate::crosswords::grid::Dimensions;
use crate::crosswords::pos::{Column, Pos, Side};
use crate::crosswords::Mode;
use crate::selection::{Selection, SelectionType};
use rio_backend::clipboard::{Clipboard, ClipboardType};

impl Screen<'_> {
    pub fn copy_selection(&mut self, ty: ClipboardType, clipboard: &mut Clipboard) {
        let terminal = self.context_manager.current_mut().terminal.lock();
        let text = match terminal.selection_to_string().filter(|s| !s.is_empty()) {
            Some(text) => text,
            None => return,
        };
        drop(terminal);

        clipboard.set(ty, text);
    }

    #[inline]
    pub fn select_all(&mut self) {
        let current = self.context_manager.current_mut();
        let mut terminal = current.terminal.lock();
        let start = Pos::new(terminal.grid.topmost_line(), Column(0));
        let end = Pos::new(terminal.grid.bottommost_line(), terminal.grid.last_column());
        let mut selection = Selection::new(SelectionType::Simple, start, Side::Left);
        selection.update(end, Side::Right);
        let selection_range = selection.to_range(&terminal);
        terminal.selection = Some(selection);
        drop(terminal);

        current.set_selection(selection_range);
        self.context_manager.request_render();
    }

    #[inline]
    pub fn clear_selection(&mut self) {
        // Clear the selection on the terminal.
        let mut terminal = self.context_manager.current_mut().terminal.lock();
        terminal.selection.take();
        drop(terminal);
        self.context_manager.current_mut().set_selection(None);
    }

    #[inline]
    pub fn paste(&mut self, text: &str, bracketed: bool) {
        if self.search_active() {
            for c in text.chars() {
                self.search_input(c);
            }
        } else if bracketed && self.get_mode().contains(Mode::BRACKETED_PASTE) {
            self.scroll_bottom_when_cursor_not_visible();
            self.clear_selection();

            self.ctx_mut()
                .current_mut()
                .messenger
                .send_write(&b"\x1b[200~"[..]);

            // Write filtered escape sequences.
            //
            // We remove `\x1b` to ensure it's impossible for the pasted text to write the bracketed
            // paste end escape `\x1b[201~` and `\x03` since some shells incorrectly terminate
            // bracketed paste on its receival.
            let filtered = text.replace(['\x1b', '\x03'], "");
            self.ctx_mut()
                .current_mut()
                .messenger
                .send_write(filtered.into_bytes());

            self.ctx_mut()
                .current_mut()
                .messenger
                .send_write(&b"\x1b[201~"[..]);
        } else {
            let payload = if bracketed {
                // In non-bracketed (ie: normal) mode, terminal applications cannot distinguish
                // pasted data from keystrokes.
                //
                // In theory, we should construct the keystrokes needed to produce the data we are
                // pasting... since that's neither practical nor sensible (and probably an
                // impossible task to solve in a general way), we'll just replace line breaks
                // (windows and unix style) with a single carriage return (\r, which is what the
                // Enter key produces).
                text.replace("\r\n", "\r").replace('\n', "\r").into_bytes()
            } else {
                // When we explicitly disable bracketed paste don't manipulate with the input,
                // so we pass user input as is.
                text.to_owned().into_bytes()
            };

            self.ctx_mut().current_mut().messenger.send_write(payload);
        }
    }
}
