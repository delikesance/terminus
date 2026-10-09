//! Inputs: text field, search, select and textarea geometry.

use crate::geom::Rect;
use crate::icons::Icon;

pub const FIELD_HEIGHT: f32 = 46.0;
pub const TEXTAREA_HEIGHT: f32 = 92.0;
pub const SEARCH_HEIGHT: f32 = 40.0;
pub const COMMAND_HEIGHT: f32 = 38.0;
pub const LABEL_FONT: f32 = 13.0;
pub const LABEL_HEIGHT: f32 = 16.0;
pub const HELPER_FONT: f32 = 12.0;
pub const HELPER_HEIGHT: f32 = 16.0;
pub const LABEL_GAP: f32 = 8.0;
pub const PAD_LEFT: f32 = 14.0;
pub const PAD_RIGHT: f32 = 12.0;
pub const INNER_GAP: f32 = 10.0;
pub const TEXTAREA_PAD_TOP: f32 = 12.0;
pub const TRAILING_ICON: f32 = 16.0;
pub const FOCUS_RING: f32 = 3.0;
pub const FOCUS_RING_ALPHA: f32 = 0.18;
pub const DISABLED_OPACITY: f32 = 0.5;
pub const CARET_WIDTH: f32 = 1.5;

pub const SANS_VALUE_FONT: f32 = 15.0;
pub const MONO_VALUE_FONT: f32 = 14.0;

pub const SEARCH_PAD: f32 = 12.0;
pub const SEARCH_ICON: f32 = 15.0;
pub const SEARCH_FONT: f32 = 14.0;
pub const COMMAND_PAD: f32 = 10.0;
pub const COMMAND_FONT: f32 = 13.0;
pub const COMMAND_HINT_FONT: f32 = 10.0;
pub const COMMAND_HINT: &str = "Ctrl K";
pub const COMMAND_PLACEHOLDER: &str = "Search or run\u{2026}";

pub const PORT_LOCAL_WIDTH: f32 = 140.0;
pub const PORT_ARROW_WIDTH: f32 = 16.0;
pub const PORT_DEST_WIDTH: f32 = 320.0;
pub const PORT_PORT_WIDTH: f32 = 110.0;
pub const PORT_GAP: f32 = 12.0;

