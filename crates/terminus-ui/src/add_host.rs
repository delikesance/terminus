//! The add-host editor: a small form with one focused field.
//!
//! The form owns the text, the focus and the caret, and nothing else —
//! no validation and no storage. Submitting hands the raw values to the
//! host repository, which is the single place that decides whether a
//! host is storable; a rejection comes back as [`AddHostForm::set_error`]
//! so the form stays open with its text intact.

use crate::geom::Rect;

/// The fields, in tab order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Name,
    Hostname,
    Username,
    Port,
}

pub const FIELDS: [Field; 4] =
    [Field::Name, Field::Hostname, Field::Username, Field::Port];

impl Field {
    pub const fn label(self) -> &'static str {
        match self {
            Field::Name => "Host Name / Label",
            Field::Hostname => "IP Address or Hostname",
            Field::Username => "Username",
            Field::Port => "Port",
        }
    }

    pub const fn placeholder(self) -> &'static str {
        match self {
            Field::Name => "e.g. AWS Production Cluster",
            Field::Hostname => "192.168.1.50",
            Field::Username => "root",
            Field::Port => "22",
        }
    }

    pub const fn index(self) -> usize {
        match self {
            Field::Name => 0,
            Field::Hostname => 1,
            Field::Username => 2,
            Field::Port => 3,
        }
    }

    pub const fn from_index(index: usize) -> Self {
        FIELDS[index % FIELDS.len()]
    }
}

/// Dialog metrics, in logical pixels (`max-w-md` ≈ 448).
pub const WIDTH: f32 = 448.0;
/// Height of one field row: a caption plus its input box.
pub const FIELD_HEIGHT: f32 = 52.0;
/// Top of the input box inside a field row.
pub const INPUT_TOP: f32 = 18.0;
/// Height of the input box itself.
pub const INPUT_HEIGHT: f32 = 32.0;
pub const FIELD_GAP: f32 = 12.0;
pub const PAD: f32 = 24.0;
pub const TITLE_HEIGHT: f32 = 28.0;
pub const HINT_HEIGHT: f32 = 44.0;
/// Corner radius (`rounded-2xl`).
pub const DIALOG_RADIUS: f32 = 16.0;
/// Input corner radius (`rounded-xl`).
pub const INPUT_RADIUS: f32 = 12.0;

impl AddHostForm {
    /// Total dialog height, including a hint/error line.
    pub fn height(&self) -> f32 {
        PAD + TITLE_HEIGHT
            + FIELDS.len() as f32 * FIELD_HEIGHT
            + (FIELDS.len() - 1) as f32 * FIELD_GAP
            + PAD
            + HINT_HEIGHT
    }
}

/// What the form collected.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct HostFormValues {
    pub name: String,
    pub hostname: String,
    pub username: String,
    pub port: String,
}

/// One keyboard input, platform-neutral.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormInput {
    /// Committed text: a single keystroke's characters, or an IME commit.
    Text,
    Backspace,
    Delete,
    /// Tab, or the down arrow: next field.
    Next,
    /// Shift+Tab, or the up arrow: previous field.
    Previous,
    Left,
    Right,
    Home,
    End,
    Enter,
    Escape,
}

/// What the caller must do after an input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormOutcome {
    /// The form swallowed the input; repaint if the caret moved.
    Consumed,
    /// Dismiss without saving.
    Cancel,
    /// Save, then close only if the repository accepts the values.
    Submit,
}

/// The add-host editor's state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddHostForm {
    open: bool,
    values: [String; 4],
    /// Caret position, in characters (not bytes), per field.
    carets: [usize; 4],
    focus: usize,
    error: Option<String>,
}

impl Default for AddHostForm {
    fn default() -> Self {
        Self {
            open: false,
            values: Default::default(),
            carets: [0; 4],
            focus: 0,
            error: None,
        }
    }
}

impl AddHostForm {
    /// Show the form, empty, focused on the first field.
    ///
    /// Always a fresh form: a half-typed host from a previous attempt
    /// reappearing unasked is worse than retyping two fields.
    pub fn open(&mut self) {
        self.values = Default::default();
        self.carets = [0; 4];
        self.focus = 0;
        self.error = None;
        self.open = true;
    }

