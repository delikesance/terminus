//! Shared single-line (or paste-friendly) text draft: caret, selection,
//! arrows, word jumps. Used by Settings key import and other chrome fields
//! that need the same editing model as inline rename.

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

/// Paint model for a labeled text field card (Settings, SFTP name, …).
///
/// Carries everything a painter needs to place the caret and draw the
/// selection wash: the text being shown, the display-space prefix that
/// precedes the caret, and the selected display range.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldPaint {
    pub text: String,
    pub placeholder: bool,
    pub show_caret: bool,
    /// Display text from the line start up to the caret. The caret is
    /// painted immediately after it, so it moves with the editing model
    /// instead of being pinned to the end of the value.
    pub caret_prefix: String,
    /// Selected display range while focused — `None` when collapsed or
    /// when the field does not own the caret.
    pub selection: Option<(usize, usize)>,
}

impl FieldPaint {
    /// Build paint state from a [`TextDraft`].
    pub fn from_draft(draft: &TextDraft, placeholder: &str, focused: bool) -> Self {
        if draft.value.is_empty() && !focused {
            Self {
                text: placeholder.to_string(),
                placeholder: true,
                show_caret: false,
                caret_prefix: String::new(),
                selection: None,
            }
        } else if draft.value.is_empty() {
            Self {
                text: String::new(),
                placeholder: false,
                show_caret: focused,
                caret_prefix: String::new(),
                selection: None,
            }
        } else {
            Self {
                text: draft.display_line(),
                placeholder: false,
                show_caret: focused,
                caret_prefix: draft.prefix_display(),
                selection: if focused {
                    draft.selection_range()
                } else {
                    None
                },
            }
        }
    }

    /// [Self::from_draft] with every visible character replaced by a
    /// bullet — used by secret fields. Lengths are preserved so the caret
    /// prefix and the selection range stay aligned with the real draft.
    pub fn from_draft_masked(
        draft: &TextDraft,
        placeholder: &str,
        focused: bool,
        mask: bool,
    ) -> Self {
        let mut paint = Self::from_draft(draft, placeholder, focused);
        if mask && !paint.placeholder && !draft.value.is_empty() {
            let len = draft.value.chars().count();
            paint.text = "•".repeat(len);
            paint.caret_prefix = "•".repeat(draft.caret.min(len));
        }
        paint
    }

    /// Idle / focused paint for a plain `String` field (Settings URI, …).
    pub fn from_value(value: &str, placeholder: &str, focused: bool) -> Self {
        if value.is_empty() && !focused {
            Self {
                text: placeholder.to_string(),
                placeholder: true,
                show_caret: false,
                caret_prefix: String::new(),
                selection: None,
            }
        } else {
            Self {
                text: value.to_string(),
                placeholder: false,
                show_caret: focused,
                caret_prefix: value.to_string(),
                selection: None,
            }
        }
    }
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
    c.is_alphanumeric()
        || c == '_'
        || c == '-'
        || c == '.'
        || c == '+'
        || c == '/'
        || c == '='
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pem_insert_keeps_newlines() {
        let mut d = TextDraft::default();
        assert!(d.insert(
            "-----BEGIN OPENSSH PRIVATE KEY-----\nabc\n-----END OPENSSH PRIVATE KEY-----\n",
            4096,
            true
        ));
        assert!(d.value.contains('\n'));
        assert!(d.value.contains("BEGIN OPENSSH"));
    }

    #[test]
    fn label_rejects_newlines() {
        let mut d = TextDraft::default();
        assert!(!d.insert("a\nb", 64, false));
        assert!(d.insert("ab", 64, false));
    }

    #[test]
    fn masked_paint_keeps_caret_and_selection_aligned() {
        let mut d = TextDraft::new("secret");
        d.move_home(TextMoveKind::Collapse);
        assert!(d.move_right(TextMoveKind::Extend, false));
        assert!(d.move_right(TextMoveKind::Extend, false));

        let masked = FieldPaint::from_draft_masked(&d, "Enter passphrase", true, true);
        assert_eq!(masked.text, "••••••");
        assert_eq!(masked.caret_prefix, "••");
        assert_eq!(masked.selection, Some((0, 2)));
        assert!(masked.show_caret);
        assert!(!masked.placeholder);

        let plain = FieldPaint::from_draft_masked(&d, "Enter passphrase", true, false);
        assert_eq!(plain.text, "secret");
        assert_eq!(plain.caret_prefix, "se");
        assert_eq!(plain.selection, Some((0, 2)));

        let idle = FieldPaint::from_draft_masked(
            &TextDraft::default(),
            "Enter passphrase",
            false,
            true,
        );
        assert!(idle.placeholder);
        assert_eq!(idle.text, "Enter passphrase");
        assert!(!idle.show_caret);
        assert_eq!(idle.selection, None);
    }
}
