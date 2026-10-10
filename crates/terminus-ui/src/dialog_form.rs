//! A generic dialog form component extracted from `add_host.rs`.
//! Supports dynamic text fields while strictly retaining the exact visual layout
//! (paddings, corner radii, and button placements) of the original Host modal.

use crate::components::input::{TextDraft, TextMoveKind};
use crate::components::overlay::action_row;
use crate::geom::Rect;

pub const WIDTH: f32 = 520.0;
pub const PAD: f32 = 28.0;
pub const TITLE_HEIGHT: f32 = 28.0;
/// Gap between the title and the first field, and between blocks.
pub const BLOCK_GAP: f32 = 18.0;
/// Label (16) + gap (8) above the 46px input.
pub const LABEL_BLOCK: f32 = 24.0;
pub const INPUT_HEIGHT: f32 = 46.0;
/// A whole labelled field: label, gap and input.
pub const FIELD_HEIGHT: f32 = LABEL_BLOCK + INPUT_HEIGHT;
pub const FIELD_GAP: f32 = BLOCK_GAP;
pub const ERROR_HEIGHT: f32 = 16.0;
/// Extra room above the buttons (mock `padding-top: 6px`).
pub const BUTTONS_TOP: f32 = 6.0;
/// Corner radius of the dialog (design token `radius.dialog`).
pub const DIALOG_RADIUS: f32 = 18.0;
pub const BUTTON_HEIGHT: f32 = 44.0;
pub const BUTTON_GAP: f32 = 10.0;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormFieldData {
    pub key: String,
    pub label: String,
    pub draft: TextDraft,
    /// Shown faintly while the field is empty.
    pub placeholder: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DynamicFormState {
    pub title: String,
    pub fields: Vec<FormFieldData>,
    pub focused_index: usize,
    pub error: Option<String>,
    pub closing: bool,
    pub save_label: String,
    /// Save / Cancel under the pointer (visual hover only).
    pub btn_hover: Option<DynamicFormHit>,
}

impl DynamicFormState {
    pub fn new(title: impl Into<String>, save_label: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            fields: Vec::new(),
            focused_index: 0,
            error: None,
            // Closed until the caller clears this (e.g. Add snippet CTA).
            closing: true,
            save_label: save_label.into(),
            btn_hover: None,
        }
    }

    pub fn with_field(mut self, key: &str, label: &str, initial_value: &str) -> Self {
        let mut draft = TextDraft::new(initial_value.to_string());
        draft.caret = initial_value.chars().count();
        self.fields.push(FormFieldData {
            key: key.to_string(),
            label: label.to_string(),
            draft,
            placeholder: String::new(),
        });
        self
    }

    /// Placeholder of the field added last.
    pub fn with_placeholder(mut self, text: &str) -> Self {
        if let Some(f) = self.fields.last_mut() {
            f.placeholder = text.to_string();
        }
        self
    }

    pub fn is_open(&self) -> bool {
        !self.closing
    }

    pub fn set_error(&mut self, err: String) {
        self.error = Some(err);
    }

    pub fn get_value(&self, key: &str) -> Option<&str> {
        self.fields
            .iter()
            .find(|f| f.key == key)
            .map(|f| f.draft.value.as_str())
    }

    pub fn cycle_focus(&mut self, reverse: bool) {
        if self.fields.is_empty() {
            return;
        }
        if reverse {
            self.focused_index =
                (self.focused_index + self.fields.len() - 1) % self.fields.len();
        } else {
            self.focused_index = (self.focused_index + 1) % self.fields.len();
        }
        if let Some(draft) = self.focused_draft_mut() {
            draft.sel_anchor = None;
        }
    }

    pub fn focused_draft_mut(&mut self) -> Option<&mut TextDraft> {
        let i = self.focused_index;
        self.fields.get_mut(i).map(|f| &mut f.draft)
    }

    pub fn focused_draft(&self) -> Option<&TextDraft> {
        self.fields.get(self.focused_index).map(|f| &f.draft)
    }

    /// Insert printable text (spaces included) into the focused field.
    pub fn insert_text(&mut self, text: &str) -> bool {
        let Some(draft) = self.focused_draft_mut() else {
            return false;
        };
        if draft.insert(text, usize::MAX, false) {
            self.error = None;
            true
        } else {
            false
        }
    }

    pub fn backspace(&mut self, by_word: bool) -> bool {
        let Some(draft) = self.focused_draft_mut() else {
            return false;
        };
        if draft.backspace(by_word) {
            self.error = None;
            true
        } else {
            false
        }
    }

    pub fn delete_forward(&mut self, by_word: bool) -> bool {
        let Some(draft) = self.focused_draft_mut() else {
            return false;
        };
        if draft.delete_forward(by_word) {
            self.error = None;
            true
        } else {
            false
        }
    }

    pub fn move_left(&mut self, kind: TextMoveKind, by_word: bool) -> bool {
        self.focused_draft_mut()
            .is_some_and(|d| d.move_left(kind, by_word))
    }

    pub fn move_right(&mut self, kind: TextMoveKind, by_word: bool) -> bool {
        self.focused_draft_mut()
            .is_some_and(|d| d.move_right(kind, by_word))
    }

    pub fn move_home(&mut self, kind: TextMoveKind) -> bool {
        self.focused_draft_mut().is_some_and(|d| d.move_home(kind))
    }

    pub fn move_end(&mut self, kind: TextMoveKind) -> bool {
        self.focused_draft_mut().is_some_and(|d| d.move_end(kind))
    }

    pub fn select_all(&mut self) -> bool {
        self.focused_draft_mut().is_some_and(|d| d.select_all())
    }

    /// Legacy single-char insert path (kept for older call sites).
    pub fn handle_key(&mut self, key: &str) {
        let _ = self.insert_text(key);
    }

    pub fn handle_backspace(&mut self) {
        let _ = self.backspace(false);
    }

    pub fn handle_delete(&mut self) {
        let _ = self.delete_forward(false);
    }

    pub fn move_caret(&mut self, offset: isize) {
        if offset < 0 {
            for _ in 0..offset.abs() {
                let _ = self.move_left(TextMoveKind::Collapse, false);
            }
        } else {
            for _ in 0..offset.abs() {
                let _ = self.move_right(TextMoveKind::Collapse, false);
            }
        }
    }
}