    pub fn close(&mut self) {
        self.open = false;
        self.error = None;
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// Rejection from the repository, shown under the fields.
    pub fn set_error(&mut self, message: impl Into<String>) {
        self.error = Some(message.into());
    }

    pub fn focused_field(&self) -> Field {
        Field::from_index(self.focus)
    }

    pub fn focus(&self) -> usize {
        self.focus
    }

    pub fn value(&self, field: Field) -> &str {
        &self.values[field.index()]
    }

    pub fn cursor(&self, field: Field) -> usize {
        self.carets[field.index()]
    }

    /// The caret's byte offset inside `field`'s value.
    pub fn cursor_byte(&self, field: Field) -> usize {
        let value = self.value(field);
        let chars = self.cursor(field).min(value.chars().count());
        value
            .char_indices()
            .nth(chars)
            .map(|(byte, _)| byte)
            .unwrap_or(value.len())
    }

    /// Text before / after the caret, for painting the caret inline.
    pub fn split_at_cursor(&self) -> (&str, &str) {
        let value = self.value(self.focused_field());
        let byte = self.cursor_byte(self.focused_field());
        value.split_at(byte)
    }

    pub fn values(&self) -> HostFormValues {
        HostFormValues {
            name: self.values[0].clone(),
            hostname: self.values[1].clone(),
            username: self.values[2].clone(),
            port: self.values[3].clone(),
        }
    }

    /// Insert text at the caret. Empty or control-bearing text is
    /// rejected, the same policy every other text sink in rio applies.
    pub fn insert(&mut self, text: &str) -> bool {
        if text.is_empty() || text.chars().any(char::is_control) {
            return false;
        }
        let field = self.focused_field();
        let byte = self.cursor_byte(field);
        self.values[field.index()].insert_str(byte, text);
        self.carets[field.index()] += text.chars().count();
        self.error = None;
        true
    }

    /// Delete the character before the caret. Returns whether anything
    /// changed (a backspace at offset 0 must not be reported as an edit,
    /// or the caller repaints on every stray keypress).
    pub fn backspace(&mut self) -> bool {
        let field = self.focused_field();
        let chars = self.cursor(field);
        if chars == 0 {
            return false;
        }
        let value = &mut self.values[field.index()];
        let start = char_byte_offset(value, chars - 1);
        let end = char_byte_offset(value, chars);
        value.replace_range(start..end, "");
        self.carets[field.index()] = chars - 1;
        self.error = None;
        true
    }

    /// Delete the character after the caret.
    pub fn delete(&mut self) -> bool {
        let field = self.focused_field();
        let chars = self.cursor(field);
        let len = self.value(field).chars().count();
        if chars >= len {
            return false;
        }
        let value = &mut self.values[field.index()];
        let start = char_byte_offset(value, chars);
        let end = char_byte_offset(value, chars + 1);
        value.replace_range(start..end, "");
        self.error = None;
        true
    }

    pub fn move_cursor(&mut self, delta: isize) {
        let field = self.focused_field();
        let len = self.value(field).chars().count();
        let next = (self.carets[field.index()] as isize + delta).clamp(0, len as isize);
        self.carets[field.index()] = next as usize;
    }

    pub fn cursor_home(&mut self) {
        let field = self.focused_field();
        self.carets[field.index()] = 0;
    }

    pub fn cursor_end(&mut self) {
        let field = self.focused_field();
        self.carets[field.index()] = self.value(field).chars().count();
    }

    /// Move focus `delta` fields forward, wrapping.
    pub fn focus_by(&mut self, delta: isize) {
        let len = FIELDS.len() as isize;
        self.focus = (((self.focus as isize + delta) % len + len) % len) as usize;
        self.error = None;
    }

    /// Route one input. `text` is only read for [`FormInput::Text`].
    pub fn handle_input(&mut self, input: FormInput, text: &str) -> FormOutcome {
        use FormOutcome::{Cancel, Consumed, Submit};
        match input {
            FormInput::Text => {
                if self.insert(text) {
                    Consumed
                } else {
                    // An open text sink swallows even rejected text,
                    // so a stray control character never reaches the PTY.
                    Consumed
                }
            }
            FormInput::Backspace => {
                self.backspace();
                Consumed
            }
            FormInput::Delete => {
                self.delete();
                Consumed
            }
            FormInput::Next => {
                self.focus_by(1);
                Consumed
            }
            FormInput::Previous => {
                self.focus_by(-1);
                Consumed
            }
            FormInput::Left => {
                self.move_cursor(-1);
                Consumed
            }
            FormInput::Right => {
                self.move_cursor(1);
                Consumed
            }
            FormInput::Home => {
                self.cursor_home();
                Consumed
            }
            FormInput::End => {
                self.cursor_end();
                Consumed
            }
            FormInput::Enter => Submit,
            FormInput::Escape => Cancel,
        }
    }
}

/// Byte offset of the `chars`-th character (or the length).
fn char_byte_offset(value: &str, chars: usize) -> usize {
    value
        .char_indices()
        .nth(chars)
        .map(|(byte, _)| byte)
        .unwrap_or(value.len())
}

/// Screen geometry of the dialog.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AddHostLayout {
    /// Top-left of the whole dialog.
    pub x: f32,
    pub y: f32,
}