pub const GRID_COLUMNS: usize = 3;
pub const GRID_COL_GAP: f32 = 24.0;
pub const GRID_ROW_GAP: f32 = 28.0;
pub const CAPTION_HEIGHT: f32 = 16.0;
pub const CAPTION_GAP: f32 = 10.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldState {
    Default,
    Hover,
    Focus,
    Filled,
    Error,
    Disabled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldKind {
    Text,
    Password,
    Mono,
    Select,
    Textarea,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchKind {
    Search,
    CommandBar,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldHit {
    Box,
    Trailing,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FieldLayout {
    pub total: Rect,
    pub label: Option<Rect>,
    pub box_rect: Rect,
    pub ring: Rect,
    pub text: Rect,
    pub trailing: Option<Rect>,
    pub helper: Option<Rect>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SearchLayout {
    pub box_rect: Rect,
    pub icon: Rect,
    pub text: Rect,
    pub hint: Option<Rect>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PortPairLayout {
    pub local: FieldLayout,
    pub arrow: Rect,
    pub dest: FieldLayout,
    pub port: FieldLayout,
    pub total: Rect,
}

impl FieldKind {
    pub fn is_mono(self) -> bool {
        matches!(self, FieldKind::Mono)
    }

    pub fn value_font(self) -> f32 {
        if self.is_mono() || self == FieldKind::Textarea {
            MONO_VALUE_FONT
        } else {
            SANS_VALUE_FONT
        }
    }

    /// Icon in the trailing slot, if the kind has one.
    pub fn trailing_icon(self, revealed: bool) -> Option<Icon> {
        match self {
            FieldKind::Password if revealed => Some(Icon::EyeOff),
            FieldKind::Password => Some(Icon::Eye),
            FieldKind::Select => Some(Icon::ChevronDown),
            _ => None,
        }
    }

    fn has_trailing(self) -> bool {
        matches!(self, FieldKind::Password | FieldKind::Select)
    }
}

/// Height of the input box itself.
pub fn field_height(kind: FieldKind) -> f32 {
    if kind == FieldKind::Textarea {
        TEXTAREA_HEIGHT
    } else {
        FIELD_HEIGHT
    }
}

/// Line box height used for value text of `font` size.
fn line_height(font: f32) -> f32 {
    (font * 1.25).round()
}

/// Label above, box, optional helper below, all stacked in `width`.
pub fn field_layout(
    origin: (f32, f32),
    width: f32,
    kind: FieldKind,
    has_label: bool,
    has_helper: bool,
) -> FieldLayout {
    let (x, mut y) = origin;
    let label = has_label.then(|| {
        let r = Rect::new(x, y, width, LABEL_HEIGHT);
        y += LABEL_HEIGHT + LABEL_GAP;
        r
    });
    let h = field_height(kind);
    let box_rect = Rect::new(x, y, width, h);
    y += h;
    let helper = has_helper.then(|| {
        y += LABEL_GAP;
        let r = Rect::new(x, y, width, HELPER_HEIGHT);
        y += HELPER_HEIGHT;
        r
    });
    let ring = Rect::new(
        box_rect.x - FOCUS_RING,
        box_rect.y - FOCUS_RING,
        box_rect.width + 2.0 * FOCUS_RING,
        box_rect.height + 2.0 * FOCUS_RING,
    );
    let inner_right = box_rect.right() - PAD_RIGHT;
    let trailing = kind.has_trailing().then(|| {
        Rect::new(
            inner_right - TRAILING_ICON,
            box_rect.y + (FIELD_HEIGHT - TRAILING_ICON) / 2.0,
            TRAILING_ICON,
            TRAILING_ICON,
        )
    });
    let text_right = trailing.map_or(inner_right, |t| t.x - INNER_GAP);
    let lh = line_height(kind.value_font());
    let text_y = if kind == FieldKind::Textarea {
        box_rect.y + TEXTAREA_PAD_TOP
    } else {
        box_rect.y + (h - lh) / 2.0
    };
    let text_x = box_rect.x + PAD_LEFT;
    let text = Rect::new(text_x, text_y, (text_right - text_x).max(0.0), lh);
    let total = Rect::new(x, origin.1, width, y - origin.1);
    FieldLayout {
        total,
        label,
        box_rect,
        ring,
        text,
        trailing,
        helper,
    }
}

/// Layout of a bare text field already placed at `rect`; `trailing_slot`
/// keeps the value clear of an adornment the caller draws itself.
pub fn bare_field_layout(rect: &Rect, trailing_slot: f32) -> FieldLayout {
    let mut layout =
        field_layout((rect.x, rect.y), rect.width, FieldKind::Text, false, false);
    layout.text.width = (layout.text.width - trailing_slot).max(0.0);
    layout
}

/// Caret for a measured `prefix_width` (text from line start to the caret).
pub fn caret_rect(layout: &FieldLayout, _kind: FieldKind, prefix_width: f32) -> Rect {
    let t = layout.text;
    let x = (t.x + prefix_width.max(0.0)).min((t.right() - CARET_WIDTH).max(t.x));
    Rect::new(x, t.y, CARET_WIDTH, t.height)
}

/// What a click at `(x, y)` lands on. Disabled fields are inert.
pub fn hit_test(
    layout: &FieldLayout,
    state: FieldState,
    x: f32,
    y: f32,
) -> Option<FieldHit> {
    if state == FieldState::Disabled {
        return None;
    }
    if layout.trailing.is_some_and(|t| t.contains(x, y)) {
        return Some(FieldHit::Trailing);
    }
    layout.box_rect.contains(x, y).then_some(FieldHit::Box)
}

/// Bullets shown for a password of `chars` characters.
pub fn mask(chars: usize) -> String {
    "\u{2022}".repeat(chars)
}

/// Box border colour for a state (the focus ring is separate).
pub fn border_color(theme: &crate::theme::ChromeTheme, state: FieldState) -> [f32; 4] {
    match state {
        FieldState::Default | FieldState::Filled => theme.line,
        FieldState::Hover => theme.hover_border,
        FieldState::Focus => theme.accent,
        FieldState::Error => theme.danger_fill,
        FieldState::Disabled => theme.divider,
    }
}

/// Search box (40px) or command bar (38px). `hint_width` is the measured
/// width of the command-bar hint; ignored for plain search.
pub fn search_layout(
    origin: (f32, f32),
    width: f32,
    kind: SearchKind,
    hint_width: f32,
) -> SearchLayout {
    let (h, pad) = match kind {
        SearchKind::Search => (SEARCH_HEIGHT, SEARCH_PAD),
        SearchKind::CommandBar => (COMMAND_HEIGHT, COMMAND_PAD),
    };
    let box_rect = Rect::new(origin.0, origin.1, width, h);
    let icon = Rect::new(
        box_rect.x + pad,
        box_rect.y + (h - SEARCH_ICON) / 2.0,
        SEARCH_ICON,
        SEARCH_ICON,
    );
    let right = box_rect.right() - pad;
    let hint = (kind == SearchKind::CommandBar).then(|| {
        let hh = line_height(COMMAND_HINT_FONT);
        Rect::new(
            right - hint_width,
            box_rect.y + (h - hh) / 2.0,
            hint_width,
            hh,
        )
    });
    let text_right = hint.map_or(right, |r| r.x - INNER_GAP);
    let font = if kind == SearchKind::Search {
        SEARCH_FONT
    } else {
        COMMAND_FONT
    };
    let lh = line_height(font);
    let tx = icon.right() + INNER_GAP;
    let text = Rect::new(
        tx,
        box_rect.y + (h - lh) / 2.0,
        (text_right - tx).max(0.0),
        lh,
    );
    SearchLayout {
        box_rect,
        icon,
        text,
        hint,
    }
}

/// Local port -> destination -> port, bottom-aligned like the design.
pub fn port_pair_layout(origin: (f32, f32)) -> PortPairLayout {
    let (x, y) = origin;
    let local = field_layout((x, y), PORT_LOCAL_WIDTH, FieldKind::Mono, true, false);
    let ax = local.box_rect.right() + PORT_GAP;
    let arrow = Rect::new(ax, local.box_rect.y, PORT_ARROW_WIDTH, FIELD_HEIGHT);
    let dest = field_layout(
        (arrow.right() + PORT_GAP, y),
        PORT_DEST_WIDTH,
        FieldKind::Mono,
        true,
        false,
    );
    let port = field_layout(
        (dest.box_rect.right() + PORT_GAP, y),
        PORT_PORT_WIDTH,
        FieldKind::Mono,
        true,
        false,
    );
    let total = Rect::new(x, y, port.box_rect.right() - x, local.total.height);
    PortPairLayout {
        local,
        arrow,
        dest,
        port,
        total,
    }
}

/// Cell `index` of the 3-column gallery grid.
pub fn grid_cell(origin: (f32, f32), width: f32, index: usize, row_height: f32) -> Rect {
    let w = (width - GRID_COL_GAP * (GRID_COLUMNS as f32 - 1.0)) / GRID_COLUMNS as f32;
    let (col, row) = (index % GRID_COLUMNS, index / GRID_COLUMNS);
    Rect::new(
        origin.0 + col as f32 * (w + GRID_COL_GAP),
        origin.1 + row as f32 * (row_height + GRID_ROW_GAP),
        w,
        row_height,
    )
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::ChromeTheme;

    #[test]
    fn bare_layout_reserves_trailing_slot_in_text_width() {
        let rect = Rect::new(10.0, 20.0, 300.0, FIELD_HEIGHT);
        let plain = bare_field_layout(&rect, 0.0);
        let reserved = bare_field_layout(&rect, 40.0);
        assert_eq!(plain.box_rect, rect);
        assert_eq!(plain.label, None);
        assert_eq!(reserved.text.width, plain.text.width - 40.0);
    }

    fn lay(kind: FieldKind, label: bool, helper: bool) -> FieldLayout {
        field_layout((10.0, 20.0), 300.0, kind, label, helper)
    }

    #[test]
    fn box_is_46_tall_below_label() {
        let l = lay(FieldKind::Text, true, false);
        let label = l.label.unwrap();
        assert_eq!(label.height, LABEL_HEIGHT);
        assert_eq!(l.box_rect.y, 20.0 + LABEL_HEIGHT + LABEL_GAP);
        assert_eq!(l.box_rect.height, 46.0);
        assert_eq!(l.box_rect.width, 300.0);
        assert!(l.helper.is_none());
        assert_eq!(l.total.height, LABEL_HEIGHT + LABEL_GAP + 46.0);
    }

    #[test]
    fn without_label_box_starts_at_origin() {
        let l = lay(FieldKind::Text, false, false);
        assert!(l.label.is_none());
        assert_eq!(l.box_rect.y, 20.0);
    }

    #[test]
    fn helper_sits_below_box_with_gap() {
        let l = lay(FieldKind::Text, true, true);
        let h = l.helper.unwrap();
        assert_eq!(h.y, l.box_rect.bottom() + LABEL_GAP);
        assert_eq!(h.height, HELPER_HEIGHT);
        assert_eq!(l.total.bottom(), h.bottom());
    }

    #[test]
    fn textarea_is_92_tall_and_text_starts_12_down() {
        let l = lay(FieldKind::Textarea, true, false);
        assert_eq!(l.box_rect.height, 92.0);
        assert_eq!(l.text.y, l.box_rect.y + TEXTAREA_PAD_TOP);
    }

    #[test]
    fn single_line_text_is_vertically_centered() {
        let l = lay(FieldKind::Text, false, false);
        let mid = l.text.y + l.text.height / 2.0;
        assert!((mid - (l.box_rect.y + 23.0)).abs() < 0.01);
        assert_eq!(l.text.x, l.box_rect.x + PAD_LEFT);
    }

    #[test]
    fn plain_text_has_no_trailing_slot_and_text_fills_box() {
        let l = lay(FieldKind::Text, false, false);
        assert!(l.trailing.is_none());
        assert_eq!(l.text.right(), l.box_rect.right() - PAD_RIGHT);
    }

    #[test]
    fn password_and_select_have_trailing_slot_at_right_edge() {
        for kind in [FieldKind::Password, FieldKind::Select] {
            let l = lay(kind, false, false);
            let t = l.trailing.unwrap();
            assert_eq!(t.width, TRAILING_ICON);
            assert_eq!(t.right(), l.box_rect.right() - PAD_RIGHT);
            assert!((t.y + t.height / 2.0 - (l.box_rect.y + 23.0)).abs() < 0.01);
            assert_eq!(l.text.right(), t.x - INNER_GAP);
        }
    }

    #[test]
    fn trailing_icons() {
        assert_eq!(FieldKind::Password.trailing_icon(false), Some(Icon::Eye));
        assert_eq!(FieldKind::Password.trailing_icon(true), Some(Icon::EyeOff));
        assert_eq!(
            FieldKind::Select.trailing_icon(false),
            Some(Icon::ChevronDown)
        );
        assert_eq!(FieldKind::Text.trailing_icon(false), None);
        assert_eq!(FieldKind::Mono.trailing_icon(false), None);
    }

    #[test]
    fn value_fonts() {
        assert_eq!(FieldKind::Text.value_font(), 15.0);
        assert_eq!(FieldKind::Mono.value_font(), 14.0);
        assert_eq!(FieldKind::Textarea.value_font(), 14.0);
    }

    #[test]
    fn ring_expands_box_by_three() {
        let l = lay(FieldKind::Text, true, false);
        assert_eq!(l.ring.x, l.box_rect.x - 3.0);
        assert_eq!(l.ring.width, l.box_rect.width + 6.0);
        assert_eq!(l.ring.height, l.box_rect.height + 6.0);
    }

    #[test]
    fn caret_follows_measured_prefix_and_clamps() {
        let l = lay(FieldKind::Text, false, false);
        let c = caret_rect(&l, FieldKind::Text, 40.0);
        assert_eq!(c.x, l.text.x + 40.0);
        assert_eq!(c.width, CARET_WIDTH);
        assert_eq!(c.y, l.text.y);
        let far = caret_rect(&l, FieldKind::Text, 9999.0);
        assert!(far.right() <= l.text.right() + 0.01);
    }

    #[test]
    fn hit_test_box_and_trailing() {
        let l = lay(FieldKind::Password, true, false);
        let t = l.trailing.unwrap();
        assert_eq!(
            hit_test(&l, FieldState::Default, t.x + 1.0, t.y + 1.0),
            Some(FieldHit::Trailing)
        );
        assert_eq!(
            hit_test(
                &l,
                FieldState::Default,
                l.box_rect.x + 5.0,
                l.box_rect.y + 5.0
            ),
            Some(FieldHit::Box)
        );
        assert_eq!(
            hit_test(
                &l,
                FieldState::Default,
                l.label.unwrap().x + 1.0,
                l.label.unwrap().y + 1.0
            ),
            None
        );
        assert_eq!(hit_test(&l, FieldState::Default, -5.0, -5.0), None);
    }

    #[test]
    fn disabled_field_ignores_hits() {
        let l = lay(FieldKind::Text, false, false);
        assert_eq!(
            hit_test(
                &l,
                FieldState::Disabled,
                l.box_rect.x + 5.0,
                l.box_rect.y + 5.0
            ),
            None
        );
    }

    #[test]
    fn border_colours_per_state() {
        let th = ChromeTheme::default();
        assert_eq!(border_color(&th, FieldState::Default), th.line);
        assert_eq!(border_color(&th, FieldState::Filled), th.line);
        assert_eq!(border_color(&th, FieldState::Hover), th.hover_border);
        assert_eq!(border_color(&th, FieldState::Focus), th.accent);
        assert_eq!(border_color(&th, FieldState::Error), th.danger_fill);
        assert_eq!(border_color(&th, FieldState::Disabled), th.divider);
    }

    #[test]
    fn mask_is_bullets() {
        assert_eq!(mask(3), "\u{2022}\u{2022}\u{2022}");
        assert_eq!(mask(0), "");
    }

    #[test]
    fn search_box_geometry() {
        let s = search_layout((0.0, 0.0), 300.0, SearchKind::Search, 0.0);
        assert_eq!(s.box_rect.height, 40.0);
        assert_eq!(s.icon.x, SEARCH_PAD);
        assert_eq!(s.icon.width, SEARCH_ICON);
        assert_eq!(s.text.x, SEARCH_PAD + SEARCH_ICON + INNER_GAP);
        assert!(s.hint.is_none());
        assert_eq!(s.text.right(), 300.0 - SEARCH_PAD);
    }

    #[test]
    fn command_bar_geometry_has_right_aligned_hint() {
        let s = search_layout((0.0, 0.0), 300.0, SearchKind::CommandBar, 40.0);
        assert_eq!(s.box_rect.height, 38.0);
        let h = s.hint.unwrap();
        assert_eq!(h.width, 40.0);
        assert_eq!(h.right(), 300.0 - COMMAND_PAD);
        assert_eq!(s.icon.x, COMMAND_PAD);
        assert_eq!(s.text.right(), h.x - INNER_GAP);
    }

    #[test]
    fn port_pair_widths_and_alignment() {
        let p = port_pair_layout((5.0, 7.0));
        assert_eq!(p.local.box_rect.width, 140.0);
        assert_eq!(p.dest.box_rect.width, 320.0);
        assert_eq!(p.port.box_rect.width, 110.0);
        assert_eq!(p.local.box_rect.x, 5.0);
        assert_eq!(p.arrow.x, p.local.box_rect.right() + PORT_GAP);
        assert_eq!(p.dest.box_rect.x, p.arrow.right() + PORT_GAP);
        assert_eq!(p.port.box_rect.x, p.dest.box_rect.right() + PORT_GAP);
        // arrow is vertically centered on the boxes
        assert_eq!(p.arrow.y, p.local.box_rect.y);
        assert_eq!(p.arrow.height, 46.0);
        assert_eq!(p.dest.box_rect.y, p.port.box_rect.y);
        assert_eq!(p.total.right(), p.port.box_rect.right());
    }

    #[test]
    fn grid_cells_three_columns() {
        let c0 = grid_cell((0.0, 0.0), 1000.0, 0, 100.0);
        let c1 = grid_cell((0.0, 0.0), 1000.0, 1, 100.0);
        let c3 = grid_cell((0.0, 0.0), 1000.0, 3, 100.0);
        let w = (1000.0 - 2.0 * GRID_COL_GAP) / 3.0;
        assert!((c0.width - w).abs() < 0.01);
        assert!((c1.x - (w + GRID_COL_GAP)).abs() < 0.01);
        assert_eq!(c3.x, 0.0);
        assert_eq!(c3.y, 100.0 + GRID_ROW_GAP);
    }
}

#[cfg(test)]
mod draft_tests {
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
    fn apply_routes_every_edit_the_same_way() {
        let mut d = TextDraft::new("abc");
        assert!(d.apply(TextEdit::Backspace { by_word: false }));
        assert_eq!(d.value, "ab");
        assert!(d.apply(TextEdit::Home {
            kind: TextMoveKind::Collapse
        }));
        assert!(d.apply(TextEdit::Delete { by_word: false }));
        assert_eq!(d.value, "b");
        assert!(d.apply(TextEdit::SelectAll));
        assert!(d.apply(TextEdit::Backspace { by_word: false }));
        assert_eq!(d.value, "");
        assert!(!d.apply(TextEdit::Backspace { by_word: false }));
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