pub struct DialogFormLayout {
    pub dialog: Rect,
    pub title: Rect,
    pub fields: Vec<Rect>,
    pub error_line: Option<Rect>,
    pub cancel_btn: Rect,
    pub save_btn: Rect,
}

impl DialogFormLayout {
    pub fn compute(
        form: &DynamicFormState,
        window_width: f32,
        window_height: f32,
    ) -> Self {
        let n = form.fields.len() as f32;
        let mut height = PAD + TITLE_HEIGHT;
        if n > 0.0 {
            height += BLOCK_GAP + n * FIELD_HEIGHT + (n - 1.0) * FIELD_GAP;
        }
        if form.error.is_some() {
            height += BLOCK_GAP + ERROR_HEIGHT;
        }
        height += BLOCK_GAP + BUTTONS_TOP + BUTTON_HEIGHT + PAD;

        let x = ((window_width - WIDTH).max(0.0) / 2.0).round();
        let y = ((window_height - height).max(0.0) / 2.0).round();
        let dialog = Rect::new(x, y, WIDTH, height);
        let title = Rect::new(x + PAD, y + PAD, WIDTH - 2.0 * PAD, TITLE_HEIGHT);

        let mut fields = Vec::with_capacity(form.fields.len());
        let mut cy = title.bottom() + BLOCK_GAP;
        for _ in 0..form.fields.len() {
            fields.push(Rect::new(x + PAD, cy, WIDTH - 2.0 * PAD, FIELD_HEIGHT));
            cy += FIELD_HEIGHT + FIELD_GAP;
        }
        // `cy` overshoots by one field gap after the last field.
        let error_y = if fields.is_empty() {
            title.bottom() + BLOCK_GAP
        } else {
            cy - FIELD_GAP + BLOCK_GAP
        };

        let error_line = form
            .error
            .is_some()
            .then(|| Rect::new(x + PAD, error_y, WIDTH - 2.0 * PAD, ERROR_HEIGHT));

        let save_w = crate::vault_unlock::button_width(&form.save_label);
        let cancel_w = crate::vault_unlock::button_width("Cancel");
        let (cancel_btn, save_btn) =
            action_row(&dialog, PAD, BUTTON_HEIGHT, BUTTON_GAP, cancel_w, save_w);
        Self {
            dialog,
            title,
            fields,
            error_line,
            cancel_btn,
            save_btn,
        }
    }