impl AddHostLayout {
    /// Center the dialog in a `width` x `height` window, in logical pixels.
    pub fn centered(width: f32, height: f32, dialog_height: f32) -> Self {
        Self {
            x: ((width - WIDTH) / 2.0).max(0.0),
            y: ((height - dialog_height) / 2.0).max(0.0),
        }
    }

    pub fn rect(&self, dialog_height: f32) -> Rect {
        Rect::new(self.x, self.y, WIDTH, dialog_height)
    }

    pub fn title_rect(&self) -> Rect {
        Rect::new(self.x + PAD, self.y + PAD, WIDTH - 2.0 * PAD, TITLE_HEIGHT)
    }

    /// The input box of `field`.
    pub fn field_rect(&self, field: Field) -> Rect {
        let top = self.y
            + PAD
            + TITLE_HEIGHT
            + field.index() as f32 * (FIELD_HEIGHT + FIELD_GAP);
        Rect::new(self.x + PAD, top, WIDTH - 2.0 * PAD, FIELD_HEIGHT)
    }

    /// The bordered input box inside a field row, below its caption.
    pub fn input_rect(&self, field: Field) -> Rect {
        let row = self.field_rect(field);
        Rect::new(row.x, row.y + INPUT_TOP, row.width, INPUT_HEIGHT)
    }

    /// The caption line above a field's input box.
    pub fn caption_rect(&self, field: Field) -> Rect {
        let row = self.field_rect(field);
        Rect::new(row.x + 2.0, row.y, row.width - 4.0, INPUT_TOP)
    }

