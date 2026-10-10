#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TextDraft {
    pub value: String,
    /// Character index of the caret inside [`Self::value`].
    pub caret: usize,
    /// When set, selection spans `[min(anchor, caret), max(anchor, caret))`.
    pub sel_anchor: Option<usize>,
}

/// How a caret move should treat an existing selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextMoveKind {
    Collapse,
    Extend,
}

/// One editing command. Every text field routes its keys through
/// [`TextDraft::apply`] so Backspace, Delete, caret moves and selection
/// behave identically everywhere.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextEdit {
    Backspace { by_word: bool },
    Delete { by_word: bool },
    Left { kind: TextMoveKind, by_word: bool },
    Right { kind: TextMoveKind, by_word: bool },
    Home { kind: TextMoveKind },
    End { kind: TextMoveKind },
    SelectAll,
}
impl TextDraft {
    pub fn new(value: impl Into<String>) -> Self {
        let value = value.into();
        let caret = value.chars().count();
        Self {
            value,
            caret,
            sel_anchor: None,
        }
    }

    /// Run one editing command. Returns whether anything changed.
    pub fn apply(&mut self, edit: TextEdit) -> bool {
        match edit {
            TextEdit::Backspace { by_word } => self.backspace(by_word),
            TextEdit::Delete { by_word } => self.delete_forward(by_word),
            TextEdit::Left { kind, by_word } => self.move_left(kind, by_word),
            TextEdit::Right { kind, by_word } => self.move_right(kind, by_word),
            TextEdit::Home { kind } => self.move_home(kind),
            TextEdit::End { kind } => self.move_end(kind),
            TextEdit::SelectAll => self.select_all(),
        }
    }

    pub fn clear(&mut self) {
        self.value.clear();
        self.caret = 0;
        self.sel_anchor = None;
    }

    pub fn selection_range(&self) -> Option<(usize, usize)> {
        let anchor = self.sel_anchor?;
        let (a, b) = if anchor <= self.caret {
            (anchor, self.caret)
        } else {
            (self.caret, anchor)
        };
        (a < b).then_some((a, b))
    }

    pub fn select_all(&mut self) -> bool {
        let end = self.value.chars().count();
        if end == 0 {
            return false;
        }
        self.sel_anchor = Some(0);
        self.caret = end;
        true
    }

    pub fn delete_selection(&mut self) -> bool {
        let Some((start, end)) = self.selection_range() else {
            return false;
        };
        let from = char_byte(&self.value, start);
        let to = char_byte(&self.value, end);
        self.value.replace_range(from..to, "");
        self.caret = start;
        self.sel_anchor = None;
        true
    }

    /// Insert text at the caret. Control chars rejected unless `allow_newlines`
    /// (then only `\n` / `\r` are kept among controls — needed for OpenSSH PEM).
    pub fn insert(&mut self, text: &str, max_bytes: usize, allow_newlines: bool) -> bool {
        if text.is_empty() {
            return false;
        }
        let filtered: String = if allow_newlines {
            text.chars()
                .filter(|c| *c == '\n' || *c == '\r' || !c.is_control())
                .collect()
        } else if text.chars().any(char::is_control) {
            return false;
        } else {
            text.to_string()
        };
        if filtered.is_empty() {
            return false;
        }
        self.delete_selection();
        if self.value.len() + filtered.len() > max_bytes {
            return false;
        }
        let byte = char_byte(&self.value, self.caret);
        let added = filtered.chars().count();
        self.value.insert_str(byte, &filtered);
        self.caret += added;
        self.sel_anchor = None;
        true
    }

    pub fn backspace(&mut self, by_word: bool) -> bool {
        if self.delete_selection() {
            return true;
        }
        if by_word {
            let start = word_boundary_left(&self.value, self.caret);
            if start == self.caret {
                return false;
            }
            let from = char_byte(&self.value, start);
            let to = char_byte(&self.value, self.caret);
            self.value.replace_range(from..to, "");
            self.caret = start;
            return true;
        }
        if self.caret == 0 {
            return false;
        }
        let start = char_byte(&self.value, self.caret - 1);
        let end = char_byte(&self.value, self.caret);
        self.value.replace_range(start..end, "");
        self.caret -= 1;
        true
    }

    pub fn delete_forward(&mut self, by_word: bool) -> bool {
        if self.delete_selection() {
            return true;
        }
        let len = self.value.chars().count();
        if self.caret >= len {
            return false;
        }
        if by_word {
            let end = word_boundary_right(&self.value, self.caret);
            if end == self.caret {
                return false;
            }
            let from = char_byte(&self.value, self.caret);
            let to = char_byte(&self.value, end);
            self.value.replace_range(from..to, "");
            return true;
        }
        let start = char_byte(&self.value, self.caret);
        let end = char_byte(&self.value, self.caret + 1);
        self.value.replace_range(start..end, "");
        true
    }