    /// The 46px input box of field `i` (below its label).
    pub fn input_rect(&self, i: usize) -> Rect {
        let f = self.fields[i];
        Rect::new(f.x, f.y + LABEL_BLOCK, f.width, INPUT_HEIGHT)
    }

    pub fn hit_test(&self, x: f32, y: f32) -> Option<DynamicFormHit> {
        if !self.dialog.contains(x, y) {
            return Some(DynamicFormHit::Background);
        }
        for (i, r) in self.fields.iter().enumerate() {
            if r.contains(x, y) {
                return Some(DynamicFormHit::Field(i));
            }
        }
        if self.save_btn.contains(x, y) {
            return Some(DynamicFormHit::Save);
        }
        if self.cancel_btn.contains(x, y) {
            return Some(DynamicFormHit::Cancel);
        }
        Some(DynamicFormHit::Background)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DynamicFormHit {
    Field(usize),
    Save,
    Cancel,
    Background,
}

#[cfg(test)]
mod layout_tests {
    use super::*;

    fn form() -> DynamicFormState {
        DynamicFormState::new("New snippet", "Save snippet")
            .with_field("name", "Title", "")
            .with_placeholder("Restart the web server")
            .with_field("command", "Command", "")
            .with_field("tags", "Description", "")
    }

    #[test]
    fn follows_the_design_metrics() {
        let l = DialogFormLayout::compute(&form(), 1440.0, 900.0);
        assert_eq!(l.dialog.width, 520.0);
        assert_eq!(l.fields[0].height, 70.0);
        assert_eq!(l.input_rect(0).height, 46.0);
        assert_eq!(l.fields[1].y - l.fields[0].bottom(), 18.0);
        assert_eq!(l.save_btn.height, 44.0);
        assert!((l.save_btn.right() - (l.dialog.right() - 28.0)).abs() < 0.01);
        assert!((l.save_btn.bottom() - (l.dialog.bottom() - 28.0)).abs() < 0.01);
        assert_eq!(l.save_btn.x - l.cancel_btn.right(), 10.0);
        assert!(l.fields[2].bottom() < l.cancel_btn.y);
        assert_eq!(form().fields[0].placeholder, "Restart the web server");
    }

    #[test]
    fn an_error_line_grows_the_dialog_and_stays_above_the_buttons() {
        let plain = DialogFormLayout::compute(&form(), 1440.0, 900.0);
        let mut f = form();
        f.set_error("Name is required".into());
        let l = DialogFormLayout::compute(&f, 1440.0, 900.0);
        assert_eq!(
            l.dialog.height - plain.dialog.height,
            BLOCK_GAP + ERROR_HEIGHT
        );
        let e = l.error_line.unwrap();
        assert!(e.y >= l.fields[2].bottom() && e.bottom() <= l.save_btn.y);
    }

    #[test]
    fn hit_test_finds_fields_and_buttons() {
        let l = DialogFormLayout::compute(&form(), 1440.0, 900.0);
        let i = l.input_rect(1);
        assert_eq!(
            l.hit_test(i.x + 4.0, i.y + 4.0),
            Some(DynamicFormHit::Field(1))
        );
        let s = l.save_btn;
        assert_eq!(l.hit_test(s.x + 2.0, s.y + 2.0), Some(DynamicFormHit::Save));
        assert_eq!(l.hit_test(1.0, 1.0), Some(DynamicFormHit::Background));
    }
}