    /// The hint / error line under the fields.
    pub fn hint_rect(&self, dialog_height: f32) -> Rect {
        Rect::new(
            self.x + PAD,
            self.y + dialog_height - PAD - HINT_HEIGHT,
            WIDTH - 2.0 * PAD,
            HINT_HEIGHT,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn open_form() -> AddHostForm {
        let mut form = AddHostForm::default();
        form.open();
        form
    }

    fn type_into(form: &mut AddHostForm, text: &str) {
        for ch in text.chars() {
            let buf = ch.to_string();
            form.handle_input(FormInput::Text, &buf);
        }
    }

    #[test]
    fn typing_lands_in_the_focused_field() {
        let mut form = open_form();
        type_into(&mut form, "web-01");
        assert_eq!(form.value(Field::Name), "web-01");
        assert_eq!(form.value(Field::Hostname), "");
        assert_eq!(form.cursor(Field::Name), 6);

        form.handle_input(FormInput::Next, "");
        type_into(&mut form, "example.com");
        assert_eq!(form.focused_field(), Field::Hostname);
        assert_eq!(form.value(Field::Hostname), "example.com");
        assert_eq!(form.value(Field::Name), "web-01");
    }

    #[test]
    fn tab_wraps_around_the_fields() {
        let mut form = open_form();
        for _ in 0..FIELDS.len() {
            form.handle_input(FormInput::Next, "");
        }
        assert_eq!(form.focused_field(), Field::Name);

        form.handle_input(FormInput::Previous, "");
        assert_eq!(form.focused_field(), Field::Port);
    }

    #[test]
    fn backspace_edits_at_the_caret_not_the_tail() {
        let mut form = open_form();
        type_into(&mut form, "web-99");
        form.handle_input(FormInput::Left, "");
        form.handle_input(FormInput::Left, "");
        // Caret now sits after "web", so backspace eats the '-'.
        assert!(form.backspace());
        assert_eq!(form.value(Field::Name), "web99");
        assert_eq!(form.cursor(Field::Name), 3);
    }

    #[test]
    fn backspace_at_the_beginning_is_not_an_edit() {
        let mut form = open_form();
        type_into(&mut form, "abc");
        form.handle_input(FormInput::Home, "");
        assert!(!form.backspace());
        assert_eq!(form.value(Field::Name), "abc");
    }

    #[test]
    fn delete_removes_the_character_after_the_caret() {
        let mut form = open_form();
        type_into(&mut form, "abc");
        form.handle_input(FormInput::Home, "");
        assert!(form.delete());
        assert_eq!(form.value(Field::Name), "bc");
        assert_eq!(form.cursor(Field::Name), 0);
        // Caret already at the end: nothing left to delete.
        form.cursor_end();
        assert!(!form.delete());
    }

    #[test]
    fn insert_splices_at_the_caret() {
        let mut form = open_form();
        type_into(&mut form, "web01");
        form.handle_input(FormInput::Left, "");
        form.handle_input(FormInput::Left, "");
        assert!(form.insert("-"));
        assert_eq!(form.value(Field::Name), "web-01");
        assert_eq!(form.cursor(Field::Name), 4);
    }

    #[test]
    fn carets_are_character_indices_not_byte_offsets() {
        let mut form = open_form();
        // Multi-byte characters must not split a code point.
        type_into(&mut form, "café");
        assert_eq!(form.value(Field::Name), "café");
        assert!(form.backspace());
        assert_eq!(form.value(Field::Name), "caf");
        form.handle_input(FormInput::Home, "");
        assert_eq!(form.cursor_byte(Field::Name), 0);
        assert!(form.insert("é"));
        assert_eq!(form.value(Field::Name), "écaf");
    }

    #[test]
    fn split_at_cursor_bounds_the_painted_caret() {
        let mut form = open_form();
        type_into(&mut form, "web");
        form.handle_input(FormInput::Left, "");
        assert_eq!(form.split_at_cursor(), ("we", "b"));
    }

    #[test]
    fn control_and_empty_text_is_rejected_outright() {
        let mut form = open_form();
        assert!(!form.insert(""));
        assert!(!form.insert("\u{1b}"));
        assert!(!form.insert("a\nb"));
        assert_eq!(form.value(Field::Name), "");
        // But the sink still swallows the keystroke.
        assert_eq!(
            form.handle_input(FormInput::Text, "\u{7f}"),
            FormOutcome::Consumed
        );
    }

    #[test]
    fn enter_submits_and_escape_cancels() {
        let mut form = open_form();
        assert_eq!(form.handle_input(FormInput::Enter, ""), FormOutcome::Submit);
        assert_eq!(
            form.handle_input(FormInput::Escape, ""),
            FormOutcome::Cancel
        );
    }

    #[test]
    fn a_rejection_keeps_the_form_open_with_its_text() {
        let mut form = open_form();
        type_into(&mut form, "web-01");
        form.set_error("'abc' is not a valid port");
        assert!(form.is_open());
        assert_eq!(form.value(Field::Name), "web-01");
        assert_eq!(form.error(), Some("'abc' is not a valid port"));

        // Editing clears the stale message.
        form.handle_input(FormInput::Text, "2");
        assert_eq!(form.error(), None);
    }

    #[test]
    fn opening_always_starts_from_an_empty_form() {
        let mut form = open_form();
        type_into(&mut form, "stale");
        form.set_error("boom");
        form.open();
        assert_eq!(form.values(), HostFormValues::default());
        assert_eq!(form.focused_field(), Field::Name);
        assert_eq!(form.error(), None);
    }

    #[test]
    fn closing_clears_the_error() {
        let mut form = open_form();
        form.set_error("boom");
        form.close();
        assert!(!form.is_open());
        assert_eq!(form.error(), None);
    }

    #[test]
    fn values_round_trip_every_field() {
        let mut form = open_form();
        type_into(&mut form, "web-01");
        form.focus_by(1);
        type_into(&mut form, "web-01.example.com");
        form.focus_by(1);
        type_into(&mut form, "deploy");
        form.focus_by(1);
        type_into(&mut form, "2222");

        assert_eq!(
            form.values(),
            HostFormValues {
                name: "web-01".to_string(),
                hostname: "web-01.example.com".to_string(),
                username: "deploy".to_string(),
                port: "2222".to_string(),
            }
        );
    }

    #[test]
    fn fields_are_laid_out_inside_the_dialog_and_do_not_overlap() {
        let form = open_form();
        let layout = AddHostLayout::centered(1200.0, 800.0, form.height());
        let dialog = layout.rect(form.height());

        for field in FIELDS {
            let rect = layout.field_rect(field);
            assert!(rect.x >= dialog.x && rect.right() <= dialog.right());
            assert!(rect.y >= dialog.y && rect.bottom() <= dialog.bottom());
        }

        for pair in FIELDS.windows(2) {
            let a = layout.field_rect(pair[0]);
            let b = layout.field_rect(pair[1]);
            assert!(a.bottom() <= b.y, "{:?} overlaps {:?}", pair[0], pair[1]);
        }

        // The hint line sits below the last field, inside the dialog.
        let hint = layout.hint_rect(form.height());
        assert!(hint.y >= layout.field_rect(Field::Port).bottom());
        assert!(hint.bottom() <= dialog.bottom());
    }

    #[test]
    fn the_dialog_is_centered() {
        let form = open_form();
        let layout = AddHostLayout::centered(1200.0, 800.0, form.height());
        let dialog = layout.rect(form.height());
        assert!((dialog.x - (1200.0 - WIDTH) / 2.0).abs() < f32::EPSILON);
        assert!((dialog.y - (800.0 - form.height()) / 2.0).abs() < f32::EPSILON);
    }

    #[test]
    fn a_window_smaller_than_the_dialog_does_not_go_negative() {
        let form = open_form();
        let layout = AddHostLayout::centered(100.0, 60.0, form.height());
        assert_eq!(layout.x, 0.0);
        assert_eq!(layout.y, 0.0);
    }
}
