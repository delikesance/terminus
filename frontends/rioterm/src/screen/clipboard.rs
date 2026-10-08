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

    /// A clipboard / selection paste from a key or mouse binding. While a
    /// view covers the terminal it goes to the view's focused field (a
    /// pasted private key must land in Settings, not in the hidden shell).
    pub fn paste_from_clipboard(&mut self, text: &str) {
        if self.view_takes_keys() {
            if !self.sftp_bridged() {
                self.view_text(text);
            }
            return;
        }
        self.paste(text, true);
    }

    #[inline]
    pub fn paste(&mut self, text: &str, bracketed: bool) {
        if self.search_active() {
            for c in text.chars() {
                self.search_input(c);
            }
        } else {
            let mode = self.get_mode().contains(Mode::BRACKETED_PASTE);
            if bracketed && mode {
                self.scroll_bottom_when_cursor_not_visible();
                self.clear_selection();
            }
            let payload = paste_bytes(text, bracketed, mode);
            self.ctx_mut().current_mut().messenger.send_write(payload);
        }
    }
}

/// The bytes a paste of `text` writes to the pty. `bracketed` is false for
/// raw key sequences (`Esc` bindings); `bracketed_mode` is whether the
/// program asked for bracketed paste.
pub(super) fn paste_bytes(text: &str, bracketed: bool, bracketed_mode: bool) -> Vec<u8> {
    if bracketed && bracketed_mode {
        // Write filtered escape sequences.
        //
        // We remove `\x1b` to ensure it's impossible for the pasted text to write the bracketed
        // paste end escape `\x1b[201~` and `\x03` since some shells incorrectly terminate
        // bracketed paste on its receival.
        let filtered = text.replace(['\x1b', '\x03'], "");
        let mut bytes = Vec::with_capacity(filtered.len() + 12);
        bytes.extend_from_slice(b"\x1b[200~");
        bytes.extend_from_slice(filtered.as_bytes());
        bytes.extend_from_slice(b"\x1b[201~");
        bytes
    } else if bracketed {
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
        text.as_bytes().to_vec()
    }
}

#[cfg(test)]
mod tests {
    use super::paste_bytes;

    #[test]
    fn bracketed_mode_wraps_and_strips_escapes() {
        assert_eq!(
            paste_bytes("a\x1b[201~b\x03", true, true),
            b"\x1b[200~a[201~b\x1b[201~".to_vec()
        );
    }

    #[test]
    fn plain_mode_turns_newlines_into_enter() {
        assert_eq!(paste_bytes("a\r\nb\nc", true, false), b"a\rb\rc".to_vec());
    }

    #[test]
    fn raw_sequences_pass_through() {
        assert_eq!(paste_bytes("\x1b[A\n", false, true), b"\x1b[A\n".to_vec());
    }
}
