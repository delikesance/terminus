use super::*;
use crate::components::input::{TextEdit, TextMoveKind};

impl AddHostForm {
    /// Insert text at the caret. Empty or control-bearing text is
    /// rejected, the same policy every other text sink in rio applies.
    pub fn insert(&mut self, text: &str) -> bool {
        let field = self.focused_field();
        if !field.is_text() {
            return false;
        }
        if field == Field::Port && !self.port_dirty {
            // Typing over the untouched default replaces it, unless the
            // text would be refused anyway.
            if text.is_empty() || text.chars().any(char::is_control) {
                return false;
            }
            self.values[3].clear();
        }
        let Some(draft) = self.draft_mut(field) else {
            return false;
        };
        if !draft.insert(text, usize::MAX, false) {
            return false;
        }
        if field == Field::Port {
            self.port_dirty = true;
        }
        self.clear_error();
        true
    }

    /// Shared text editing (Backspace, Delete, caret, selection, by word).
    /// Returns whether anything changed.
    pub fn edit(&mut self, edit: TextEdit) -> bool {
        let field = self.focused_field();
        let Some(draft) = self.draft_mut(field) else {
            return false;
        };
        let before = draft.value.len();
        if !draft.apply(edit) {
            return false;
        }
        let edited = draft.value.len() != before;
        if edited {
            if field == Field::Port {
                self.port_dirty = true;
            }
            self.clear_error();
        }
        true
    }

    /// Delete the character before the caret. Returns whether anything
    /// changed (a backspace at offset 0 must not be reported as an edit,
    /// or the caller repaints on every stray keypress).
    pub fn backspace(&mut self) -> bool {
        self.edit(TextEdit::Backspace { by_word: false })
    }

    /// Delete the character after the caret.
    pub fn delete(&mut self) -> bool {
        self.edit(TextEdit::Delete { by_word: false })
    }

    pub fn move_cursor(&mut self, delta: isize) {
        let edit = if delta < 0 {
            TextEdit::Left {
                kind: TextMoveKind::Collapse,
                by_word: false,
            }
        } else {
            TextEdit::Right {
                kind: TextMoveKind::Collapse,
                by_word: false,
            }
        };
        for _ in 0..delta.unsigned_abs() {
            self.edit(edit);
        }
    }

    pub fn cursor_home(&mut self) {
        self.edit(TextEdit::Home {
            kind: TextMoveKind::Collapse,
        });
    }

    pub fn cursor_end(&mut self) {
        self.edit(TextEdit::End {
            kind: TextMoveKind::Collapse,
        });
    }

    /// Leaving the address: show the user and port it carried in their
    /// own (empty) fields, so what gets saved is what the form shows.
    pub fn settle_address(&mut self) {
        if self.focus != Field::Hostname {
            return;
        }
        let parsed = self.values();
        for (i, value) in [(1, parsed.hostname), (2, parsed.username), (3, parsed.port)] {
            if self.values[i].value != value {
                if i == 3 {
                    self.port_dirty = true;
                }
                self.values[i] = TextDraft::new(value);
            }
        }
    }

    /// Move focus `delta` fields forward, wrapping across visible rows.
    pub(super) fn focus_by(&mut self, delta: isize) {
        self.settle_address();
        let fields = self.visible_fields();
        let len = fields.len() as isize;
        let i = fields.iter().position(|&f| f == self.focus).unwrap_or(0) as isize;
        let next = (i + delta).rem_euclid(len) as usize;
        self.focus_input(fields[next]);
        self.clear_error();
    }

    /// Focus `field`, closing a list that belongs to another one.
    pub(super) fn focus_input(&mut self, field: Field) {
        self.focus = field;
        let keep = match self.menu {
            Some(SelectMenu::Identity) => field == Field::Identity,
            Some(SelectMenu::Group) => field == Field::Group,
            None => true,
        };
        if !keep {
            self.close_menu();
        }
    }

    /// Focus a specific field (mouse click into an input).
    pub fn focus_field(&mut self, field: Field) {
        if field != Field::Hostname {
            self.settle_address();
        }
        if self.visible_fields().contains(&field) {
            self.focus_input(field);
            self.clear_error();
        }
    }
}