    fn prepare_move(&mut self, kind: TextMoveKind) {
        match kind {
            TextMoveKind::Collapse => {
                if let Some((start, end)) = self.selection_range() {
                    // Caller adjusts caret after; clear selection first.
                    let _ = (start, end);
                }
                self.sel_anchor = None;
            }
            TextMoveKind::Extend => {
                if self.sel_anchor.is_none() {
                    self.sel_anchor = Some(self.caret);
                }
            }
        }
    }

    pub fn move_left(&mut self, kind: TextMoveKind, by_word: bool) -> bool {
        if kind == TextMoveKind::Collapse {
            if let Some((start, _)) = self.selection_range() {
                self.caret = start;
                self.sel_anchor = None;
                return true;
            }
        }
        self.prepare_move(kind);
        let next = if by_word {
            word_boundary_left(&self.value, self.caret)
        } else if self.caret == 0 {
            self.caret
        } else {
            self.caret - 1
        };
        if next == self.caret && kind != TextMoveKind::Extend {
            return false;
        }
        let changed = next != self.caret || self.selection_range().is_some();
        self.caret = next;
        if kind == TextMoveKind::Collapse {
            self.sel_anchor = None;
        }
        changed || kind == TextMoveKind::Extend
    }

    pub fn move_right(&mut self, kind: TextMoveKind, by_word: bool) -> bool {
        if kind == TextMoveKind::Collapse {
            if let Some((_, end)) = self.selection_range() {
                self.caret = end;
                self.sel_anchor = None;
                return true;
            }
        }
        self.prepare_move(kind);
        let len = self.value.chars().count();
        let next = if by_word {
            word_boundary_right(&self.value, self.caret)
        } else if self.caret >= len {
            self.caret
        } else {
            self.caret + 1
        };
        let changed = next != self.caret;
        self.caret = next;
        if kind == TextMoveKind::Collapse {
            self.sel_anchor = None;
        }
        changed || kind == TextMoveKind::Extend
    }

    pub fn move_home(&mut self, kind: TextMoveKind) -> bool {
        self.prepare_move(kind);
        if self.caret == 0 && kind == TextMoveKind::Collapse {
            return false;
        }
        self.caret = 0;
        if kind == TextMoveKind::Collapse {
            self.sel_anchor = None;
        }
        true
    }

    pub fn move_end(&mut self, kind: TextMoveKind) -> bool {
        self.prepare_move(kind);
        let end = self.value.chars().count();
        if self.caret == end && kind == TextMoveKind::Collapse {
            return false;
        }
        self.caret = end;
        if kind == TextMoveKind::Collapse {
            self.sel_anchor = None;
        }
        true
    }

    pub fn prefix(&self) -> String {
        self.value.chars().take(self.caret).collect()
    }

    /// Single-line preview (newlines → spaces) for painting.
    pub fn display_line(&self) -> String {
        self.value
            .chars()
            .map(|c| if c == '\n' || c == '\r' { ' ' } else { c })
            .collect()
    }

    pub fn prefix_display(&self) -> String {
        self.display_line().chars().take(self.caret).collect()
    }
}

fn char_byte(value: &str, chars: usize) -> usize {
    value
        .char_indices()
        .nth(chars)
        .map(|(byte, _)| byte)
        .unwrap_or(value.len())
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

fn word_boundary_left(value: &str, caret: usize) -> usize {
    let chars: Vec<char> = value.chars().collect();
    if caret == 0 || chars.is_empty() {
        return 0;
    }
    let mut i = caret.min(chars.len());
    while i > 0 && chars[i - 1].is_whitespace() {
        i -= 1;
    }
    if i == 0 {
        return 0;
    }
    let word = is_word_char(chars[i - 1]);
    while i > 0 && is_word_char(chars[i - 1]) == word && !chars[i - 1].is_whitespace() {
        i -= 1;
    }
    i
}

fn word_boundary_right(value: &str, caret: usize) -> usize {
    let chars: Vec<char> = value.chars().collect();
    let len = chars.len();
    if caret >= len {
        return len;
    }
    let mut i = caret;
    while i < len && chars[i].is_whitespace() {
        i += 1;
    }
    if i >= len {
        return len;
    }
    let word = is_word_char(chars[i]);
    while i < len && is_word_char(chars[i]) == word && !chars[i].is_whitespace() {
        i += 1;
    }
    i
}
